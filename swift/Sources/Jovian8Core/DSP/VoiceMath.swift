import Foundation

/// Scalings measured from Jup-8 V by rendering it offline and analysing the output, so that a
/// normalized preset value produces the same physical behaviour in this engine.
public enum VoiceMath {
    // VCF: resonant peak frequency for normalized cutoff x (fit over 0...1, error < 4 %).
    public static let cutoffOffsetHz = 20.0
    public static let cutoffBaseHz = 18.8
    public static let cutoffOctaves = 9.25

    @inline(__always)
    public static func cutoffHz(_ x: Double) -> Double {
        cutoffOffsetHz + cutoffBaseHz * exp2(cutoffOctaves * x)
    }

    /// Ladder feedback gain (4 = self oscillation in the linear model) for normalized resonance.
    @inline(__always)
    public static func resonanceK(_ r: Double) -> Double {
        3.74 * pow(max(0, r), 0.62)   // the reference does not self-oscillate at maximum
    }

    // Normalized-cutoff offsets per unit of modulation.
    public static let envToCutoff = 0.75            // VCF ENV MOD 1.0 at env = 1
    public static let lfoToCutoff = 0.356           // VCF LFO MOD 1.0 at lfo = ±1
    public static let modWheelToCutoff = 0.9        // MOD lever VCF depth 1.0, lever full, lfo = ±1
    public static let bendToCutoff = 0.715          // times amount², full bend
    public static let keyFollowOctPerOct = 1.165    // VCF KYBD 1.0, pivot C4 (note 60)

    // Pitch modulation.
    public static let lfoPitchScale = 0.906         // relative to the displayed semitones
    public static let modWheelPitchSemitones = 10.8 // times amount^1.92, lever full
    public static let modWheelPitchExponent = 1.92
    public static let crossModOctaves = 6.0         // exponential FM, per unit of source
    public static let vco2LowBaseHz = 7.72          // VCO-2 LOW ignores the keyboard

    // Envelopes: time constants relative to the displayed time D (ms).
    /// Attack charges towards `attackTarget` with tau = attackTauFactor * D and ends at 1.0 (≈ 1.02 D).
    public static let attackTarget = 1.5
    public static let attackTauFactor = 0.932
    public static let decayTauDivisor = 7.9
    public static let releaseTauDivisor = 5.6

    /// ENV KEY FOLLOW: time multiplier for a note; lengthens the envelope below ~C7, at most x1.92.
    @inline(__always)
    public static func envKeyFollow(_ note: Double) -> Double {
        1 + min(0.92, 0.61 * pow(0.46, (note - 60) / 12))
    }

    /// VCA LEVEL response (gain = level^exponent); ENV-2 always drives the VCA.
    public static let vcaLevelExponent = 0.85
    /// VCA LFO depth response (depth = amount^exponent, unipolar dip).
    public static let vcaLfoExponent = 1.75

    /// Pulse width for PWM depth p (0...1): duty = 0.5 + 0.5 p, limited to 95 %.
    @inline(__always)
    public static func pulseDuty(_ p: Double) -> Double { min(0.5 + 0.5 * p, 0.95) }

    /// Oscillator level so that a full-scale waveform matches the reference output (peak 0.52 at
    /// -4.59 dB volume).
    public static let oscLevel = 0.883
    public static let vco2SineLevel = 0.80
    public static let noiseLevel = 0.28
}
