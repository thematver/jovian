import Foundation

/// The Jup-8 V effects section: three slots in series or parallel, each running one of eleven
/// effects. Parameters are read as normalized preset values (see `ParamTable`).
///
/// - FX_Enable 0 passes the input through (10 ms crossfade; tails are cleared on re-enable).
/// - FX_Routing 0 "Serie": FX1 -> FX2 -> FX3. 1 "Parallel": every slot gets the dry input and
///   the active slots' outputs are averaged, so a single active slot sounds as in series.
/// - FXn_Bypass 1 ("On") = slot active. Type and bypass changes fade the slot out, reset the new
///   effect and fade it in (10 ms each way).
/// - FXn_Dry_Wet: out = dry·(1 - w) + wet·w, smoothed (for Param EQ it scales the band gains instead).
/// - `mod[0...2]` offset FXn_Dry_Wet, `mod[3...5]` offset the main parameter of each slot's effect
///   (the knob Jup-8 V draws as modulable): Reverb Decay, Delay Time (synced or ms), Chorus
///   Feedback, Flanger Rate, Phaser Rate (synced or Hz), Overdrive Drive, Compressor Release,
///   BitCrusher Bit Depth, Filter Cutoff (Comb Freq in comb modes), EQ Scale, Stereo Pan Amount.
public final class FXRack {
    private let pEnable = FXParam("FX_Enable"), pRouting = FXParam("FX_Routing")
    private let s0 = FXSlot(slot: 0), s1 = FXSlot(slot: 1), s2 = FXSlot(slot: 2)
    private let scratch: UnsafeMutablePointer<Float>
    private let dryL: UnsafeMutablePointer<Float>, dryR: UnsafeMutablePointer<Float>
    private let accL: UnsafeMutablePointer<Float>, accR: UnsafeMutablePointer<Float>
    private let tmpL: UnsafeMutablePointer<Float>, tmpR: UnsafeMutablePointer<Float>
    private var sampleRate = 0.0
    private var enableGain: Float = 0, enableStep: Float = 0.002
    private var fresh = true, idle = false

    public init() {
        scratch = .allocate(capacity: 6 * fxChunk)
        scratch.initialize(repeating: 0, count: 6 * fxChunk)
        dryL = scratch; dryR = scratch + fxChunk
        accL = scratch + 2 * fxChunk; accR = scratch + 3 * fxChunk
        tmpL = scratch + 4 * fxChunk; tmpR = scratch + 5 * fxChunk
    }

    deinit { scratch.deallocate() }

    /// Allocates all buffers (off the render thread). Processing runs in 32-sample chunks, so any
    /// block size works; `maxFrames` is accepted for API symmetry.
    public func prepare(sampleRate: Double, maxFrames: Int) {
        self.sampleRate = sampleRate
        enableStep = 1 / Float(0.01 * sampleRate)
        s0.prepare(sampleRate: sampleRate); s1.prepare(sampleRate: sampleRate); s2.prepare(sampleRate: sampleRate)
        reset()
    }

    /// Clears every delay line and filter state; no allocation.
    public func reset() {
        s0.reset(); s1.reset(); s2.reset()
        fresh = true
        idle = false
    }

    /// Gain reduction in dB (>= 0) of the compressor running in `slot` (0...2), for the
    /// FXn_Compressor_OutGainReduction meter; 0 when that slot is not a compressor.
    public func compressorReductionDB(slot: Int) -> Float {
        switch slot {
        case 0: return s0.compressorReductionDB
        case 1: return s1.compressorReductionDB
        case 2: return s2.compressorReductionDB
        default: return 0
        }
    }

