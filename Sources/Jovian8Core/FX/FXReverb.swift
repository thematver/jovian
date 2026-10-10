import Foundation

/// Dattorro plate (J. Dattorro, "Effect Design Part 1", JAES 1997): predelay, input band-limit
/// (LP + HP), four input diffusers and the figure-eight tank with modulated allpasses, damping
/// and decay; the stereo outputs are tapped as in the paper. Lengths are the paper's values at
/// 29761 Hz, rescaled to the sample rate; Size scales the tank. Decay is the tank gain, Damping
/// the paper's damping coefficient, MSMix the side gain (0 mono ... 0.5 full width).
/// Main (modulated) parameter: Decay.
final class FXReverb {
    private let pDecay: FXParam, pDamping: FXParam, pSize: FXParam, pHP: FXParam, pLP: FXParam
    private let pMS: FXParam, pPre: FXParam

    private static let baseRate: Float = 29761
    private static let inputLengths: [Float] = [142, 107, 379, 277]
    private static let tankLengths: [Float] = [672, 4453, 1800, 3720, 908, 4217, 2656, 3163]
    private static let excursion: Float = 8
    private static let maxSize: Float = 1.5
    private static let maxPredelay: Float = 0.2

    /// 0 predelay, 1...4 input diffusers, 5...12 tank (AP 672, D 4453, AP 1800, D 3720, AP 908,
    /// D 4217, AP 2656, D 3163).
    private let d: UnsafeMutablePointer<FXDelayLine>
    private static let lineCount = 13

    private var sr: Float = 48000, k29: Float = 1
    private var cSample: Float = 0.001, cChunk: Float = 0.03
    private var inLen = (0, 0, 0, 0)
    private var lfoInc: Double = 0, phase: Double = 0

    // Targets (set once per host block) and smoothed values.
    private var decayT: Float = 0.5, dampT: Float = 0, sizeT: Float = 1, hpT: Float = 30, lpT: Float = 20000
    private var widthT: Float = 1, preT: Float = 0
    private var decay: Float = 0.5, damp: Float = 0, size: Float = 1, hpHz: Float = 30, lpHz: Float = 20000
    private var width: Float = 1, pre: Float = 0
    private var fresh = true

    private var lpIn = FXOnePole(), hpIn = FXOnePole()
    private var dampL: Float = 0, dampR: Float = 0, fbR: Float = 0

    init(slot: Int) {
        pDecay = FXParam(slot: slot, "Dattorro_Decay")
        pDamping = FXParam(slot: slot, "Dattorro_Damping")
        pSize = FXParam(slot: slot, "Dattorro_E_Size")
        pHP = FXParam(slot: slot, "Dattorro_HPBandWith")
        pLP = FXParam(slot: slot, "Dattorro_LPBandWith")
        pMS = FXParam(slot: slot, "Dattorro_MSMix")
        pPre = FXParam(slot: slot, "Dattorro_Predelay")
        d = .allocate(capacity: FXReverb.lineCount)
        for i in 0..<FXReverb.lineCount { (d + i).initialize(to: FXDelayLine()) }
    }

