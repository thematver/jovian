import Foundation

/// TS808-style overdrive: the clean signal plus a clipped copy of its 720 Hz pre-emphasised,
/// 6 kHz band-limited version (the op-amp stage with diodes in its feedback path), followed by
/// the tone lowpass, a DC blocker and the output level. The clipper is an asymmetric tanh with
/// first-order antiderivative anti-aliasing. Drive 0...1 = stage gain 2...60 (6...36 dB);
/// Tone 0...1 = 500 Hz...12.5 kHz; Level 0.5 = unity, 1 = +12 dB.
/// Main (modulated) parameter: Drive.
final class FXOverdrive {
    private let pDrive: FXParam, pTone: FXParam, pLevel: FXParam
    private static let bias = 0.15
    private static let tanhBias = 0.148_885_033_623_317_95   // tanh(bias), literal: no lazy init on the render thread
    private var sr: Float = 48000, cSample: Float = 0.001, cChunk: Float = 0.03
    private var gPre: Float = 0, gBand: Float = 0, gDC: Float = 0
    private var gainT: Float = 2, toneT: Float = 12500, levelT: Float = 1
    private var gain: Float = 2, tone: Float = 12500, level: Float = 1
    private var fresh = true
    private var preL = FXOnePole(), preR = FXOnePole(), bandL = FXOnePole(), bandR = FXOnePole()
    private var toneL = FXOnePole(), toneR = FXOnePole(), dcL = FXOnePole(), dcR = FXOnePole()
    private var xL = 0.0, xR = 0.0, fL = 0.0, fR = 0.0

    init(slot: Int) {
        pDrive = FXParam(slot: slot, "TS808_Drive")
        pTone = FXParam(slot: slot, "TS808_Tone")
        pLevel = FXParam(slot: slot, "TS808_Level")
    }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        cSample = fxSmoothCoef(0.02, sampleRate)
        cChunk = fxSmoothCoef(0.02, sampleRate, step: fxChunk)
        gPre = fxOnePoleG(720, sr)
        gBand = fxOnePoleG(6000, sr)
        gDC = fxOnePoleG(12, sr)
        reset()
    }

    func reset() {
        preL = FXOnePole(); preR = FXOnePole(); bandL = FXOnePole(); bandR = FXOnePole()
        toneL = FXOnePole(); toneR = FXOnePole(); dcL = FXOnePole(); dcR = FXOnePole()
        xL = 0; xR = 0; fL = FXOverdrive.antiderivative(0); fR = fL
        fresh = true
    }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        gainT = 2 * powf(30, Float(pDrive.norm(p, main)))
        toneT = 500 * powf(25, Float(pTone.norm(p)))
        let lv = 2 * Float(pLevel.norm(p))
        levelT = lv * lv
        if fresh { gain = gainT; tone = toneT; level = levelT; fresh = false }
    }

    @inline(__always) private static func clip(_ v: Double) -> Double { tanh(v + bias) - tanhBias }

    @inline(__always) private static func antiderivative(_ v: Double) -> Double {
        let a = abs(v + bias)
        return a + log1p(exp(-2 * a)) - M_LN2 - v * tanhBias   // log(cosh(v + b)) - v·tanh(b)
    }

    /// First-order ADAA of `clip`.
    @inline(__always) private static func adaa(_ v: Double, _ x1: inout Double, _ f1: inout Double) -> Double {
        let f = antiderivative(v)
        let dv = v - x1
        let y = abs(dv) > 1e-6 ? (f - f1) / dv : clip(0.5 * (v + x1))
        x1 = v; f1 = f
        return y
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        tone += (toneT - tone) * cChunk
        let gTone = fxOnePoleG(tone, sr)
        let c = cSample, gT = gainT, lT = levelT, gp = gPre, gb = gBand, gd = gDC
        for i in 0..<n {
            gain += (gT - gain) * c
            level += (lT - level) * c
            let a = l[i], b = r[i]
            let va = Double(bandL.lp(preL.hp(a, gp), gb) * gain)
            let vb = Double(bandR.lp(preR.hp(b, gp), gb) * gain)
            var ya = a + Float(FXOverdrive.adaa(va, &xL, &fL))
            var yb = b + Float(FXOverdrive.adaa(vb, &xR, &fR))
            ya = dcL.hp(toneL.lp(ya, gTone), gd)
            yb = dcR.hp(toneR.lp(yb, gTone), gd)
            l[i] = 0.5 * level * ya
            r[i] = 0.5 * level * yb
        }
        preL.flush(); preR.flush(); bandL.flush(); bandR.flush()
        toneL.flush(); toneR.flush(); dcL.flush(); dcR.flush()
    }
}

