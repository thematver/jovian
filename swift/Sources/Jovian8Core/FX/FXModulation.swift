import Foundation

/// 1-3 voice chorus: each voice reads the delay line at Delay + Depth·(1 + lfo)/2 with the LFO
/// phases spread evenly; in stereo mode the right channel's LFOs run a quarter cycle later.
/// Feedback returns the voice average into the line.
/// Main (modulated) parameter: Feedback (the knob Jup-8 V highlights as modulable).
final class FXChorus {
    private let pDelay: FXParam, pDepth: FXParam, pFeedback: FXParam, pFreq: FXParam
    private let pShape: FXParam, pStereo: FXParam, pVoices: FXParam
    private var lineL = FXDelayLine(), lineR = FXDelayLine()
    private var sr: Float = 48000, cSample: Float = 0.001
    private var baseT: Float = 100, depthT: Float = 0, fbT: Float = 0
    private var base: Float = 100, depth: Float = 0, fb: Float = 0
    private var inc: Double = 0, phase: Double = 0
    private var voices = 2, triangle = false, stereo = true
    private var fresh = true

    init(slot: Int) {
        pDelay = FXParam(slot: slot, "Chorus_Delay")
        pDepth = FXParam(slot: slot, "Chorus_Depth")
        pFeedback = FXParam(slot: slot, "Chorus_Feedback")
        pFreq = FXParam(slot: slot, "Chorus_Freq")
        pShape = FXParam(slot: slot, "Chorus_LFOShape")
        pStereo = FXParam(slot: slot, "Chorus_StereoMode")
        pVoices = FXParam(slot: slot, "Chorus_Voices")
    }

    deinit { lineL.deallocate(); lineR.deallocate() }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        cSample = fxSmoothCoef(0.03, sampleRate)
        let n = Int(0.032 * sampleRate)
        lineL.allocate(n); lineR.allocate(n)
        reset()
    }

    func reset() {
        lineL.clear(); lineR.clear()
        phase = 0
        fresh = true
    }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        let ms = sr / 1000
        baseT = pDelay.float(p) * ms
        depthT = pDepth.float(p) * ms
        fbT = pFeedback.float(p, main)
        inc = pFreq.value(p) / Double(sr)
        triangle = pShape.index(p) == 1
        stereo = pStereo.index(p) == 1
        voices = pVoices.index(p) + 1
        if fresh { base = baseT; depth = depthT; fb = fbT; fresh = false }
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        let c = cSample, bT = baseT, dT = depthT, fT = fbT, tri = triangle
        let nv = voices, inv = 1 / Float(nv), off: Float = stereo ? 0.25 : 0
        var ph = phase
        for i in 0..<n {
            base += (bT - base) * c
            depth += (dT - depth) * c
            fb += (fT - fb) * c
            ph += inc
            if ph >= 1 { ph -= 1 }
            let half = 0.5 * depth, b1 = base - 1   // read-before-write: delay D is linear(D - 1)
            var sumL: Float = 0, sumR: Float = 0
            var pv = Float(ph)
            for _ in 0..<nv {
                let pR = fxWrap(pv + off)
                let ml = tri ? fxTriangle(pv) : fxSine(pv)
                let mr = tri ? fxTriangle(pR) : fxSine(pR)
                sumL += lineL.linear(b1 + half * (1 + ml))
                sumR += lineR.linear(b1 + half * (1 + mr))
                pv = fxWrap(pv + inv)
            }
            sumL *= inv; sumR *= inv
            lineL.write(fxFlush(l[i] + fb * sumL))
            lineR.write(fxFlush(r[i] + fb * sumR))
            l[i] = sumL
            r[i] = sumR
        }
        phase = ph
    }
}

