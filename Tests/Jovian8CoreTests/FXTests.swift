import XCTest
import Jovian8Core

final class FXTests: XCTestCase {
    static let typeNames = ["None", "Reverb", "Delay", "Chorus", "Flanger", "Phaser", "Overdrive",
                            "Compressor", "BitCrusher", "Multi Filter", "Param EQ", "Stereo Pan"]

    // MARK: Helpers

    /// Default patch values with the FX section on and all slots active.
    private func patch(routing: Int = 0, types: [Int], dryWet: Float? = nil) -> [Float] {
        var p = ParamTable.all.map { Float($0.defaultValue) }
        setIndex(&p, "FX_Enable", 1)
        setIndex(&p, "FX_Routing", routing)
        for s in 0..<3 {
            setIndex(&p, "FX\(s + 1)_ModuleType", s < types.count ? types[s] : 0)
            setIndex(&p, "FX\(s + 1)_Bypass", 1)
            if let w = dryWet { p[ParamTable.address("FX\(s + 1)_Dry_Wet")] = w }
        }
        return p
    }

    private func setIndex(_ p: inout [Float], _ id: String, _ index: Int) {
        let s = ParamTable.spec(id)
        p[s.address] = Float(ParamSpec.normalized(index: index, count: s.itemCount))
    }

    private func setPhysical(_ p: inout [Float], _ id: String, _ v: Double) {
        let s = ParamTable.spec(id)
        p[s.address] = Float(s.normalized(fromPhysical: v))
    }

    private struct LCG {
        var s: UInt32
        mutating func next() -> Float {
            s = s &* 1_664_525 &+ 1_013_904_223
            return Float(s >> 8) / Float(1 << 24) * 2 - 1
        }
    }

    /// Noise at about -6 dBFS plus a unit impulse at the start, followed by `silence` seconds.
    private func signal(_ sr: Double, seconds: Double, silence: Double = 0, seed: UInt32 = 1) -> ([Float], [Float]) {
        let n = Int(sr * seconds), total = n + Int(sr * silence)
        var g = LCG(s: seed)
        var l = [Float](repeating: 0, count: total), r = l
        for i in 0..<n { l[i] = 0.5 * g.next(); r[i] = 0.5 * g.next() }
        l[0] = 1; r[0] = 1
        return (l, r)
    }

    /// Runs `rack` over the input in irregular block sizes.
    private func run(_ rack: FXRack, _ p: [Float], _ inL: [Float], _ inR: [Float], tempo: Double = 120,
                     mod: [Float]? = nil, blocks: [Int] = [512, 37, 1, 256, 4096, 100, 33]) -> ([Float], [Float]) {
        var l = inL, r = inR
        let count = l.count
        p.withUnsafeBufferPointer { pp in
            l.withUnsafeMutableBufferPointer { lb in
                r.withUnsafeMutableBufferPointer { rb in
                    var pos = 0, k = 0
                    while pos < count {
                        let n = min(blocks[k % blocks.count], count - pos)
                        if let m = mod {
                            m.withUnsafeBufferPointer { mb in
                                rack.process(left: lb.baseAddress! + pos, right: rb.baseAddress! + pos, frames: n,
                                             params: pp.baseAddress!, tempo: tempo, mod: mb.baseAddress!)
                            }
                        } else {
                            rack.process(left: lb.baseAddress! + pos, right: rb.baseAddress! + pos, frames: n,
                                         params: pp.baseAddress!, tempo: tempo, mod: nil)
                        }
                        pos += n; k += 1
                    }
                }
            }
        }
        return (l, r)
    }

    private func check(_ out: ([Float], [Float]), bound: Float = 8, _ what: String) {
        var peak: Float = 0
        for x in out.0 + out.1 {
            guard x.isFinite else { return XCTFail("\(what): non-finite output") }
            peak = max(peak, abs(x))
        }
        XCTAssertLessThan(peak, bound, "\(what): peak \(peak)")
    }

    private func rms(_ x: ArraySlice<Float>) -> Float {
        (x.reduce(Float(0)) { $0 + $1 * $1 } / Float(max(x.count, 1))).squareRoot()
    }

    private func makeRack(_ sr: Double, maxFrames: Int = 4096) -> FXRack {
        let rack = FXRack()
        rack.prepare(sampleRate: sr, maxFrames: maxFrames)
        return rack
    }

    // MARK: Safety

