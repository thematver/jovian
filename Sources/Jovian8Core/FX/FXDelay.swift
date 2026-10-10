import Foundation

/// Stereo / ping-pong delay with HP and LP filters inside the feedback loop. Time is free
/// (2...2000 ms) or synced to the host tempo (1/32...8 bars, binary/triplet/dotted); the read
/// position glides towards a new time (tape-like). StereoWidth lengthens the right channel by
/// that ratio (stereo mode); StereoWidthPP sets how far the ping-pong echoes are panned.
/// The buffer holds 8 s; longer synced values are clamped. A reset is O(1): reads beyond the
/// samples written since the reset return silence.
/// Main (modulated) parameter: delay time (DelayTimeSynced when synced, DelayTimeMs otherwise).
final class FXDelay {
    private let pMs: FXParam, pSynced: FXParam, pFeedback: FXParam, pHP: FXParam, pLP: FXParam
    private let pSync: FXParam, pSyncType: FXParam, pMode: FXParam, pWidth: FXParam, pWidthPP: FXParam
    private let bars: [Double]

    private static let maxSeconds = 8.0
    private var bufL: UnsafeMutablePointer<Float>, bufR: UnsafeMutablePointer<Float>
    private var cap = 8, w = 0, filled = 0

    private var sr: Float = 48000
    private var cTime: Float = 0.001, cGain: Float = 0.001, cChunk: Float = 0.03
    private var tLT: Float = 1000, tRT: Float = 1000, fbT: Float = 0, hpT: Float = 20, lpT: Float = 20000, ppT: Float = 1
    private var tL: Float = 1000, tR: Float = 1000, fb: Float = 0, hpHz: Float = 20, lpHz: Float = 20000, pp: Float = 1
    private var pingPong = false
    private var fresh = true
    private var hpL = FXOnePole(), hpR = FXOnePole(), lpL = FXOnePole(), lpR = FXOnePole()

    init(slot: Int) {
        pMs = FXParam(slot: slot, "Delay_DelayTimeMs")
        pSynced = FXParam(slot: slot, "Delay_DelayTimeSynced")
        pFeedback = FXParam(slot: slot, "Delay_Feedback")
        pHP = FXParam(slot: slot, "Delay_HPFreq")
        pLP = FXParam(slot: slot, "Delay_LPFreq")
        pSync = FXParam(slot: slot, "Delay_MidiSync")
        pSyncType = FXParam(slot: slot, "Delay_RateSyncedType")
        pMode = FXParam(slot: slot, "Delay_StereoMode")
        pWidth = FXParam(slot: slot, "Delay_StereoWidth")
        pWidthPP = FXParam(slot: slot, "Delay_StereoWidthPP")
        bars = fxBars("FX\(slot + 1)_Delay_DelayTimeSynced")
        bufL = .allocate(capacity: cap); bufL.initialize(repeating: 0, count: cap)
        bufR = .allocate(capacity: cap); bufR.initialize(repeating: 0, count: cap)
    }

    deinit { bufL.deallocate(); bufR.deallocate() }

    func prepare(sampleRate: Double) {
        sr = Float(sampleRate)
        bufL.deallocate(); bufR.deallocate()
        cap = Int(FXDelay.maxSeconds * sampleRate) + 8
        bufL = .allocate(capacity: cap); bufL.initialize(repeating: 0, count: cap)
        bufR = .allocate(capacity: cap); bufR.initialize(repeating: 0, count: cap)
        cTime = fxSmoothCoef(0.06, sampleRate)
        cGain = fxSmoothCoef(0.02, sampleRate)
        cChunk = fxSmoothCoef(0.03, sampleRate, step: fxChunk)
        reset()
    }

    func reset() {
        w = 0; filled = 0
        hpL = FXOnePole(); hpR = FXOnePole(); lpL = FXOnePole(); lpR = FXOnePole()
        fresh = true
    }

    func update(_ p: UnsafePointer<Float>, main: Float, tempo: Double) {
        let synced = pSync.index(p) == 1
        let seconds: Double
        if synced {
            let i = pSynced.index(p, main)
            seconds = fxSyncSeconds(bars[min(i, bars.count - 1)], pSyncType.index(p), tempo)
        } else {
            seconds = pMs.value(p, main) / 1000
        }
        let maxD = Float(cap - 8)
        let t = min(max(Float(seconds) * sr, 2), maxD)
        pingPong = pMode.index(p) == 1
        tLT = t
        tRT = pingPong ? t : min(t * (1 + pWidth.float(p)), maxD)
        fbT = pFeedback.float(p)
        hpT = pHP.float(p)
        lpT = pLP.float(p)
        ppT = pWidthPP.float(p)
        if fresh {
            tL = tLT; tR = tRT; fb = fbT; hpHz = hpT; lpHz = lpT; pp = ppT
            fresh = false
        }
    }

    /// 4-point Hermite read `d` samples back (d >= 2); taps older than the history written since
    /// the last reset read as silence.
    @inline(__always) private func read(_ b: UnsafeMutablePointer<Float>, _ d: Float) -> Float {
        let i = Int(d)
        if i - 1 > filled { return 0 }
        let f = d - Float(i)
        var j = w - i + 1
        if j < 0 { j += cap }
        let y0 = b[j]
        j -= 1; if j < 0 { j += cap }
        var y1 = b[j]
        j -= 1; if j < 0 { j += cap }
        var y2 = b[j]
        j -= 1; if j < 0 { j += cap }
        var y3 = b[j]
        if i + 2 > filled {
            y3 = 0
            if i + 1 > filled { y2 = 0 }
            if i > filled { y1 = 0 }
        }
        let c1 = 0.5 * (y2 - y0)
        let c2 = y0 - 2.5 * y1 + 2 * y2 - 0.5 * y3
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2)
        return ((c3 * f + c2) * f + c1) * f + y1
    }

    func process(_ l: UnsafeMutablePointer<Float>, _ r: UnsafeMutablePointer<Float>, _ n: Int) {
        hpHz += (hpT - hpHz) * cChunk
        lpHz += (lpT - lpHz) * cChunk
        pp += (ppT - pp) * cChunk
        let gHP = fxOnePoleG(hpHz, sr), gLP = fxOnePoleG(lpHz, sr)
        let a = 0.5 * (1 + pp), b = 0.5 * (1 - pp)
        let ct = cTime, cg = cGain, tlT = tLT, trT = tRT, fbTarget = fbT, ping = pingPong
        let bl = bufL, br = bufR

        for i in 0..<n {
            tL += (tlT - tL) * ct
            tR += (trT - tR) * ct
            fb += (fbTarget - fb) * cg
            let oL = read(bl, tL), oR = read(br, tR)
            var vL: Float, vR: Float
            if ping {
                vL = 0.5 * (l[i] + r[i]) + fb * oR
                vR = fb * oL
                l[i] = a * oL + b * oR
                r[i] = b * oL + a * oR
            } else {
                vL = l[i] + fb * oL
                vR = r[i] + fb * oR
                l[i] = oL
                r[i] = oR
            }
            vL = lpL.lp(hpL.hp(vL, gHP), gLP)
            vR = lpR.lp(hpR.hp(vR, gHP), gLP)
            bl[w] = fxFlush(fxSoftClip(vL, 2))
            br[w] = fxFlush(fxSoftClip(vR, 2))
            w += 1
            if w == cap { w = 0 }
            if filled < cap { filled += 1 }
        }
        hpL.flush(); hpR.flush(); lpL.flush(); lpR.flush()
    }
}
