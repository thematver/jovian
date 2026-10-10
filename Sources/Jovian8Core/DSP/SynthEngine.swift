import Foundation

/// Host transport information for one render call.
public struct HostTime: Sendable {
    public var tempo: Double = 120
    public var beatPosition: Double = 0
    public var isPlaying: Bool = false
    public init(tempo: Double = 120, beatPosition: Double = 0, isPlaying: Bool = false) {
        self.tempo = tempo; self.beatPosition = beatPosition; self.isPlaying = isPlaying
    }
}

/// The complete instrument: 16 voices, global LFO, hybrid LFOs, modulation matrix, voice assign
/// modes, arpeggiator and the effects rack. Everything on the render path is allocation free.
public final class SynthEngine: @unchecked Sendable {
    public static let maxVoices = 16
    static let controlBlock = 16          // host samples per control-rate update
    static let destStride = Dest.count

    public let params: ParamStore
    let fx = FXRack()

    public private(set) var sampleRate = 48_000.0
    private var osRate = 96_000.0
    private var maxFrames = 4096
    private var patch = Patch()
    private var lfo2Set = HybridLFOSettings()
    private var lfo3Set = HybridLFOSettings()

    private let voices: UnsafeMutablePointer<Voice>
    private let controls: UnsafeMutablePointer<VoiceControl>
    private let dest: UnsafeMutablePointer<Float>          // per-voice matrix offsets
    private let globalDest: UnsafeMutablePointer<Float>
    private let osL: UnsafeMutablePointer<Float>
    private let osR: UnsafeMutablePointer<Float>
    private let fxMod: UnsafeMutablePointer<Float>
    private let decL = HalfBandDecimator()
    private let decR = HalfBandDecimator()

    // active matrix slots for this block
    private var slotCount = 0
    private let slotSource: UnsafeMutablePointer<Int>
    private let slotDest: UnsafeMutablePointer<Int>
    private let slotAmount: UnsafeMutablePointer<Float>
    private var curves = [(Float, Float, Float, Float)](repeating: (0, 0.5, 0.5, 1), count: 4)

    // performance state
    private var modWheel: Float = 0
    private var bend: Float = 0
    private var aftertouch: Float = 0
    private var slide: Float = 0
    private var sustainPedal = false
    private let keyDown: UnsafeMutablePointer<Bool>        // 128 physical keys
    private var held = NoteList()                          // notes currently held (press order)
    private var lastVelocity: Float = 0.8
    private var lastNote = 60
    private var ageCounter: UInt64 = 0
    private var roundRobin = 0

    // LFO 1 (global)
    private var lfoPhase = 0.0
    private var lfoValue: Float = 0
    private var lfoSH: Float = 0
    private var lfoNoise = Noise(state: 0x1234_5678)
    private var lfo2Global = HybridLFOState()
    private var lfo3Global = HybridLFOState()

    // arpeggiator
    private var arp = Arpeggiator()
    private var arpWasOn = false

    public init(params: ParamStore = ParamStore()) {
        self.params = params
        _ = ParamTable.fast; _ = Matrix.slots; _ = Matrix.curveAddresses; _ = A.fxMain
        _ = HybridLFOAddresses.lfo2; _ = HybridLFOAddresses.lfo3; _ = RateLists.arpBars; _ = HalfBandDecimator.tapValues
        voices = .allocate(capacity: Self.maxVoices)
        voices.initialize(repeating: Voice(), count: Self.maxVoices)
        for i in 0..<Self.maxVoices {
            voices[i].randomize(seed: UInt32(i * 2_654_435 + 17))
            voices[i].panPos = (i % 2 == 0 ? -1 : 1) * Float(i / 2 + 1) / Float(Self.maxVoices / 2)
        }
        controls = .allocate(capacity: Self.maxVoices)
        controls.initialize(repeating: VoiceControl(), count: Self.maxVoices)
        dest = .allocate(capacity: Self.maxVoices * Self.destStride)
        dest.initialize(repeating: 0, count: Self.maxVoices * Self.destStride)
        globalDest = .allocate(capacity: Self.destStride)
        globalDest.initialize(repeating: 0, count: Self.destStride)
        osL = .allocate(capacity: 2 * Self.controlBlock)
        osR = .allocate(capacity: 2 * Self.controlBlock)
        fxMod = .allocate(capacity: 6)
        fxMod.initialize(repeating: 0, count: 6)
        slotSource = .allocate(capacity: 24)
        slotDest = .allocate(capacity: 24)
        slotAmount = .allocate(capacity: 24)
        keyDown = .allocate(capacity: 128)
        keyDown.initialize(repeating: false, count: 128)
        prepare(sampleRate: 48_000, maxFrames: 4096)
    }