    /// Every type, alone and stacked in three slots, both routings, 44.1 and 96 kHz.
    func testAllTypesFiniteAndBounded() {
        for sr in [44100.0, 96000.0] {
            let input = signal(sr, seconds: 0.6, silence: 0.3)
            for routing in 0...1 {
                for t in 1...11 {
                    let alone = run(makeRack(sr), patch(routing: routing, types: [t]), input.0, input.1)
                    check(alone, "\(FXTests.typeNames[t]) alone, routing \(routing), \(sr) Hz")
                    let stack = [t, t % 11 + 1, (t + 1) % 11 + 1]
                    let out = run(makeRack(sr), patch(routing: routing, types: stack), input.0, input.1)
                    check(out, "\(stack.map { FXTests.typeNames[$0] }), routing \(routing), \(sr) Hz")
                }
            }
        }
    }

    /// Dry/Wet = 0 must return the input unchanged (no effect here adds latency to the dry path).
    func testDryWetZeroIsTransparent() {
        for sr in [44100.0, 96000.0] {
            let input = signal(sr, seconds: 0.3)
            for routing in 0...1 {
                for t in 1...11 {
                    let out = run(makeRack(sr), patch(routing: routing, types: [t, t, t], dryWet: 0), input.0, input.1)
                    var err: Float = 0
                    for i in 0..<input.0.count {
                        err = max(err, abs(out.0[i] - input.0[i]), abs(out.1[i] - input.1[i]))
                    }
                    XCTAssertLessThan(err, 1e-4, "\(FXTests.typeNames[t]) routing \(routing) \(sr) Hz")
                }
            }
        }
    }

    /// Type, bypass, routing and FX on/off changes while audio runs, plus modulation and odd tempos.
    func testSwitchingWhileProcessing() {
        for sr in [44100.0, 96000.0] {
            let rack = makeRack(sr)
            var g = LCG(s: 7)
            var p = patch(types: [1, 2, 3])
            let input = signal(sr, seconds: 0.05, seed: 3)
            let tempos = [120.0, 0, -5, .nan, 40, 300, 1e9]
            for block in 0..<300 {
                let slot = block % 3 + 1
                setIndex(&p, "FX\(slot)_ModuleType", Int((g.next() + 1) * 6) % 12)
                if block % 7 == 0 { setIndex(&p, "FX\(slot)_Bypass", block % 14 == 0 ? 0 : 1) }
                if block % 11 == 0 { setIndex(&p, "FX_Routing", (block / 11) % 2) }
                if block % 37 == 0 { setIndex(&p, "FX_Enable", (block / 37) % 2 == 0 ? 1 : 0) }
                let mod = (0..<6).map { _ in g.next() }
                let out = run(rack, p, input.0, input.1, tempo: tempos[block % tempos.count], mod: mod,
                              blocks: [Int((g.next() + 1) * 300) + 1])
                check(out, "switch block \(block) at \(sr) Hz")
            }
        }
    }

    /// Random normalized values for every FX parameter (gains up to +54 dB are legal, so only
    /// finiteness and stability are checked).
    func testRandomParametersStayFinite() {
        let fxAddresses = ParamTable.all.filter { $0.id.hasPrefix("FX") }.map(\.address)
        for sr in [44100.0, 96000.0] {
            var g = LCG(s: 11)
            for trial in 0..<40 {
                var p = patch(types: [trial % 11 + 1, (trial + 4) % 11 + 1, (trial + 8) % 11 + 1])
                for a in fxAddresses where !ParamTable.all[a].id.hasSuffix("ModuleType") && !ParamTable.all[a].id.hasSuffix("Bypass")
                    && ParamTable.all[a].id != "FX_Enable" {
                    p[a] = (g.next() + 1) / 2
                }
                let input = signal(sr, seconds: 0.15, silence: 0.05, seed: UInt32(trial + 1))
                let out = run(makeRack(sr), p, input.0, input.1, mod: (0..<6).map { _ in g.next() })
                check(out, bound: 1e4, "random trial \(trial) at \(sr) Hz")
            }
        }
    }

    /// Every FX parameter at its minimum and at its maximum (longest delays, largest reverb,
    /// 8 bars synced at 40 BPM, 20 Hz comb...) at 192 kHz and 22.05 kHz.
    func testExtremeSettingsAtHighAndLowRates() {
        let fxParams = ParamTable.all.filter {
            $0.id.hasPrefix("FX") && !$0.id.hasSuffix("ModuleType") && !$0.id.hasSuffix("Bypass") && $0.id != "FX_Enable"
        }
        for sr in [192000.0, 22050.0] {
            let input = signal(sr, seconds: 0.1, silence: 0.1)
            for extreme: Float in [0, 1] {
                for t in 1...11 {
                    var p = patch(types: [t, t, t])
                    for s in fxParams { p[s.address] = extreme }
                    let out = run(makeRack(sr), p, input.0, input.1, tempo: 40, mod: [1, 1, 1, 1, 1, 1])
                    check(out, bound: 1e4, "\(FXTests.typeNames[t]) all params \(extreme) at \(sr) Hz")
                }
            }
        }
    }

