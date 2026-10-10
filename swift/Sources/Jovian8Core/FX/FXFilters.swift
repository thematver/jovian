import Foundation

/// Multi-mode filter. LP/HP/BP: 1...3 cascaded trapezoidal SVF stages (12/24/36 dB); for LP and
/// HP the first stages use the Butterworth Qs of that order and the last stage carries the
/// resonance (Q 0.707 = Butterworth); BP stages are peak-normalized. CombFB / CombFF: comb at
/// CombFreq (delay 1/CombFreq) whose gain comes from the Q knob (the comb screen labels Q
/// "Gain"): g = 1 - 1/(2Q), RMS-normalized.
/// Main (modulated) parameter: Freq (LP/HP/BP) or CombFreq (comb modes), as highlighted in Jup-8 V.
final class FXMultiFilter {
    private let pFreq: FXParam, pQ: FXParam, pMode: FXParam, pSlope: FXParam, pComb: FXParam
    private var sr: Float = 48000, cChunk: Float = 0.03, cSample: Float = 0.001
    private var line = (FXDelayLine(), FXDelayLine())
    private let st: UnsafeMutablePointer<FXSVFState>   // 3 stages L, then 3 stages R
    private var c0 = FXSVFCoef(), c1 = FXSVFCoef(), c2 = FXSVFCoef()
    private var mode = 0, stages = 1
    private var logFT: Float = 7, qT: Float = 0.707, combT: Float = 100, logF: Float = 7, q: Float = 0.707, comb: Float = 100
    private var fresh = true

    init(slot: Int) {
        pFreq = FXParam(slot: slot, "MultiModeFilter_Freq")
        pQ = FXParam(slot: slot, "MultiModeFilter_Q")
        pMode = FXParam(slot: slot, "MultiModeFilter_Mode")
        pSlope = FXParam(slot: slot, "MultiModeFilter_Slope")
        pComb = FXParam(slot: slot, "MultiModeFilter_CombFreq")
        st = .allocate(capacity: 6)
        st.initialize(repeating: FXSVFState(), count: 6)
    }

    deinit { line.0.deallocate(); line.1.deallocate(); st.deallocate() }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        cChunk = fxSmoothCoef(0.03, sampleRate, step: fxChunk)
        cSample = fxSmoothCoef(0.03, sampleRate)
        let n = Int(sampleRate / 20) + 4   // lowest comb frequency 20 Hz
        line.0.allocate(n); line.1.allocate(n)
        reset()
    }

    func reset() {
        line.0.clear(); line.1.clear()
        st.update(repeating: FXSVFState(), count: 6)
        fresh = true
    }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        let m = pMode.index(p)
        let isComb = m >= 3
        // FB stores output, FF stores input: start a comb with an empty line.
        if m != mode && isComb { line.0.clear(); line.1.clear() }
        mode = m
        stages = pSlope.index(p) + 1
        logFT = logf(pFreq.float(p, isComb ? 0 : main))
        qT = pQ.float(p)
        combT = pComb.float(p, isComb ? main : 0)
        if fresh { logF = logFT; q = qT; comb = combT; fresh = false }
    }

    private func updateCoefficients() {
        let g = FXSVFCoef.g(expf(logF), sr)
        switch (mode, stages) {
        case (2, _):
            c0 = .bandpass(g, q: q); c1 = c0; c2 = c0
        case (_, 1):
            c0 = mode == 0 ? .lowpass(g, q: q) : .highpass(g, q: q)
        case (_, 2):
            let q1: Float = 0.541_196, q2 = min(q * 1.847_759, 40)
            c0 = mode == 0 ? .lowpass(g, q: q1) : .highpass(g, q: q1)
            c1 = mode == 0 ? .lowpass(g, q: q2) : .highpass(g, q: q2)
        default:
            let q1: Float = 0.517_638, q2: Float = 0.707_107, q3 = min(q * 2.732_051, 40)
            c0 = mode == 0 ? .lowpass(g, q: q1) : .highpass(g, q: q1)
            c1 = mode == 0 ? .lowpass(g, q: q2) : .highpass(g, q: q2)
            c2 = mode == 0 ? .lowpass(g, q: q3) : .highpass(g, q: q3)
        }
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        logF += (logFT - logF) * cChunk
        q += (qT - q) * cChunk
        if mode >= 3 { processComb(l, r, n); return }
        updateCoefficients()
        let a = c0, b = c1, c = c2, ns = stages
        for i in 0..<n {
            var x = st[0].tick(l[i], a)
            var y = st[3].tick(r[i], a)
            if ns > 1 { x = st[1].tick(x, b); y = st[4].tick(y, b) }
            if ns > 2 { x = st[2].tick(x, c); y = st[5].tick(y, c) }
            l[i] = x
            r[i] = y
        }
        for k in 0..<6 { st[k].flush() }
    }

    private func processComb(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        let g = 1 - 0.5 / max(q, 0.5)
        let feedback = mode == 3
        let norm = feedback ? (1 - g * g).squareRoot() : 1 / (1 + g * g).squareRoot()
        let cT = combT, cs = cSample
        for i in 0..<n {
            comb += (cT - comb) * cs
            let d = sr / comb
            if feedback {
                let yl = l[i] + g * line.0.linear(d - 1)
                let yr = r[i] + g * line.1.linear(d - 1)
                line.0.write(fxFlush(fxSoftClip(yl, 4)))
                line.1.write(fxFlush(fxSoftClip(yr, 4)))
                l[i] = yl * norm
                r[i] = yr * norm
            } else {
                line.0.write(l[i]); line.1.write(r[i])
                l[i] = (l[i] + g * line.0.linear(d)) * norm
                r[i] = (r[i] + g * line.1.linear(d)) * norm
            }
        }
    }
}