/// Feed-forward compressor with a stereo-linked peak detector and a 6 dB soft knee; attack and
/// release smooth the gain reduction in dB (Giannoulis/Massberg/Reiss "smooth branching").
/// Auto make-up adds half of the static reduction at 0 dBFS: -Threshold·(1 - 1/Ratio)/2.
/// Main (modulated) parameter: Release (the knob Jup-8 V highlights as modulable).
final class FXCompressor {
    private let pThreshold: FXParam, pRatio: FXParam, pAttack: FXParam, pRelease: FXParam
    private let pInput: FXParam, pMakeup: FXParam, pOutput: FXParam
    private static let knee: Float = 6
    private var sr: Float = 48000, cSample: Float = 0.001
    private var thr: Float = 0, slope: Float = 0, att: Float = 0, rel: Float = 0
    private var inT: Float = 1, outT: Float = 1, inG: Float = 1, outG: Float = 1
    private var env: Float = 0
    private var fresh = true
    /// Largest gain reduction (dB) of the last processed chunk, for the UI meter.
    private(set) var reductionDB: Float = 0

    init(slot: Int) {
        pThreshold = FXParam(slot: slot, "Compressor_Threshold")
        pRatio = FXParam(slot: slot, "Compressor_Ratio")
        pAttack = FXParam(slot: slot, "Compressor_Attack")
        pRelease = FXParam(slot: slot, "Compressor_Release")
        pInput = FXParam(slot: slot, "Compressor_InputGain")
        pMakeup = FXParam(slot: slot, "Compressor_Makeup")
        pOutput = FXParam(slot: slot, "Compressor_OutputGain")
    }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        cSample = fxSmoothCoef(0.02, sampleRate)
        reset()
    }

    func reset() { env = 0; reductionDB = 0; fresh = true }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        thr = pThreshold.float(p)
        let ratio = max(pRatio.float(p), 1)
        slope = 1 / ratio - 1
        att = expf(-1000 / (max(pAttack.float(p), 0.001) * sr))
        rel = expf(-1000 / (max(pRelease.float(p, main), 0.001) * sr))
        let makeup: Float = pMakeup.index(p) == 1 ? -0.5 * thr * (1 - 1 / ratio) : 0
        inT = fxDbToGain(pInput.float(p))
        outT = fxDbToGain(pOutput.float(p) + makeup)
        if fresh { inG = inT; outG = outT; fresh = false }
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        let c = cSample, iT = inT, oT = outT, t = thr, s = slope, a = att, rl = rel
        let w = FXCompressor.knee, half = 0.5 * FXCompressor.knee
        var e = env, peak: Float = 0
        for i in 0..<n {
            inG += (iT - inG) * c
            outG += (oT - outG) * c
            let x = l[i] * inG, y = r[i] * inG
            let lvl = 8.685_89 * logf(max(max(abs(x), abs(y)), 1e-6))
            let over = lvl - t
            var gr: Float = 0   // desired reduction, dB (>= 0)
            if over > half {
                gr = -s * over
            } else if over > -half {
                let k = over + half
                gr = -s * k * k / (2 * w)
            }
            e = gr > e ? a * e + (1 - a) * gr : rl * e + (1 - rl) * gr
            peak = max(peak, e)
            let g = outG * fxDbToGain(-e)
            l[i] = x * g
            r[i] = y * g
        }
        env = fxFlush(e)
        reductionDB = peak
    }
}

/// Bit crusher: sample & hold at sampleRate / Downsample, then a mid-tread quantizer with
/// 2^(bits-1) steps per unit (fractional bit depths allowed).
/// Main (modulated) parameter: Bitdepth.
final class FXBitCrusher {
    private let pBits: FXParam, pDown: FXParam
    private var step: Float = 32768, ratio: Float = 1
    private var count: Float = 0, holdL: Float = 0, holdR: Float = 0

    init(slot: Int) {
        pBits = FXParam(slot: slot, "BitCrusher_Bitdepth")
        pDown = FXParam(slot: slot, "BitCrusher_Downsample")
    }

    func prepare(sampleRate: Double) { reset() }

    func reset() { count = 0; holdL = 0; holdR = 0 }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        step = exp2f(pBits.float(p, main) - 1)
        ratio = max(pDown.float(p), 1)
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        let q = step, inv = 1 / step, rt = ratio
        var c = count, hl = holdL, hr = holdR
        for i in 0..<n {
            c += 1
            if c >= rt {
                c -= rt
                hl = (l[i] * q).rounded() * inv
                hr = (r[i] * q).rounded() * inv
            }
            l[i] = hl
            r[i] = hr
        }
        count = c; holdL = hl; holdR = hr
    }
}