    // MARK: Behaviour

    func testParallelWithOneSlotMatchesSerial() {
        let input = signal(48000, seconds: 0.3)
        for t in 1...11 {
            let a = run(makeRack(48000), patch(routing: 0, types: [0, t, 0]), input.0, input.1)
            let b = run(makeRack(48000), patch(routing: 1, types: [0, t, 0]), input.0, input.1)
            var err: Float = 0
            for i in 0..<a.0.count { err = max(err, abs(a.0[i] - b.0[i]), abs(a.1[i] - b.1[i])) }
            XCTAssertLessThan(err, 1e-5, FXTests.typeNames[t])
        }
    }

    private func impulseResponse(_ p: [Float], sr: Double = 48000, seconds: Double = 1, tempo: Double = 120) -> [Float] {
        var l = [Float](repeating: 0, count: Int(sr * seconds)), r = l
        l[0] = 1; r[0] = 1
        return run(makeRack(sr), p, l, r, tempo: tempo).0
    }

    private func peakIndex(_ x: [Float], from: Int) -> Int {
        var best = from
        for i in from..<x.count where abs(x[i]) > abs(x[best]) { best = i }
        return best
    }

    func testDelayTimes() {
        var p = patch(types: [2], dryWet: 1)
        setIndex(&p, "FX1_Delay_MidiSync", 0)
        setPhysical(&p, "FX1_Delay_DelayTimeMs", 300)
        p[ParamTable.address("FX1_Delay_Feedback")] = 0
        p[ParamTable.address("FX1_Delay_StereoWidth")] = 0
        let ir = impulseResponse(p)
        // The in-loop lowpass (20 kHz at its default) spreads the impulse over a few samples.
        XCTAssertEqual(peakIndex(ir, from: 1), 14400)
        XCTAssertGreaterThan(ir[14400], 0.7)

        setIndex(&p, "FX1_Delay_MidiSync", 1)
        setIndex(&p, "FX1_Delay_DelayTimeSynced", 3)       // 1/4 bar = one beat
        setIndex(&p, "FX1_Delay_RateSyncedType", 2)        // dotted
        let synced = impulseResponse(p, tempo: 120)
        XCTAssertEqual(peakIndex(synced, from: 1), 36000)
        XCTAssertGreaterThan(synced[36000], 0.7)
    }

    func testPingPongAlternates() {
        var p = patch(types: [2], dryWet: 1)
        setIndex(&p, "FX1_Delay_MidiSync", 0)
        setIndex(&p, "FX1_Delay_StereoMode", 1)
        setPhysical(&p, "FX1_Delay_DelayTimeMs", 100)
        setPhysical(&p, "FX1_Delay_StereoWidthPP", 1)
        setPhysical(&p, "FX1_Delay_Feedback", 0.5)
        var l = [Float](repeating: 0, count: 24000), r = l
        l[0] = 1; r[0] = 1
        let out = run(makeRack(48000), p, l, r)
        XCTAssertGreaterThan(out.0[4800], 0.7)                      // 1st echo left
        XCTAssertLessThan(abs(out.1[4800]), 1e-3)
        XCTAssertGreaterThan(out.1[9600], 0.25)                     // 2nd echo right, × feedback
        XCTAssertLessThan(abs(out.0[9600]), 1e-3)
        XCTAssertGreaterThan(out.0[14400], 0.1)                     // 3rd echo left again
        XCTAssertLessThan(abs(out.1[14400]), 1e-3)
    }

    /// Turning the section off and on, or leaving a type and coming back, starts from silence.
    func testOldTailsAreCleared() {
        let sr = 48000.0
        var p = patch(types: [2], dryWet: 1)
        setIndex(&p, "FX1_Delay_MidiSync", 0)
        setPhysical(&p, "FX1_Delay_DelayTimeMs", 500)
        setPhysical(&p, "FX1_Delay_Feedback", 0.9)
        let noise = signal(sr, seconds: 0.2)
        let silence = [Float](repeating: 0, count: Int(sr))
        for change in ["enable", "type"] {
            let rack = makeRack(sr)
            _ = run(rack, p, noise.0, noise.1)                    // fill the delay line
            var q = p
            if change == "enable" { setIndex(&q, "FX_Enable", 0) } else { setIndex(&q, "FX1_ModuleType", 1) }
            _ = run(rack, q, Array(silence[0..<4800]), Array(silence[0..<4800]))   // fade out (10 ms) and switch
            let out = run(rack, p, silence, silence)
            XCTAssertLessThan(out.0.map(abs).max()!, 1e-6, change)
        }
    }

