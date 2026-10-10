import Foundation

/// Destinations of the Jup-8 V modulation matrix (`ModDestV1` list order, None excluded).
enum Dest {
    static let vco1Coarse = 0, vco2Coarse = 1, vco12Coarse = 2, vco2Fine = 3, vcoLfo = 4, vcoEnv = 5
    static let vco1PW = 6, vco2PW = 7, pwAmount = 8, crossMod = 9, mix = 10, hpf = 11, cutoff = 12, reso = 13
    static let vcfEnv = 14, vcfLfo = 15, vcfKey = 16, vcaLevel = 17, vcaLfo = 18
    static let e1A = 19, e1D = 20, e1S = 21, e1R = 22, e2A = 23, e2D = 24, e2S = 25, e2R = 26
    static let detune = 27, pan = 28, porta = 29, lfo1Rate = 30, lfo1Delay = 31
    static let lfo2Rate = 32, lfo2Phase = 33, lfo2Warp = 34, lfo2Fade = 35
    static let lfo3Rate = 36, lfo3Phase = 37, lfo3Warp = 38, lfo3Fade = 39
    static let arpRate = 40, fx1Mix = 47, fx2Mix = 48, fx3Mix = 49, fx1Param = 50, fx2Param = 51, fx3Param = 52
    static let count = 53
}

/// The 24 fixed source/destination slots of the matrix and the four keyboard response curves.
enum Matrix {
    struct Slot { let source: Int; let destAddress: Int; let amountAddress: Int }
    // source numbering: 1 LFO2, 2 LFO3, 3 Mod Mixer, 4 Seq Mod, 5 Velocity, 6 Aftertouch, 7 Keyboard, 8 Mod Wheel, 9 MPE Slide
    static let slots: [Slot] = {
        let layout: [(Int, [Int])] = [(1, [1, 2]), (2, [3, 4]), (3, [5, 6, 7]), (4, [8, 9, 10]), (5, [11, 12, 13]),
                                      (6, [14, 15, 16]), (7, [17, 18, 19]), (8, [20, 21, 22]), (9, [23, 24])]
        return layout.flatMap { src, dests in
            dests.map { d in Slot(source: src, destAddress: ParamTable.address("ModMatrixDest\(d)"),
                                  amountAddress: ParamTable.address("ModMatrixAmount_Src\(src)_Dest\(d)")) }
        }
    }()

    /// Keyboard1 = velocity, 2 = aftertouch, 3 = key tracking, 4 = mod wheel; each is (Y0, X1, Y1, Y2).
    static let curveAddresses: [(Int, Int, Int, Int)] = (1...4).map { k in
        (ParamTable.address("Keyboard\(k)_Y0"), ParamTable.address("Keyboard\(k)_X1"),
         ParamTable.address("Keyboard\(k)_Y1"), ParamTable.address("Keyboard\(k)_Y2"))
    }

    @inline(__always)
    static func curve(_ x: Float, _ y0: Float, _ x1: Float, _ y1: Float, _ y2: Float) -> Float {
        let x = min(max(x, 0), 1)
        if x <= x1 { return x1 <= 0 ? y1 : y0 + (y1 - y0) * x / x1 }
        return x1 >= 1 ? y1 : y1 + (y2 - y1) * (x - x1) / (1 - x1)
    }
}

/// Addresses of the two hybrid LFOs (Jup-8 V calls them "LFO 2/3", stored as LFO1_* / LFO2_*).
struct HybridLFOAddresses {
    let wave, rateMode, rate, rateSync, fade, warp, phase, loop, retrig, unipolar, poly: Int
    init(_ i: Int) {
        func a(_ s: String) -> Int { ParamTable.address("LFO\(i)_\(s)") }
        wave = a("Waveform"); rateMode = a("RateMode"); rate = a("RateUnSynced"); rateSync = a("RateSynced")
        fade = a("Fade"); warp = a("PhaseDist"); phase = a("PhaseInit"); loop = a("LoopMode")
        retrig = a("Retrig"); unipolar = a("Unipolar"); poly = a("Poly")
    }
    static let lfo2 = HybridLFOAddresses(1), lfo3 = HybridLFOAddresses(2)
}

/// Settings of one hybrid LFO for the current control block.
struct HybridLFOSettings {
    var wave = 0, hz = 1.0, fadeSec = 0.0, warp = 0.0, phase0 = 0.0, single = false, retrig = false, unipolar = false, poly = true

    mutating func update(_ p: UnsafePointer<Float>, _ a: HybridLFOAddresses, tempo: Double) {
        let f = ParamTable.fast
        wave = f[a.wave].index(p[a.wave])
        let mode = f[a.rateMode].index(p[a.rateMode])
        if mode == 0 {
            hz = f[a.rate].physical(p[a.rate])
        } else {
            let bars = RateLists.hybridBars[f[a.rateSync].index(p[a.rateSync])] * (mode == 2 ? 2.0 / 3 : mode == 3 ? 1.5 : 1)
            hz = RateLists.hz(bars: bars, tempo: tempo)
        }
        fadeSec = f[a.fade].physical(p[a.fade])
        warp = f[a.warp].physical(p[a.warp])
        phase0 = f[a.phase].physical(p[a.phase]) / 360
        single = f[a.loop].index(p[a.loop]) == 0
        retrig = p[a.retrig] >= 0.5
        unipolar = p[a.unipolar] >= 0.5
        poly = p[a.poly] >= 0.5
    }
}

/// Running state of one hybrid LFO instance (global or per voice).
struct HybridLFOState {
    var phase = 0.0, fade: Float = 1, sh: Float = 0, done = false, value: Float = 0
    var noise = Noise()

    mutating func trigger(_ s: HybridLFOSettings) {
        if s.retrig || s.single { phase = s.phase0; done = false }
        fade = s.fadeSec > 0 ? 0 : 1
    }

    @inline(__always)
    mutating func advance(_ s: HybridLFOSettings, dt: Double, rateMul: Double, warpOffset: Double) -> Float {
        if !done {
            phase += s.hz * rateMul * dt
            if phase >= 1 {
                if s.single { phase = 1; done = true } else { phase -= phase.rounded(.down); sh = noise.next() }
            }
        }
        if fade < 1 { fade = min(1, fade + Float(dt / max(s.fadeSec, 1e-4))) }
        // phase distortion ("warp"): bends the phase towards the start or the end of the cycle
        let w = min(max(s.warp + warpOffset, -1), 1)
        var ph = min(phase, 0.999_999)
        if w > 0 { ph = pow(ph, 1 + 3 * w) } else if w < 0 { ph = 1 - pow(1 - ph, 1 - 3 * w) }
        var v: Double
        switch s.wave {
        case 0: v = sin(2 * Double.pi * ph)
        case 1: v = ph < 0.25 ? 4 * ph : ph < 0.75 ? 2 - 4 * ph : 4 * ph - 4
        case 2: v = 2 * ph - 1
        case 3: v = 1 - 2 * ph
        case 4: v = ph < 0.5 ? 1 : -1
        default: v = Double(sh)
        }
        if s.unipolar { v = 0.5 * v + 0.5 }
        value = Float(v) * fade
        return value
    }
}