    deinit {
        for i in 0..<FXReverb.lineCount { d[i].deallocate() }
        d.deallocate()
    }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        k29 = sr / FXReverb.baseRate
        cSample = fxSmoothCoef(0.05, sampleRate)
        cChunk = fxSmoothCoef(0.08, sampleRate, step: fxChunk)
        lfoInc = 1.0 / sampleRate
        d[0].allocate(Int(FXReverb.maxPredelay * sr) + 4)
        for i in 0..<4 { d[1 + i].allocate(Int(FXReverb.inputLengths[i] * k29) + 4) }
        inLen = (Int(142 * k29), Int(107 * k29), Int(379 * k29), Int(277 * k29))
        let exc = Int(FXReverb.excursion * k29) + 4
        for i in 0..<8 { d[5 + i].allocate(Int(FXReverb.tankLengths[i] * k29 * FXReverb.maxSize) + exc) }
        reset()
    }

    func reset() {
        for i in 0..<FXReverb.lineCount { d[i].clear() }
        lpIn = FXOnePole(); hpIn = FXOnePole()
        dampL = 0; dampR = 0; fbR = 0; phase = 0
        fresh = true
    }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        decayT = pDecay.float(p, main)
        dampT = min(pDamping.float(p), 0.99)
        sizeT = pSize.float(p)
        hpT = pHP.float(p)
        lpT = pLP.float(p)
        widthT = min(2 * pMS.float(p), 1)
        preT = pPre.float(p) * sr
        if fresh {
            decay = decayT; damp = dampT; size = sizeT; hpHz = hpT; lpHz = lpT; width = widthT; pre = preT
            fresh = false
        }
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        let c = cChunk
        decay += (decayT - decay) * c
        damp += (dampT - damp) * c
        size += (sizeT - size) * c
        hpHz += (hpT - hpHz) * c
        lpHz += (lpT - lpHz) * c
        width += (widthT - width) * c

        let gLP = fxOnePoleG(lpHz, sr), gHP = fxOnePoleG(hpHz, sr)
        let dec = decay
        let dd2 = min(max(decay + 0.15, 0.25), 0.5)
        // Damping coefficient is defined at 29761 Hz: same cutoff at any rate.
        let pd = damp > 0 ? powf(damp, FXReverb.baseRate / sr) : 0
        let k = k29 * size
        let t5 = 672 * k, t6 = 4453 * k, t7 = 1800 * k, t8 = 3720 * k
        let t9 = 908 * k, t10 = 4217 * k, t11 = 2656 * k, t12 = 3163 * k
        let exc = FXReverb.excursion * k29
        @inline(__always) func o(_ x: Float) -> Int { Int(x * k) }
        let oL0 = o(266), oL1 = o(2974), oL2 = o(1913), oL3 = o(1996), oL4 = o(1990), oL5 = o(187), oL6 = o(1066)
        let oR0 = o(353), oR1 = o(3627), oR2 = o(1228), oR3 = o(2673), oR4 = o(2111), oR5 = o(335), oR6 = o(121)
        let (i1, i2, i3, i4) = inLen
        let side = 0.5 * width
        let cs = cSample, preTarget = preT, inc = lfoInc

        var lp = lpIn, hp = hpIn
        var dl = dampL, dr = dampR, fr = fbR, ph = phase, pr = pre
        let d = self.d

        for i in 0..<n {
            d[0].write(0.5 * (l[i] + r[i]))
            pr += (preTarget - pr) * cs
            var s = d[0].linear(pr)
            s = hp.hp(lp.lp(s, gLP), gHP)
            s = fxAllpass(&d[1], s, i1, 0.75)
            s = fxAllpass(&d[2], s, i2, 0.75)
            s = fxAllpass(&d[3], s, i3, 0.625)
            s = fxAllpass(&d[4], s, i4, 0.625)

            ph += inc
            if ph >= 1 { ph -= 1 }
            let mL = fxSine(Float(ph)) * exc
            let mR = fxSine(fxWrap(Float(ph) + 0.25)) * exc

            // Left half of the tank.
            var a = fxAllpass(&d[5], s + fr, frac: t5 + mL, -0.7)
            d[6].write(fxFlush(a))
            a = d[6].linear(t6)
            dl = a + pd * (dl - a)
            a = fxAllpass(&d[7], dl * dec, frac: t7, dd2)
            d[8].write(fxFlush(a))
            let fl = d[8].linear(t8) * dec

            // Right half.
            var b = fxAllpass(&d[9], s + fl, frac: t9 + mR, -0.7)
            d[10].write(fxFlush(b))
            b = d[10].linear(t10)
            dr = b + pd * (dr - b)
            b = fxAllpass(&d[11], dr * dec, frac: t11, dd2)
            d[12].write(fxFlush(b))
            fr = d[12].linear(t12) * dec

            let yl = d[10].at(oL0) + d[10].at(oL1) - d[11].at(oL2) + d[12].at(oL3)
                - d[6].at(oL4) - d[7].at(oL5) - d[8].at(oL6)
            let yr = d[6].at(oR0) + d[6].at(oR1) - d[7].at(oR2) + d[8].at(oR3)
                - d[10].at(oR4) - d[11].at(oR5) - d[12].at(oR6)
            let mid = 0.3 * (yl + yr), sd = 0.3 * (yl - yr) * side * 2
            l[i] = mid + sd
            r[i] = mid - sd
        }

        lp.flush(); hp.flush()
        lpIn = lp; hpIn = hp
        dampL = fxFlush(dl); dampR = fxFlush(dr); fbR = fxFlush(fr); phase = ph; pre = pr
    }
}