    deinit {
        voices.deallocate(); controls.deallocate(); dest.deallocate(); globalDest.deallocate()
        osL.deallocate(); osR.deallocate(); fxMod.deallocate()
        slotSource.deallocate(); slotDest.deallocate(); slotAmount.deallocate(); keyDown.deallocate()
    }

    /// Call off the render thread before rendering (and whenever the format changes).
    public func prepare(sampleRate: Double, maxFrames: Int) {
        self.sampleRate = sampleRate
        self.osRate = sampleRate * 2
        self.maxFrames = maxFrames
        fx.prepare(sampleRate: sampleRate, maxFrames: maxFrames)
        reset()
    }

    public func reset() {
        for i in 0..<Self.maxVoices {
            voices[i].active = false; voices[i].gate = false
            voices[i].env1 = Envelope(); voices[i].env2 = Envelope()
            voices[i].ladder.reset(); voices[i].hpf = OnePoleHP()
            voices[i].amp = 0
        }
        held.removeAll()
        keyDown.update(repeating: false, count: 128)
        arp.reset()
        decL.reset(); decR.reset()
        fx.reset()
    }

    // MARK: - MIDI

    /// Handles one MIDI 1.0 channel message (any channel).
    public func handleMIDI(_ status: UInt8, _ d1: UInt8, _ d2: UInt8) {
        switch status & 0xF0 {
        case 0x90 where d2 > 0: noteOn(Int(d1), velocity: Float(d2) / 127)
        case 0x80, 0x90: noteOff(Int(d1))
        case 0xB0:
            switch d1 {
            case 1: modWheel = Float(d2) / 127
            case 64: setSustain(d2 >= 64)
            case 74: slide = Float(d2) / 127
            case 120, 123: allNotesOff(hard: d1 == 120)
            default: break
            }
        case 0xD0: aftertouch = Float(d1) / 127
        case 0xA0: aftertouch = max(aftertouch, Float(d2) / 127)
        case 0xE0: bend = Float((Int(d2) << 7 | Int(d1)) - 8192) / 8192
        default: break
        }
    }

    public func noteOn(_ note: Int, velocity: Float) {
        guard (0..<128).contains(note) else { return }
        keyDown[note] = true
        if patch.arpOn { arp.add(note: note, velocity: velocity); return }
        startNote(note, velocity: velocity)
    }

    public func noteOff(_ note: Int) {
        guard (0..<128).contains(note) else { return }
        keyDown[note] = false
        if patch.arpOn {
            if !sustainPedal { arp.remove(note: note) }
            return
        }
        stopNote(note, pedal: sustainPedal)
    }

    public func allNotesOff(hard: Bool) {
        held.removeAll()
        arp.reset()
        keyDown.update(repeating: false, count: 128)
        for i in 0..<Self.maxVoices {
            voices[i].gate = false; voices[i].sustained = false
            voices[i].env1.gateOff(); voices[i].env2.gateOff()
            if hard { voices[i].active = false; voices[i].env2.level = 0; voices[i].env2.stage = .idle }
        }
    }