    /// Render-thread safe: processes the stereo signal in place.
    public func process(left: UnsafeMutablePointer<Float>, right: UnsafeMutablePointer<Float>, frames: Int,
                        params: UnsafePointer<Float>, tempo: Double, mod: UnsafePointer<Float>?) {
        guard sampleRate > 0, frames > 0 else { return }
        let on = pEnable.index(params) == 1
        if fresh { enableGain = on ? 1 : 0 }
        fresh = false
        if !on && enableGain <= 0 { idle = true; return }
        if idle {
            s0.reset(); s1.reset(); s2.reset()
            idle = false
        }
        let bpm = tempo.isFinite && tempo >= 1 ? min(tempo, 999) : 120
        let parallel = pRouting.index(params) == 1
        s0.begin(params, tempo: bpm, mixMod: mod?[0] ?? 0, mainMod: mod?[3] ?? 0)
        s1.begin(params, tempo: bpm, mixMod: mod?[1] ?? 0, mainMod: mod?[4] ?? 0)
        s2.begin(params, tempo: bpm, mixMod: mod?[2] ?? 0, mainMod: mod?[5] ?? 0)

        let target: Float = on ? 1 : 0
        var pos = 0
        while pos < frames {
            let n = min(fxChunk, frames - pos)
            let l = left + pos, r = right + pos
            dryL.update(from: l, count: n)
            dryR.update(from: r, count: n)
            if parallel {
                mixParallel(l, r, n)
            } else {
                s0.process(l, r, n); s1.process(l, r, n); s2.process(l, r, n)
            }
            if enableGain != target {
                let step = on ? enableStep : -enableStep
                var g = enableGain
                for i in 0..<n {
                    g = min(max(g + step, 0), 1)
                    l[i] = dryL[i] + g * (l[i] - dryL[i])
                    r[i] = dryR[i] + g * (r[i] - dryR[i])
                }
                enableGain = g
            }
            pos += n
        }
    }

    /// Parallel routing: dry + sum of the slots' (output - dry), divided by the number of active
    /// slots (counted with their crossfade gains, so fades stay smooth).
    private func mixParallel(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        accL.update(repeating: 0, count: n)
        accR.update(repeating: 0, count: n)
        var start: Float = 0, end: Float = 0
        accumulate(s0, n, &start, &end)
        accumulate(s1, n, &start, &end)
        accumulate(s2, n, &start, &end)
        var k = 1 / max(1, start)
        let dk = (1 / max(1, end) - k) / Float(n)
        for i in 0..<n {
            k += dk
            l[i] = dryL[i] + accL[i] * k
            r[i] = dryR[i] + accR[i] * k
        }
    }

    @inline(__always) private func accumulate(_ s: FXSlot, _ n: Int, _ start: inout Float, _ end: inout Float) {
        let f0 = s.fade
        tmpL.update(from: dryL, count: n)
        tmpR.update(from: dryR, count: n)
        guard s.process(tmpL, tmpR, n) else { return }
        start += f0
        end += s.fade
        for i in 0..<n {
            accL[i] += tmpL[i] - dryL[i]
            accR[i] += tmpR[i] - dryR[i]
        }
    }
}

/// One effect slot. Owns an instance of every effect type (all allocated in `prepare`), so a type
/// change only resets state.
final class FXSlot {
    private let pType: FXParam, pBypass: FXParam, pDryWet: FXParam
    private let reverb: FXReverb, delay: FXDelay, chorus: FXChorus, flanger: FXFlanger
    private let phaser: FXPhaser, overdrive: FXOverdrive, compressor: FXCompressor
    private let crusher: FXBitCrusher, filter: FXMultiFilter, eq: FXParamEQ, pan: FXStereoPan

    private let dryL: UnsafeMutablePointer<Float>, dryR: UnsafeMutablePointer<Float>
    /// Effect type currently running (0 = none) and its crossfade gain.
    private(set) var current = 0
    private(set) var fade: Float = 0
    private var want = 0
    /// Bit per effect type touched since its last reset; only those need clearing.
    private var dirty = 0
    private var wetT: Float = 0, wet: Float = 0
    private var fadeStep: Float = 0.002, cWet: Float = 0.001
    private var fresh = true
    private var params: UnsafePointer<Float>?
    private var tempo = 120.0, mainMod: Float = 0

    init(slot: Int) {
        pType = FXParam(slot: slot, "ModuleType")
        pBypass = FXParam(slot: slot, "Bypass")
        pDryWet = FXParam(slot: slot, "Dry_Wet")
        reverb = FXReverb(slot: slot); delay = FXDelay(slot: slot); chorus = FXChorus(slot: slot)
        flanger = FXFlanger(slot: slot); phaser = FXPhaser(slot: slot); overdrive = FXOverdrive(slot: slot)
        compressor = FXCompressor(slot: slot); crusher = FXBitCrusher(slot: slot)
        filter = FXMultiFilter(slot: slot); eq = FXParamEQ(slot: slot); pan = FXStereoPan(slot: slot)
        dryL = .allocate(capacity: 2 * fxChunk)
        dryL.initialize(repeating: 0, count: 2 * fxChunk)
        dryR = dryL + fxChunk
    }

    deinit { dryL.deallocate() }

