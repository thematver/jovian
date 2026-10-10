import Foundation

/// One synth voice: VCO-1 / VCO-2 (sync, cross-mod) -> mixer -> HPF -> VCF -> VCA.
/// Plain data only, so voices live in a preallocated buffer and never touch the allocator.
struct Voice {
    // assignment
    var active = false
    var gate = false
    var sustained = false
    var note: Int = 60
    var velocity: Float = 0.8
    var age: UInt64 = 0
    var stackIndex = 0
    var stackSize = 1
    var hasPlayed = false

    // pitch (semitones, MIDI note scale)
    var pitch = 60.0
    var targetPitch = 60.0

    // oscillators
    var phase1: Float = 0
    var phase2: Float = 0
    var xm: Float = 0
    var noise = Noise()

    // filters
    var hpf = OnePoleHP()
    var ladder = Ladder()

    // envelopes & per-voice modulators
    var env1 = Envelope()
    var env2 = Envelope()
    var lfoFade: Float = 1
    var lfo2 = HybridLFOState()
    var lfo3 = HybridLFOState()

    // fixed per-voice "analog" deviations in [-1, 1] (dispersion)
    var rPitch1: Float = 0, rPitch2: Float = 0, rPW: Float = 0, rEnv: Float = 0
    var rCut: Float = 0, rRes: Float = 0, rMod: Float = 0, panPos: Float = 0

    // control-rate results, interpolated across the block
    var inc1: Float = 0, inc2: Float = 0
    var G: Float = 0.5, k: Float = 0
    var hpG: Float = 0
    var duty1: Float = 0.5, duty2: Float = 0.5
    var mix1: Float = 0.707, mix2: Float = 0.707
    var amp: Float = 0
    var panL: Float = 0.707, panR: Float = 0.707

    mutating func randomize(seed: UInt32) {
        var n = Noise(state: seed | 1)
        for _ in 0..<4 { _ = n.next() }
        rPitch1 = n.next(); rPitch2 = n.next(); rPW = n.next(); rEnv = n.next()
        rCut = n.next(); rRes = n.next(); rMod = n.next()
        noise = Noise(state: seed &* 747_796_405 | 1)
        lfo2.noise = Noise(state: seed &+ 12345)
        lfo3.noise = Noise(state: seed &+ 54321)
        phase1 = abs(n.next())
        phase2 = abs(n.next())
    }

    /// Renders `count` oversampled samples, adding into `outL`/`outR`.
    /// Inc/G/amp move linearly from the previous control values to the new targets.
    @inline(__always)
    mutating func render(count: Int, outL: UnsafeMutablePointer<Float>, outR: UnsafeMutablePointer<Float>,
                         c: VoiceControl, wave1: Int, wave2: Int, sync: Int, crossMod: Float, twoPole: Bool) {
        let invN = 1 / Float(count)
        let dInc1 = (c.inc1 - inc1) * invN, dInc2 = (c.inc2 - inc2) * invN
        let dG = (c.G - G) * invN, dAmp = (c.amp - amp) * invN
        let dPanL = (c.panL - panL) * invN, dPanR = (c.panR - panR) * invN
        k = c.k; hpG = c.hpG; duty1 = c.duty1; duty2 = c.duty2; mix1 = c.mix1; mix2 = c.mix2
        let comp = 1 + 0.11 * k
        let xmScale = Float(VoiceMath.crossModOctaves) * crossMod
        let sineLevel = Float(VoiceMath.vco2SineLevel / VoiceMath.oscLevel)
        let noiseLevel = Float(VoiceMath.noiseLevel / VoiceMath.oscLevel)

        for i in 0..<count {
            inc1 += dInc1; inc2 += dInc2; G += dG; amp += dAmp; panL += dPanL; panR += dPanR

            // VCO-2 (master for cross-mod, and for sync when VCO-1 is the slave)
            phase2 += inc2
            var wrapped2 = false
            if phase2 >= 1 { phase2 -= 1; wrapped2 = true }

            // VCO-1 with exponential cross modulation from VCO-2
            var i1 = inc1
            if xmScale > 0 { i1 *= fastExp2(xmScale * xm) }
            i1 = min(i1, 0.45)
            phase1 += i1
            var wrapped1 = false
            if phase1 >= 1 { phase1 -= 1; wrapped1 = true }
            if sync == 0 && wrapped2 {
                phase1 = min(phase2 * i1 / max(inc2, 1e-9), 0.999)
            } else if sync == 2 && wrapped1 {
                phase2 = min(phase1 * inc2 / max(i1, 1e-9), 0.999)
            }

            let o1: Float
            switch wave1 {
            case 0: o1 = Wave.triangle(phase1)
            case 1: o1 = Wave.saw(phase1, i1)
            case 2: o1 = Wave.pulse(phase1, i1, duty1)
            default: o1 = Wave.pulse(phase1, i1, 0.5)
            }
            let o2: Float
            switch wave2 {
            case 0:
                let s = fastSin2Pi(phase2)
                o2 = s * sineLevel
                xm = s * 0.4
            case 1:
                o2 = Wave.saw(phase2, inc2)
                xm = 0.5 * o2 + 0.5          // the saw drives cross-mod unipolar, like the pulse
            case 2:
                o2 = Wave.pulse(phase2, inc2, duty2)
                xm = 0.5 * o2 + 0.5
            default:
                let nz = noise.next()
                o2 = nz * noiseLevel
                xm = nz * 0.4
            }

            var s = o1 * mix1 + o2 * mix2
            s = hpf.process(s, g: hpG)
            let y = ladder.process(s * comp * 0.35, G: G, k: k, twoPole: twoPole) * (1 / 0.35)
            let out = y * amp
            outL[i] += out * panL
            outR[i] += out * panR
        }
    }
}

/// Control-rate targets computed by the engine for one voice.
struct VoiceControl {
    var inc1: Float = 0, inc2: Float = 0
    var G: Float = 0.5, k: Float = 0, hpG: Float = 0
    var duty1: Float = 0.5, duty2: Float = 0.5
    var mix1: Float = 0.707, mix2: Float = 0.707
    var amp: Float = 0
    var panL: Float = 0.707, panR: Float = 0.707
}