    private func setSustain(_ on: Bool) {
        sustainPedal = on
        guard !on else { return }
        if patch.arpOn {
            arp.keepOnly { keyDown[$0] }
            return
        }
        for i in 0..<Self.maxVoices where voices[i].sustained {
            voices[i].sustained = false
            if !keyDown[voices[i].note] { releaseVoice(i) }
        }
        var i = 0
        while i < held.count {
            if !keyDown[held.notes[i]] { held.remove(at: i) } else { i += 1 }
        }
        if isMonoMode && held.count == 0 { for v in 0..<Self.maxVoices where voices[v].gate { releaseVoice(v) } }
    }

    // MARK: - Voice assignment

    private var isMonoMode: Bool { patch.assign == 0 || (patch.assign == 1 && patch.uniVoices == 0) }

    private var voiceLimit: Int { min(max(patch.polyphony, 1), Self.maxVoices) }

    private func startNote(_ note: Int, velocity: Float) {
        lastNote = note
        lastVelocity = velocity
        held.remove(note: note)
        held.append(note: note, velocity: velocity)
        if patch.lfoRetrig { lfoPhase = 0 }
        if !lfo2Set.poly { lfo2Global.trigger(lfo2Set) }
        if !lfo3Set.poly { lfo3Global.trigger(lfo3Set) }

        if isMonoMode {
            let stack = patch.assign == 0 ? 1 : voiceLimit
            let target = held.pick(priority: patch.prio)
            for j in 0..<stack {
                assign(voice: j, note: target.note, velocity: target.velocity, stackIndex: j, stackSize: stack, retrigger: true)
            }
            for j in stack..<Self.maxVoices where voices[j].gate { releaseVoice(j) }
            return
        }
        let stack = patch.assign == 1 ? max(1, min(patch.uniVoices, voiceLimit)) : 1
        let slots = max(1, voiceLimit / stack) * stack
        // re-striking a held/sounding note reuses its voices
        var count = 0
        var picks = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        withUnsafeMutableBytes(of: &picks) { raw in
            let p = raw.bindMemory(to: Int.self)
            for i in 0..<slots where voices[i].active && voices[i].note == note && count < stack {
                p[count] = i; count += 1
            }
            while count < stack {
                let v = freeVoice(limit: slots, exclude: p, excludeCount: count)
                p[count] = v; count += 1
            }
            for j in 0..<stack {
                assign(voice: p[j], note: note, velocity: velocity, stackIndex: j, stackSize: stack, retrigger: true)
            }
        }
    }

    /// Picks a voice: idle first, then the quietest released one, then the oldest held one.
    private func freeVoice(limit: Int, exclude: UnsafeMutableBufferPointer<Int>, excludeCount: Int) -> Int {
        func excluded(_ i: Int) -> Bool {
            for j in 0..<excludeCount where exclude[j] == i { return true }
            return false
        }
        let rotate = patch.assign == 2      // Poly 1 rotates, Poly 2 / unison fill from the bottom
        for n in 0..<limit {
            let i = rotate ? (roundRobin + n) % limit : n
            if !voices[i].active && !excluded(i) {
                if rotate { roundRobin = (i + 1) % limit }
                return i
            }
        }
        var best = -1
        var bestLevel: Float = .greatestFiniteMagnitude
        for i in 0..<limit where !voices[i].gate && !excluded(i) {
            if voices[i].env2.level < bestLevel { bestLevel = voices[i].env2.level; best = i }
        }
        if best >= 0 { return best }
        var oldest: UInt64 = .max
        for i in 0..<limit where !excluded(i) {
            if voices[i].age < oldest { oldest = voices[i].age; best = i }
        }
        return max(best, 0)
    }