    var compressorReductionDB: Float { current == 7 ? compressor.reductionDB : 0 }

    func prepare(sampleRate: Double) {
        reverb.prepare(sampleRate: sampleRate); delay.prepare(sampleRate: sampleRate)
        chorus.prepare(sampleRate: sampleRate); flanger.prepare(sampleRate: sampleRate)
        phaser.prepare(sampleRate: sampleRate); overdrive.prepare(sampleRate: sampleRate)
        compressor.prepare(sampleRate: sampleRate); crusher.prepare(sampleRate: sampleRate)
        filter.prepare(sampleRate: sampleRate); eq.prepare(sampleRate: sampleRate)
        pan.prepare(sampleRate: sampleRate)
        fadeStep = 1 / Float(0.01 * sampleRate)
        cWet = fxSmoothCoef(0.02, sampleRate)
        reset()
    }

    func reset() {
        for t in 1...11 where dirty & (1 << t) != 0 { resetEffect(t) }
        current = 0; fade = 0
        fresh = true
    }

    /// Reads the slot parameters once per host block.
    func begin(_ p: UnsafePointer<Float>, tempo: Double, mixMod: Float, mainMod: Float) {
        params = p
        self.tempo = tempo
        self.mainMod = mainMod
        want = pBypass.index(p) == 1 ? pType.index(p) : 0
        wetT = pDryWet.float(p, mixMod) / 100
        if want == 10 {
            // Param EQ: Dry/Wet scales the band gains instead of crossfading
            eq.dryWetScale = wetT
            wetT = 1
        }
        if fresh {
            current = want
            fade = want == 0 ? 0 : 1
            wet = wetT
            fresh = false
        }
        if current != 0 { update(current, p) }
    }

    /// Processes one chunk in place. Returns false if the slot is empty (signal untouched).
    @discardableResult
    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) -> Bool {
        if current != want && fade <= 0 {
            current = want
            if current != 0, let p = params {
                resetEffect(current)
                update(current, p)
            }
        }
        if current == 0 { fade = 0; return false }
        dryL.update(from: l, count: n)
        dryR.update(from: r, count: n)
        run(current, l, r, n)
        let step = current == want ? fadeStep : -fadeStep
        let wt = wetT, cw = cWet
        var f = fade, w = wet
        for i in 0..<n {
            f = min(max(f + step, 0), 1)
            w += (wt - w) * cw
            let m = f * w
            l[i] = dryL[i] + m * (l[i] - dryL[i])
            r[i] = dryR[i] + m * (r[i] - dryR[i])
        }
        fade = f
        wet = w
        return true
    }

    private func update(_ type: Int, _ p: UnsafePointer<Float>) {
        let m = mainMod, t = tempo
        dirty |= 1 << type
        switch type {
        case 1: reverb.update(p, main: m, tempo: t)
        case 2: delay.update(p, main: m, tempo: t)
        case 3: chorus.update(p, main: m, tempo: t)
        case 4: flanger.update(p, main: m, tempo: t)
        case 5: phaser.update(p, main: m, tempo: t)
        case 6: overdrive.update(p, main: m, tempo: t)
        case 7: compressor.update(p, main: m, tempo: t)
        case 8: crusher.update(p, main: m, tempo: t)
        case 9: filter.update(p, main: m, tempo: t)
        case 10: eq.update(p, main: m, tempo: t)
        case 11: pan.update(p, main: m, tempo: t)
        default: break
        }
    }

    private func resetEffect(_ type: Int) {
        dirty &= ~(1 << type)
        switch type {
        case 1: reverb.reset()
        case 2: delay.reset()
        case 3: chorus.reset()
        case 4: flanger.reset()
        case 5: phaser.reset()
        case 6: overdrive.reset()
        case 7: compressor.reset()
        case 8: crusher.reset()
        case 9: filter.reset()
        case 10: eq.reset()
        case 11: pan.reset()
        default: break
        }
    }

    private func run(_ type: Int, _ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        switch type {
        case 1: reverb.process(l, r, n)
        case 2: delay.process(l, r, n)
        case 3: chorus.process(l, r, n)
        case 4: flanger.process(l, r, n)
        case 5: phaser.process(l, r, n)
        case 6: overdrive.process(l, r, n)
        case 7: compressor.process(l, r, n)
        case 8: crusher.process(l, r, n)
        case 9: filter.process(l, r, n)
        case 10: eq.process(l, r, n)
        case 11: pan.process(l, r, n)
        default: break
        }
    }
}