/// Five-band parametric EQ: low shelf, three bells, high shelf (RBJ-cookbook responses realised
/// as trapezoidal SVFs, which stay clean under modulation and at low frequency / high rate).
/// All band gains are multiplied by Scale. Parameters glide per chunk.
/// Main (modulated) parameter: Scale.
final class FXParamEQ {
    private static let bands = 5
    /// Per band: frequency, gain (dB), Q.
    private let pBand: [FXParam]
    private let pScale: FXParam
    private let target: UnsafeMutablePointer<Float>   // 15 values: ln(Hz), dB, Q per band
    private let value: UnsafeMutablePointer<Float>
    private let coef: UnsafeMutablePointer<FXSVFCoef>
    private let st: UnsafeMutablePointer<FXSVFState>   // 5 bands L, then 5 bands R
    private var sr: Float = 48000, cChunk: Float = 0.03
    private var scaleT: Float = 1, scale: Float = 1
    /// Jup-8 V shows the slot's Dry/Wet as the EQ's "Scale": it scales the band gains (no crossfade).
    var dryWetScale: Float = 1
    private var settled = false, fresh = true

    init(slot: Int) {
        let names = ["LFShelvCutoff", "LFShelvGain", "LFShelvQ",
                     "PeakCutoff1", "PeakGain1", "PeakBandwidth1",
                     "PeakCutoff2", "PeakGain2", "PeakBandwidth2",
                     "PeakCutoffLast", "PeakGainLast", "PeakBandwidthLast",
                     "HFShelvCutoff", "HFShelvGain", "HFShelvQ"]
        pBand = names.map { FXParam(slot: slot, "ParamEq_" + $0) }
        pScale = FXParam(slot: slot, "ParamEq_Scale")
        target = .allocate(capacity: 15); target.initialize(repeating: 0, count: 15)
        value = .allocate(capacity: 15); value.initialize(repeating: 0, count: 15)
        coef = .allocate(capacity: FXParamEQ.bands); coef.initialize(repeating: FXSVFCoef(), count: FXParamEQ.bands)
        st = .allocate(capacity: 2 * FXParamEQ.bands); st.initialize(repeating: FXSVFState(), count: 2 * FXParamEQ.bands)
    }

    deinit { target.deallocate(); value.deallocate(); coef.deallocate(); st.deallocate() }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        cChunk = fxSmoothCoef(0.03, sampleRate, step: fxChunk)
        reset()
    }

    func reset() {
        st.update(repeating: FXSVFState(), count: 2 * FXParamEQ.bands)
        fresh = true
    }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        for b in 0..<FXParamEQ.bands {
            target[3 * b] = logf(pBand[3 * b].float(p))
            target[3 * b + 1] = pBand[3 * b + 1].float(p)
            target[3 * b + 2] = pBand[3 * b + 2].float(p)
        }
        scaleT = pScale.float(p, main) * dryWetScale
        if fresh {
            value.update(from: target, count: 15)
            scale = scaleT
            fresh = false
            computeCoefficients()
        }
        settled = false
    }

    private func computeCoefficients() {
        for b in 0..<FXParamEQ.bands {
            let g = FXSVFCoef.g(expf(value[3 * b]), sr)
            let db = value[3 * b + 1] * scale, q = max(value[3 * b + 2], 0.05)
            switch b {
            case 0: coef[b] = .lowShelf(g, q: q, db: db)
            case FXParamEQ.bands - 1: coef[b] = .highShelf(g, q: q, db: db)
            default: coef[b] = .bell(g, q: q, db: db)
            }
        }
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        if !settled {
            var delta: Float = abs(scaleT - scale)
            scale += (scaleT - scale) * cChunk
            for k in 0..<15 {
                let d = target[k] - value[k]
                value[k] += d * cChunk
                delta = max(delta, abs(d))
            }
            if delta < 1e-4 {
                value.update(from: target, count: 15)
                scale = scaleT
                settled = true
            }
            computeCoefficients()
        }
        let c = coef, s = st, sR = st + FXParamEQ.bands
        for i in 0..<n {
            var x = l[i], y = r[i]
            for b in 0..<FXParamEQ.bands {
                x = s[b].tick(x, c[b])
                y = sR[b].tick(y, c[b])
            }
            l[i] = x
            r[i] = y
        }
        for k in 0..<(2 * FXParamEQ.bands) { s[k].flush() }
    }
}
