//! Scalings measured from Jup-8 V by rendering it offline and analysing the output, so that a
//! normalized preset value produces the same physical behaviour in this engine.

// VCF: resonant peak frequency for normalized cutoff x (fit over 0...1, error < 4 %).
pub const CUTOFF_OFFSET_HZ: f64 = 20.0;
pub const CUTOFF_BASE_HZ: f64 = 18.8;
pub const CUTOFF_OCTAVES: f64 = 9.25;

#[inline(always)]
pub fn cutoff_hz(x: f64) -> f64 {
    CUTOFF_OFFSET_HZ + CUTOFF_BASE_HZ * (CUTOFF_OCTAVES * x).exp2()
}

/// Normalized cutoff for a frequency in Hz (inverse of [`cutoff_hz`]).
pub fn cutoff_normalized(hz: f64) -> f64 {
    ((hz - CUTOFF_OFFSET_HZ).max(1.0) / CUTOFF_BASE_HZ).log2() / CUTOFF_OCTAVES
}

/// Ladder feedback gain (4 = self oscillation in the linear model) for normalized resonance.
#[inline(always)]
pub fn resonance_k(r: f64) -> f64 {
    3.74 * r.max(0.0).powf(0.62) // the reference does not self-oscillate at maximum
}

// Normalized-cutoff offsets per unit of modulation.
pub const ENV_TO_CUTOFF: f64 = 0.75; // VCF ENV MOD 1.0 at env = 1
pub const LFO_TO_CUTOFF: f64 = 0.356; // VCF LFO MOD 1.0 at lfo = ±1
pub const MOD_WHEEL_TO_CUTOFF: f64 = 0.9; // MOD lever VCF depth 1.0, lever full, lfo = ±1
pub const BEND_TO_CUTOFF: f64 = 0.715; // times amount², full bend
pub const KEY_FOLLOW_OCT_PER_OCT: f64 = 1.165; // VCF KYBD 1.0, pivot C4 (note 60)

// Pitch modulation.
pub const LFO_PITCH_SCALE: f64 = 0.906; // relative to the displayed semitones
pub const MOD_WHEEL_PITCH_SEMITONES: f64 = 10.8; // times amount^1.92, lever full
pub const MOD_WHEEL_PITCH_EXPONENT: f64 = 1.92;
pub const CROSS_MOD_OCTAVES: f64 = 6.0; // exponential FM, per unit of source
pub const VCO2_LOW_BASE_HZ: f64 = 7.72; // VCO-2 LOW ignores the keyboard

// Envelopes: time constants relative to the displayed time D (ms).
/// Attack charges towards `ATTACK_TARGET` with tau = ATTACK_TAU_FACTOR * D and ends at 1.0.
pub const ATTACK_TARGET: f64 = 1.5;
pub const ATTACK_TAU_FACTOR: f64 = 0.932;
pub const DECAY_TAU_DIVISOR: f64 = 7.9;
pub const RELEASE_TAU_DIVISOR: f64 = 5.6;

/// ENV KEY FOLLOW: time multiplier for a note; lengthens the envelope below ~C7, at most x1.92.
#[inline(always)]
pub fn env_key_follow(note: f64) -> f64 {
    1.0 + 0.92f64.min(0.61 * 0.46f64.powf((note - 60.0) / 12.0))
}

/// VCA LEVEL response (gain = level^exponent); ENV-2 always drives the VCA.
pub const VCA_LEVEL_EXPONENT: f64 = 0.85;
/// VCA LFO depth response (depth = amount^exponent, unipolar dip).
pub const VCA_LFO_EXPONENT: f64 = 1.75;

/// Pulse width for PWM depth p (0...1): duty = 0.5 + 0.5 p, limited to 95 %.
#[inline(always)]
pub fn pulse_duty(p: f64) -> f64 {
    (0.5 + 0.5 * p).min(0.95)
}

/// Oscillator level so that a full-scale waveform matches the reference output.
pub const OSC_LEVEL: f64 = 0.883;
pub const VCO2_SINE_LEVEL: f64 = 0.80;
pub const NOISE_LEVEL: f64 = 0.28;
pub const UNISON_DETUNE_SCALE: f64 = 1.0;