    func testReverbTailGrowsWithDecay() {
        func tail(_ decay: Double) -> Float {
            var p = patch(types: [1], dryWet: 1)
            setPhysical(&p, "FX1_Dattorro_Decay", decay)
            let ir = impulseResponse(p, seconds: 2)
            return rms(ir[48000..<96000])
        }
        let short = tail(0.3), long = tail(0.9)
        XCTAssertGreaterThan(long, short * 4)
        XCTAssertGreaterThan(short, 0)
    }

    func testCompressorReducesLoudSignal() {
        var p = patch(types: [7], dryWet: 1)
        setPhysical(&p, "FX1_Compressor_Threshold", -30)
        setPhysical(&p, "FX1_Compressor_Ratio", 8)
        let rack = makeRack(48000)
        let input = signal(48000, seconds: 0.5)
        let out = run(rack, p, input.0, input.1)
        XCTAssertLessThan(rms(out.0[12000...]), rms(input.0[12000...]) * 0.4)
        XCTAssertGreaterThan(rack.compressorReductionDB(slot: 0), 10)
    }

    func testEQBoostAndNeutral() {
        let sr = 48000.0
        let sine = (0..<Int(sr / 2)).map { Float(0.25 * sin(2 * Double.pi * 1000 * Double($0) / sr)) }
        var p = patch(types: [10], dryWet: 1)
        let flat = run(makeRack(sr), p, sine, sine)
        var err: Float = 0
        for i in 0..<sine.count { err = max(err, abs(flat.0[i] - sine[i])) }
        XCTAssertLessThan(err, 1e-4, "all gains at 0 dB must be transparent")

        setPhysical(&p, "FX1_ParamEq_PeakCutoff2", 1000)
        setPhysical(&p, "FX1_ParamEq_PeakGain2", 12)
        let boosted = run(makeRack(sr), p, sine, sine)
        let ratio = rms(boosted.0[12000...]) / rms(sine[12000...])
        XCTAssertEqual(ratio, powf(10, 12 / 20), accuracy: 0.15)
    }

    func testMainParameterModulation() {
        // A +1 offset on FX1 Param drives the bit depth to 1.5 bits: heavy distortion.
        let p = patch(types: [8], dryWet: 1)
        let input = signal(48000, seconds: 0.2)
        let clean = run(makeRack(48000), p, input.0, input.1)
        let crushed = run(makeRack(48000), p, input.0, input.1, mod: [0, 0, 0, 1, 0, 0])
        var eClean: Float = 0, eCrushed: Float = 0
        for i in 0..<input.0.count {
            eClean = max(eClean, abs(clean.0[i] - input.0[i]))
            eCrushed = max(eCrushed, abs(crushed.0[i] - input.0[i]))
        }
        XCTAssertLessThan(eClean, 1e-4)
        XCTAssertGreaterThan(eCrushed, 0.1)
    }

    // MARK: Performance

    /// Reverb + Delay + Chorus on 10 s of stereo at 48 kHz. Run with
    /// `FX_BENCH=1 swift test -c release --filter FXTests/testRealtimeFactor`.
    func testRealtimeFactor() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["FX_BENCH"] != nil, "set FX_BENCH=1 to benchmark")
        let sr = 48000.0, seconds = 10.0, block = 256
        let rack = makeRack(sr, maxFrames: block)
        let p = patch(types: [1, 2, 3])
        let input = signal(sr, seconds: seconds)
        var l = input.0, r = input.1
        let start = DispatchTime.now().uptimeNanoseconds
        p.withUnsafeBufferPointer { pp in
            l.withUnsafeMutableBufferPointer { lb in
                r.withUnsafeMutableBufferPointer { rb in
                    var pos = 0
                    while pos < lb.count {
                        let n = min(block, lb.count - pos)
                        rack.process(left: lb.baseAddress! + pos, right: rb.baseAddress! + pos, frames: n,
                                     params: pp.baseAddress!, tempo: 120, mod: nil)
                        pos += n
                    }
                }
            }
        }
        let elapsed = Double(DispatchTime.now().uptimeNanoseconds - start) / 1e9
        print(String(format: "FX realtime factor (Reverb+Delay+Chorus, 48 kHz): %.1fx (%.3f s for %.0f s)",
                     seconds / elapsed, elapsed, seconds))
        XCTAssert(l.allSatisfy(\.isFinite))
    }
}