    private func assign(voice i: Int, note: Int, velocity: Float, stackIndex: Int, stackSize: Int, retrigger: Bool) {
        ageCounter &+= 1
        let v = voices + i
        let wasActive = v.pointee.active
        v.pointee.note = note
        v.pointee.velocity = velocity
        v.pointee.age = ageCounter
        v.pointee.stackIndex = stackIndex
        v.pointee.stackSize = stackSize
        v.pointee.gate = true
        v.pointee.sustained = false
        v.pointee.active = true
        v.pointee.targetPitch = Double(note)
        if !(patch.portaOn && patch.portaMs > 0 && v.pointee.hasPlayed) { v.pointee.pitch = Double(note) }
        v.pointee.hasPlayed = true
        if retrigger || !wasActive {
            v.pointee.env1.gateOn()
            v.pointee.env2.gateOn()
            v.pointee.lfoFade = patch.lfoDelay > 0.001 ? 0 : 1
            if lfo2Set.poly { v.pointee.lfo2.trigger(lfo2Set) }
            if lfo3Set.poly { v.pointee.lfo3.trigger(lfo3Set) }
        }
    }

    private func releaseVoice(_ i: Int) {
        voices[i].gate = false
        voices[i].env1.gateOff()
        voices[i].env2.gateOff()
    }

    private func stopNote(_ note: Int, pedal: Bool) {
        if isMonoMode {
            if pedal { for i in 0..<Self.maxVoices where voices[i].gate { voices[i].sustained = true }; return }
            held.remove(note: note)
            if held.count > 0 {
                let target = held.pick(priority: patch.prio)
                let stack = patch.assign == 0 ? 1 : voiceLimit
                if voices[0].note != target.note {
                    for j in 0..<stack {
                        assign(voice: j, note: target.note, velocity: target.velocity, stackIndex: j, stackSize: stack, retrigger: false)
                    }
                }
            } else {
                for i in 0..<Self.maxVoices where voices[i].gate { releaseVoice(i) }
            }
            return
        }
        held.remove(note: note)
        for i in 0..<Self.maxVoices where voices[i].gate && voices[i].note == note {
            if pedal { voices[i].sustained = true } else { releaseVoice(i) }
        }
    }

    // MARK: - Arpeggiator clock

    private func runArp(dt: Double, time: HostTime) {
        if patch.arpOn != arpWasOn {
            // switching modes hands the held keys over
            if patch.arpOn {
                for i in 0..<held.count { arp.add(note: held.notes[i], velocity: held.velocities[i]) }
                held.removeAll()
                for i in 0..<Self.maxVoices where voices[i].gate { releaseVoice(i) }
            } else {
                if let n = arp.sounding { stopNote(n, pedal: false) }
                for i in 0..<arp.count where keyDown[arp.notes[i]] { startNote(arp.notes[i], velocity: arp.velocities[i]) }
                arp.reset()
            }
            arpWasOn = patch.arpOn
        }
        guard patch.arpOn else { return }
        let ev = arp.advance(dt: dt, hz: patch.arpHz, mode: patch.arpMode, octaves: patch.arpOctaves)
        if let off = ev.off { stopNote(off, pedal: false) }
        if let on = ev.on { startNote(on, velocity: ev.velocity) }
    }

    // MARK: - Rendering

    /// Renders `frames` samples (<= maxFrames) of stereo output, replacing the buffer contents.
    public func render(frames: Int, left: UnsafeMutablePointer<Float>, right: UnsafeMutablePointer<Float>, time: HostTime) {
        var done = 0
        let tempo = time.tempo > 0 ? time.tempo : 120
        while done < frames {
            let n = min(Self.controlBlock, frames - done)
            control(frames: n, time: time, tempo: tempo)
            let os = 2 * n
            osL.update(repeating: 0, count: os)
            osR.update(repeating: 0, count: os)
            let w1 = patch.osc1Wave, w2 = patch.osc2Wave, sync = patch.sync, two = patch.twoPole
            for i in 0..<Self.maxVoices where voices[i].active {
                let xmAmt = Float(min(max(patch.crossMod + Double(dest[i * Self.destStride + Dest.crossMod]), 0), 1))
                voices[i].render(count: os, outL: osL, outR: osR, c: controls[i], wave1: w1, wave2: w2,
                                 sync: sync, crossMod: xmAmt, twoPole: two)
                if voices[i].env2.stage == .idle && !voices[i].gate { voices[i].active = false }
            }
            let g = Float(patch.gain)
            for s in 0..<n {
                left[done + s] = decL.process(osL[2 * s], osL[2 * s + 1]) * g
                right[done + s] = decR.process(osR[2 * s], osR[2 * s + 1]) * g
            }
            done += n
        }
        for i in 0..<3 {
            fxMod[i] = globalDest[Dest.fx1Mix + i]
            fxMod[3 + i] = globalDest[Dest.fx1Param + i]
        }
        fx.process(left: left, right: right, frames: frames, params: params.values, tempo: tempo, mod: fxMod)
    }