/// Flanger: short modulated delay (MinDelay + Depth·(1 + lfo)/2) with signed feedback through
/// HP and LP filters; in stereo mode the right LFO runs a quarter cycle later.
/// Main (modulated) parameter: LFO rate (Flanger_Chorus_Freq, highlighted in Jup-8 V).
final class FXFlanger {
    private let pMin: FXParam, pDepth: FXParam, pFeedback: FXParam, pPolarity: FXParam, pStereo: FXParam
    private let pLP: FXParam, pHP: FXParam, pShape: FXParam, pFreq: FXParam
    private var lineL = FXDelayLine(), lineR = FXDelayLine()
    private var sr: Float = 48000, cSample: Float = 0.001, cChunk: Float = 0.03
    private var minT: Float = 0, depthT: Float = 0, fbT: Float = 0, lpT: Float = 20000, hpT: Float = 30
    private var minD: Float = 0, depth: Float = 0, fb: Float = 0, lpHz: Float = 20000, hpHz: Float = 30
    private var inc: Double = 0, phase: Double = 0
    private var triangle = true, stereo = true, fresh = true
    private var yl: Float = 0, yr: Float = 0
    private var hpL = FXOnePole(), hpR = FXOnePole(), lpL = FXOnePole(), lpR = FXOnePole()

    init(slot: Int) {
        pMin = FXParam(slot: slot, "Flanger_MinDelay")
        pDepth = FXParam(slot: slot, "Flanger_DepthFlanger")
        pFeedback = FXParam(slot: slot, "Flanger_Chorus_Feedback")
        pPolarity = FXParam(slot: slot, "Flanger_Chorus_FBPolarity")
        pStereo = FXParam(slot: slot, "Flanger_Chorus_StereoMode")
        pLP = FXParam(slot: slot, "Flanger_Chorus_LPFfc")
        pHP = FXParam(slot: slot, "Flanger_Chorus_HPFfc")
        pShape = FXParam(slot: slot, "Flanger_Chorus_LFOShape")
        pFreq = FXParam(slot: slot, "Flanger_Chorus_Freq")
    }

    deinit { lineL.deallocate(); lineR.deallocate() }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        cSample = fxSmoothCoef(0.03, sampleRate)
        cChunk = fxSmoothCoef(0.03, sampleRate, step: fxChunk)
        let n = Int(0.022 * sampleRate)
        lineL.allocate(n); lineR.allocate(n)
        reset()
    }

    func reset() {
        lineL.clear(); lineR.clear()
        hpL = FXOnePole(); hpR = FXOnePole(); lpL = FXOnePole(); lpR = FXOnePole()
        yl = 0; yr = 0; phase = 0
        fresh = true
    }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        let ms = sr / 1000
        minT = pMin.float(p) * ms
        depthT = pDepth.float(p) * ms
        fbT = pFeedback.float(p) * (pPolarity.index(p) == 1 ? -1 : 1)
        lpT = pLP.float(p)
        hpT = pHP.float(p)
        inc = pFreq.value(p, main) / Double(sr)
        triangle = pShape.index(p) == 1
        stereo = pStereo.index(p) == 1
        if fresh { minD = minT; depth = depthT; fb = fbT; lpHz = lpT; hpHz = hpT; fresh = false }
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        lpHz += (lpT - lpHz) * cChunk
        hpHz += (hpT - hpHz) * cChunk
        let gLP = fxOnePoleG(lpHz, sr), gHP = fxOnePoleG(hpHz, sr)
        let c = cSample, mT = minT, dT = depthT, fT = fbT, tri = triangle
        let off: Float = stereo ? 0.25 : 0
        var ph = phase, a = yl, b = yr
        for i in 0..<n {
            minD += (mT - minD) * c
            depth += (dT - depth) * c
            fb += (fT - fb) * c
            ph += inc
            if ph >= 1 { ph -= 1 }
            let pl = Float(ph), pr = fxWrap(pl + off)
            let ml = tri ? fxTriangle(pl) : fxSine(pl)
            let mr = tri ? fxTriangle(pr) : fxSine(pr)
            let half = 0.5 * depth
            let fl = lpL.lp(hpL.hp(a, gHP), gLP)
            let fr = lpR.lp(hpR.hp(b, gHP), gLP)
            lineL.write(fxFlush(fxSoftClip(l[i] + fb * fl, 2)))
            lineR.write(fxFlush(fxSoftClip(r[i] + fb * fr, 2)))
            a = lineL.linear(minD + half * (1 + ml))
            b = lineR.linear(minD + half * (1 + mr))
            l[i] = a
            r[i] = b
        }
        phase = ph; yl = a; yr = b
        hpL.flush(); hpR.flush(); lpL.flush(); lpR.flush()
    }
}

/// Phaser: cascade of 2...12 first-order (TPT) allpasses whose break frequency sweeps
/// ±3 octaves × LFO Amount around Frequency, with feedback from the last stage. The right
/// channel's LFO is offset by Stereo degrees. Coefficients are interpolated across each chunk.
/// Main (modulated) parameter: LFO rate (RateSynced when synced, RateUnSynced otherwise).
final class FXPhaser {
    private let pFeedback: FXParam, pFreq: FXParam, pAmount: FXParam, pWave: FXParam, pSync: FXParam
    private let pSynced: FXParam, pSyncType: FXParam, pRate: FXParam, pPoles: FXParam, pStereo: FXParam
    private let bars: [Double]
    private static let maxPoles = 12
    private static let sweepOctaves: Float = 3
    private let st: UnsafeMutablePointer<Float>   // allpass states, L then R
    private var sr: Float = 48000, cChunk: Float = 0.03
    private var fbT: Float = 0, freqT: Float = 740, amountT: Float = 1
    private var fb: Float = 0, freq: Float = 740, amount: Float = 1
    private var inc: Double = 0, phase: Double = 0, offset: Double = 0, lastR: Double = 0
    private var wave = 0, poles = 6
    private var gL: Float = 0, gR: Float = 0, yl: Float = 0, yr: Float = 0
    private var shL: Float = 0, shR: Float = 0, rng = FXRandom()
    private var fresh = true

    init(slot: Int) {
        pFeedback = FXParam(slot: slot, "Phaser_Feedback")
        pFreq = FXParam(slot: slot, "Phaser_Freq")
        pAmount = FXParam(slot: slot, "Phaser_LFOAmount")
        pWave = FXParam(slot: slot, "Phaser_LFOWaveform")
        pSync = FXParam(slot: slot, "Phaser_LFO_MidiSync")
        pSynced = FXParam(slot: slot, "Phaser_LFO_RateSynced")
        pSyncType = FXParam(slot: slot, "Phaser_LFO_RateSyncedType")
        pRate = FXParam(slot: slot, "Phaser_LFO_RateUnSynced")
        pPoles = FXParam(slot: slot, "Phaser_NPoles")
        pStereo = FXParam(slot: slot, "Phaser_Stereo")
        bars = fxBars("FX\(slot + 1)_Phaser_LFO_RateSynced")
        st = .allocate(capacity: 2 * FXPhaser.maxPoles)
        st.initialize(repeating: 0, count: 2 * FXPhaser.maxPoles)
    }