    /// Control-rate update for the next `frames` host samples.
    private func control(frames: Int, time: HostTime, tempo: Double) {
        let p = UnsafePointer(params.values)
        patch.update(p, tempo: tempo)
        lfo2Set.update(p, .lfo2, tempo: tempo)
        lfo3Set.update(p, .lfo3, tempo: tempo)
        let dt = Double(frames) / sampleRate
        let osN = 2 * frames

        runArp(dt: dt, time: time)
        updateMatrixSlots(p)

        // global modulation sources (last played note) and destinations
        let gl2 = lfo2Global.advance(lfo2Set, dt: dt, rateMul: 1, warpOffset: 0)
        let gl3 = lfo3Global.advance(lfo3Set, dt: dt, rateMul: 1, warpOffset: 0)
        globalDest.update(repeating: 0, count: Self.destStride)
        accumulate(into: globalDest, lfo2: gl2, lfo3: gl3, velocity: lastVelocity, note: lastNote, scale: 1)

        // LFO 1
        let rateMul = exp2(Double(globalDest[Dest.lfo1Rate]) * 8)
        lfoPhase += patch.lfoHz * rateMul * dt
        if lfoPhase >= 1 { lfoPhase -= lfoPhase.rounded(.down); lfoSH = lfoNoise.next() }
        switch patch.lfoWave {
        case 0: lfoValue = Float(-cos(2 * Double.pi * lfoPhase))
        case 1: lfoValue = Float(1 - 2 * lfoPhase)
        case 2: lfoValue = lfoPhase < 0.5 ? 1 : -1
        default: lfoValue = lfoSH
        }
        let lfoDelay = max(0, patch.lfoDelay * (1 + Double(globalDest[Dest.lfo1Delay]) * 4))
        let fadeStep = Float(lfoDelay > 0.001 ? dt / lfoDelay : 1)

        let f = ParamTable.fast
        let bendSemis = Double(bend) * patch.bendSt
        let mwVib = patch.mwVCO ? Double(lfoValue * modWheel) * patch.mwVCOSt : 0
        let mwCut = patch.mwVCF ? Double(lfoValue * modWheel) * patch.mwVCFAmt * VoiceMath.modWheelToCutoff : 0
        let bendCut = patch.bendF ? Double(bend) * patch.bendFAmt * patch.bendFAmt * VoiceMath.bendToCutoff : 0
        let portaCoef = patch.portaOn && patch.portaMs > 0 ? 1 - exp(-dt * 4.6 / (patch.portaMs / 1000)) : 1
        let nyq = osRate * 0.45

        for i in 0..<Self.maxVoices where voices[i].active {
            let v = voices + i
            let d = dest + i * Self.destStride
            d.update(repeating: 0, count: Self.destStride)
            let modScale = 1 + Float(patch.dispMod) * v.pointee.rMod * 0.2
            let l2 = lfo2Set.poly ? v.pointee.lfo2.advance(lfo2Set, dt: dt, rateMul: 1, warpOffset: 0) : gl2
            let l3 = lfo3Set.poly ? v.pointee.lfo3.advance(lfo3Set, dt: dt, rateMul: 1, warpOffset: 0) : gl3
            if slotCount > 0 {
                accumulate(into: d, lfo2: l2, lfo3: l3, velocity: v.pointee.velocity, note: v.pointee.note, scale: modScale)
            }
            @inline(__always) func mod(_ base: Int, _ di: Int) -> Float { min(max(p[base] + d[di], 0), 1) }
            @inline(__always) func phys(_ base: Int, _ di: Int) -> Double { d[di] == 0 ? f[base].physical(p[base]) : f[base].physical(mod(base, di)) }

            // envelopes at control rate
            let note = v.pointee.pitch
            let envScale = (1 + patch.dispEnv * Double(v.pointee.rEnv) * 0.25)
            let kf1 = patch.e1KF ? VoiceMath.envKeyFollow(note) : 1
            let kf2 = patch.e2KF ? VoiceMath.envKeyFollow(note) : 1
            let e1 = stepEnvelope(&v.pointee.env1, a: phys(A.e1A, Dest.e1A) * kf1 * envScale, d: phys(A.e1D, Dest.e1D) * kf1 * envScale,
                                  s: phys(A.e1S, Dest.e1S) / 100, r: phys(A.e1R, Dest.e1R) * kf1 * envScale, samples: osN)
            let e2 = stepEnvelope(&v.pointee.env2, a: phys(A.e2A, Dest.e2A) * kf2 * envScale, d: phys(A.e2D, Dest.e2D) * kf2 * envScale,
                                  s: phys(A.e2S, Dest.e2S) / 100, r: phys(A.e2R, Dest.e2R) * kf2 * envScale, samples: osN)
            let env1 = patch.e1Inv ? -e1 : e1

            // LFO 1 with per-voice delay fade
            if v.pointee.lfoFade < 1 { v.pointee.lfoFade = min(1, v.pointee.lfoFade + fadeStep) }
            let lfo = Double(lfoValue * v.pointee.lfoFade)

            // portamento
            v.pointee.pitch += (v.pointee.targetPitch - v.pointee.pitch) * portaCoef

            // pitch
            let stack = v.pointee.stackSize
            let detune = stack > 1 ? patch.detuneSt * (1 + Double(d[Dest.detune]) * 4) * (2 * Double(v.pointee.stackIndex) / Double(stack - 1) - 1) : 0
            let vib = lfo * phys(A.vcoLfo, Dest.vcoLfo) * VoiceMath.lfoPitchScale
            let envP = env1 * phys(A.vcoEnv, Dest.vcoEnv)
            let modP = vib + envP
            let dsel = patch.vcoDest
            let disp = patch.dispPitch * 0.5
            var p1 = v.pointee.pitch + patch.osc1Semis + detune + mwVib + disp * Double(v.pointee.rPitch1)
            var p2 = v.pointee.pitch + patch.osc2Semis + patch.osc2Fine + detune + mwVib + disp * Double(v.pointee.rPitch2)
            if dsel >= 1 { p1 += modP }
            if dsel <= 1 { p2 += modP }
            if patch.bendV1 { p1 += bendSemis }
            if patch.bendV2 { p2 += bendSemis }
            p1 += Double(d[Dest.vco1Coarse] + d[Dest.vco12Coarse]) * 36
            p2 += Double(d[Dest.vco2Coarse] + d[Dest.vco12Coarse]) * 36 + Double(d[Dest.vco2Fine]) * 2
            let f1 = patch.a4 * exp2((p1 - 69) / 12)
            let f2: Double
            if patch.osc2Low {
                var lowSemis = patch.osc2Semis + patch.osc2Fine + Double(d[Dest.vco2Coarse] + d[Dest.vco12Coarse]) * 36
                if dsel <= 1 { lowSemis += modP }
                f2 = VoiceMath.vco2LowBaseHz * exp2(lowSemis / 12)
            } else {
                f2 = patch.a4 * exp2((p2 - 69) / 12)
            }

            // pulse width
            let pwAmt = phys(A.pwAmount, Dest.pwAmount) / 100
            let pwSrc: Double
            switch patch.pwMode {
            case 0: pwSrc = abs(env1)
            case 1: pwSrc = 1
            default: pwSrc = 0.5 + 0.5 * lfo
            }
            let pwDisp = patch.dispPW * Double(v.pointee.rPW) * 0.03
            let duty = VoiceMath.pulseDuty(pwAmt * pwSrc) + pwDisp
            let duty1 = min(max(duty + Double(d[Dest.vco1PW]) * 0.45, 0.05), 0.95)
            let duty2 = min(max(duty + Double(d[Dest.vco2PW]) * 0.45, 0.05), 0.95)

            // mixer
            let m = Double(mod(A.mix, Dest.mix)) * Double.pi / 2

            // HPF
            let hp = min(phys(A.hpf, Dest.hpf), nyq)
            let gh = tan(Double.pi * hp / osRate)

            // VCF
            let envSrc = patch.vcfEnvSel == 0 ? env1 : e2
            var x = Double(mod(A.cutoff, Dest.cutoff))
            x += Double(mod(A.vcfEnv, Dest.vcfEnv)) * VoiceMath.envToCutoff * envSrc
            x += Double(mod(A.vcfLfo, Dest.vcfLfo)) * VoiceMath.lfoToCutoff * lfo
            x += mwCut + bendCut + patch.dispCut * Double(v.pointee.rCut) * 0.04
            let key = Double(mod(A.vcfKey, Dest.vcfKey)) * VoiceMath.keyFollowOctPerOct * (note - 60) / 12
            let fc = min(max(VoiceMath.cutoffHz(x) * exp2(key), 8), nyq)
            let g = tan(Double.pi * fc / osRate)
            let reso = Double(mod(A.reso, Dest.reso)) + patch.dispRes * Double(v.pointee.rRes) * 0.05

            // VCA
            let level = pow(Double(mod(A.vcaLevel, Dest.vcaLevel)), VoiceMath.vcaLevelExponent)
            let dip = pow(Double(mod(A.vcaLfo, Dest.vcaLfo)), VoiceMath.vcaLfoExponent) * (1 - lfo) / 2
            let stackGain = 1.0   // the reference sums unison voices without normalization

            // pan
            let spread = Double(mod(A.pan, Dest.pan))
            let panPos = stack > 1 ? (2 * Double(v.pointee.stackIndex) / Double(stack - 1) - 1) : Double(v.pointee.panPos)
            let angle = (spread * panPos + 1) * Double.pi / 4

            let c = controls + i
            c.pointee.inc1 = Float(f1 / osRate)
            c.pointee.inc2 = Float(min(f2 / osRate, 0.45))
            c.pointee.G = Float(g / (1 + g))
            c.pointee.k = Float(VoiceMath.resonanceK(min(max(reso, 0), 1)))
            c.pointee.hpG = Float(gh / (1 + gh))
            c.pointee.duty1 = Float(duty1); c.pointee.duty2 = Float(duty2)
            c.pointee.mix1 = Float(cos(m)); c.pointee.mix2 = Float(sin(m))
            c.pointee.amp = Float(e2 * level * (1 - dip) * stackGain)
            c.pointee.panL = Float(cos(angle) * 1.414_213_56)
            c.pointee.panR = Float(sin(angle) * 1.414_213_56)
            if !v.pointee.hasPlayedControl { v.pointee.copyControl(c.pointee) }
        }
    }