    deinit { st.deallocate() }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        cChunk = fxSmoothCoef(0.03, sampleRate, step: fxChunk)
        reset()
    }

    func reset() {
        st.update(repeating: 0, count: 2 * FXPhaser.maxPoles)
        yl = 0; yr = 0; phase = 0; lastR = 0
        fresh = true
    }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        let hz: Double
        if pSync.index(p) == 1 {
            let i = pSynced.index(p, main)
            hz = 1 / fxSyncSeconds(bars[min(i, bars.count - 1)], pSyncType.index(p), tempo)
        } else {
            hz = pRate.value(p, main)
        }
        inc = hz / Double(sr)
        fbT = pFeedback.float(p)
        freqT = pFreq.float(p)
        amountT = pAmount.float(p)
        wave = pWave.index(p)
        poles = 2 * (pPoles.index(p) + 1)
        offset = pStereo.value(p) / 360
        if fresh {
            fb = fbT; freq = freqT; amount = amountT
            gL = coef(lfo(phase, shL)); gR = coef(lfo(fxWrapD(phase + offset), shR))
            fresh = false
        }
    }

    @inline(__always) private func fxWrapD(_ p: Double) -> Double { p >= 1 ? p - 1 : p }

    @inline(__always) private func lfo(_ p: Double, _ sh: Float) -> Float {
        let x = Float(p)
        switch wave {
        case 0: return fxSine(x)
        case 1: return fxTriangle(x)
        case 2: return 1 - 2 * x
        case 3: return 2 * x - 1
        case 4: return x < 0.5 ? 1 : -1
        default: return sh
        }
    }

    @inline(__always) private func coef(_ m: Float) -> Float {
        let hz = freq * exp2f(m * amount * FXPhaser.sweepOctaves)
        return fxOnePoleG(hz, sr)
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        fb += (fbT - fb) * cChunk
        freq += (freqT - freq) * cChunk
        amount += (amountT - amount) * cChunk
        // Advance the LFO to the end of the chunk; sample & hold draws on each cycle start.
        phase += inc * Double(n)
        if phase >= 1 { phase -= floor(phase); shL = rng.bipolar() }
        let pR = fxWrapD(phase + offset)
        if pR < lastR { shR = rng.bipolar() }
        lastR = pR
        let gL1 = coef(lfo(phase, shL)), gR1 = coef(lfo(pR, shR))
        let dl = (gL1 - gL) / Float(n), dr = (gR1 - gR) / Float(n)
        var a = gL, b = gR, ya = yl, yb = yr
        let np = poles, f = fb, s = st, sR = st + FXPhaser.maxPoles
        for i in 0..<n {
            a += dl; b += dr
            var u = fxSoftClip(l[i] + f * ya, 4)
            var v = fxSoftClip(r[i] + f * yb, 4)
            for k in 0..<np {
                let x1 = (u - s[k]) * a
                let lp1 = x1 + s[k]
                s[k] = lp1 + x1
                u = 2 * lp1 - u
                let x2 = (v - sR[k]) * b
                let lp2 = x2 + sR[k]
                sR[k] = lp2 + x2
                v = 2 * lp2 - v
            }
            ya = u; yb = v
            l[i] = u
            r[i] = v
        }
        gL = gL1; gR = gR1; yl = fxFlush(ya); yr = fxFlush(yb)
        for k in 0..<(2 * FXPhaser.maxPoles) { s[k] = fxFlush(s[k]) }
    }
}

/// Auto-panner: equal-power balance driven by a sine LFO, Amount = depth (0 centre ... 1 hard).
/// Rate in Hz or synced (binary, triplet, dotted).
/// Main (modulated) parameter: Amount.
final class FXStereoPan {
    private let pAmount: FXParam, pSynced: FXParam, pType: FXParam, pRate: FXParam
    private let bars: [Double]
    private var sr: Float = 48000, cSample: Float = 0.001
    private var amountT: Float = 0, amount: Float = 0
    private var inc: Double = 0, phase: Double = 0
    private var fresh = true

    init(slot: Int) {
        pAmount = FXParam(slot: slot, "StereoPan_Amount")
        pSynced = FXParam(slot: slot, "StereoPan_LFOSimple_RateSynced")
        pType = FXParam(slot: slot, "StereoPan_LFOSimple_RateType")
        pRate = FXParam(slot: slot, "StereoPan_LFOSimple_RateUnSynced")
        bars = fxBars("FX\(slot + 1)_StereoPan_LFOSimple_RateSynced")
    }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        cSample = fxSmoothCoef(0.02, sampleRate)
        reset()
    }

    func reset() { phase = 0; fresh = true }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        let type = pType.index(p)
        let hz: Double
        if type == 0 {
            hz = pRate.value(p)
        } else {
            let i = pSynced.index(p)
            hz = 1 / fxSyncSeconds(bars[min(i, bars.count - 1)], type - 1, tempo)
        }
        inc = hz / Double(sr)
        amountT = pAmount.float(p, main)
        if fresh { amount = amountT; fresh = false }
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        let c = cSample, aT = amountT
        var ph = phase
        for i in 0..<n {
            amount += (aT - amount) * c
            ph += inc
            if ph >= 1 { ph -= 1 }
            let q = (amount * fxSine(Float(ph)) + 1) * 0.125   // pan angle in cycles, 0...0.25
            l[i] *= 1.414_213_6 * fxSine(q + 0.25)
            r[i] *= 1.414_213_6 * fxSine(q)
        }
        phase = ph
    }
}