    /// Advances an envelope by `samples` oversampled steps at once (exact for the RC segments).
    @inline(__always)
    private func stepEnvelope(_ e: inout Envelope, a: Double, d: Double, s: Double, r: Double, samples: Int) -> Double {
        let n = Double(samples)
        let sr = osRate
        switch e.stage {
        case .idle: return 0
        case .attack:
            let tau = max(a, 0.05) * VoiceMath.attackTauFactor * 0.001 * sr
            let target = VoiceMath.attackTarget
            let lv = target + (Double(e.level) - target) * exp(-n / tau)
            if lv >= 1 { e.level = 1; e.stage = .decay } else { e.level = Float(lv) }
        case .decay:
            let tau = max(d, 0.05) / VoiceMath.decayTauDivisor * 0.001 * sr
            e.level = Float(s + (Double(e.level) - s) * exp(-n / tau))
        case .release:
            let tau = max(r, 0.05) / VoiceMath.releaseTauDivisor * 0.001 * sr
            e.level = Float(Double(e.level) * exp(-n / tau))
            if e.level < 1e-5 { e.level = 0; e.stage = .idle }
        }
        return Double(e.level)
    }

    // MARK: - Matrix

    private func updateMatrixSlots(_ p: UnsafePointer<Float>) {
        slotCount = 0
        let f = ParamTable.fast
        for s in Matrix.slots {
            let di = f[s.destAddress].index(p[s.destAddress]) - 1
            guard di >= 0 else { continue }
            let amt = Float(f[s.amountAddress].physical(p[s.amountAddress]))
            guard abs(amt) > 1e-5 else { continue }
            slotSource[slotCount] = s.source
            slotDest[slotCount] = di
            slotAmount[slotCount] = amt
            slotCount += 1
        }
        for k in 0..<4 {
            let a = Matrix.curveAddresses[k]
            curves[k] = (p[a.0], p[a.1], p[a.2], p[a.3])
        }
    }

    @inline(__always)
    private func accumulate(into d: UnsafeMutablePointer<Float>, lfo2: Float, lfo3: Float, velocity: Float, note: Int, scale: Float) {
        for s in 0..<slotCount {
            let src: Float
            switch slotSource[s] {
            case 1: src = lfo2
            case 2: src = lfo3
            case 5: let c = curves[0]; src = Matrix.curve(velocity, c.0, c.1, c.2, c.3)
            case 6: let c = curves[1]; src = Matrix.curve(aftertouch, c.0, c.1, c.2, c.3)
            case 7: let c = curves[2]; src = Matrix.curve(Float(note) / 127, c.0, c.1, c.2, c.3)
            case 8: let c = curves[3]; src = Matrix.curve(modWheel, c.0, c.1, c.2, c.3)
            case 9: src = slide
            default: src = 0
            }
            d[slotDest[s]] += slotAmount[s] * src * scale
        }
    }
}

private extension Voice {
    var hasPlayedControl: Bool { inc1 != 0 }
    mutating func copyControl(_ c: VoiceControl) {
        inc1 = c.inc1; inc2 = c.inc2; G = c.G; k = c.k; hpG = c.hpG
        duty1 = c.duty1; duty2 = c.duty2; mix1 = c.mix1; mix2 = c.mix2
        amp = 0; panL = c.panL; panR = c.panR
    }
}

/// Fixed-capacity list of held notes in press order.
struct NoteList {
    static let capacity = 32
    var notes = [Int](repeating: 0, count: NoteList.capacity)
    var velocities = [Float](repeating: 0, count: NoteList.capacity)
    var count = 0

    mutating func removeAll() { count = 0 }

    mutating func append(note: Int, velocity: Float) {
        if count == NoteList.capacity { remove(at: 0) }
        notes[count] = note; velocities[count] = velocity; count += 1
    }

    mutating func remove(at i: Int) {
        guard i < count else { return }
        for j in i..<(count - 1) { notes[j] = notes[j + 1]; velocities[j] = velocities[j + 1] }
        count -= 1
    }

    mutating func remove(note: Int) {
        var i = 0
        while i < count { if notes[i] == note { remove(at: i) } else { i += 1 } }
    }

    /// Note priority: 0 low, 1 high, 2 last.
    func pick(priority: Int) -> (note: Int, velocity: Float) {
        guard count > 0 else { return (60, 0.8) }
        var best = count - 1
        if priority != 2 {
            for i in 0..<count where priority == 0 ? notes[i] < notes[best] : notes[i] > notes[best] { best = i }
        }
        return (notes[best], velocities[best])
    }
}
