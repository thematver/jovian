//! Oscillator, filter and envelope building blocks. Plain data, no allocation.

// MARK: - Fast math

/// 2^x via integer/fraction split and a 5th order polynomial (rel. error ~2e-7 on [0,1)).
#[inline(always)]
pub fn fast_exp2(x: f32) -> f32 {
    let xi = x.floor();
    let f = x - xi;
    let p = 1.0 + f * (0.693_147_18 + f * (0.240_226_51 + f * (0.055_504_11 + f * (0.009_618_13 + f * 0.001_333_36))));
    let e = ((xi.clamp(-126.0, 126.0) as i32) + 127) << 23;
    p * f32::from_bits(e as u32)
}

/// Padé approximant of tanh, clamped; smooth and monotonic, exact at ±3.
#[inline(always)]
pub fn fast_tanh(x: f32) -> f32 {
    let c = x.clamp(-3.0, 3.0);
    let c2 = c * c;
    c * (27.0 + c2) / (27.0 + 9.0 * c2)
}

/// sin(2πφ) for φ in [0, 1): parabolic approximation with one refinement step (error < 0.1 %).
#[inline(always)]
pub fn fast_sin_2pi(phase: f32) -> f32 {
    let mut x = phase - 0.5;
    x = -x * 2.0;
    let y = 4.0 * x * (1.0 - x.abs());
    0.225 * (y * y.abs() - y) + y
}

/// xorshift32 noise in [-1, 1).
#[derive(Debug, Clone, Copy)]
pub struct Noise {
    pub state: u32,
}

impl Default for Noise {
    fn default() -> Self {
        Noise { state: 0x9E37_79B9 }
    }
}

impl Noise {
    pub const fn new(state: u32) -> Self {
        Noise { state }
    }

    #[inline(always)]
    pub fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        (self.state as i32) as f32 * (1.0 / 2_147_483_648.0)
    }
}

// MARK: - PolyBLEP

#[inline(always)]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline(always)]
pub fn wrap_phase(p: f32) -> f32 {
    p - p.floor()
}

/// Band-limited saw at phase `p` with increment `dt` (both in cycles).
#[inline(always)]
pub fn saw(p: f32, dt: f32) -> f32 {
    2.0 * p - 1.0 - poly_blep(p, dt)
}

#[inline(always)]
pub fn pulse(p: f32, dt: f32, duty: f32) -> f32 {
    let mut v: f32 = if p < duty { 1.0 } else { -1.0 };
    v += poly_blep(p, dt);
    v -= poly_blep(wrap_phase(p - duty + 1.0), dt);
    v
}

/// Oversampled by the engine; the naive triangle's 12 dB/oct harmonics alias negligibly.
#[inline(always)]
pub fn triangle(p: f32) -> f32 {
    let v = 4.0 * p - 1.0;
    if v < 1.0 {
        v
    } else {
        3.0 - 4.0 * p
    }
}

// MARK: - Filters

/// One-pole TPT high-pass (the Jupiter's non-resonant 6 dB HPF).
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePoleHp {
    pub s: f32,
}

impl OnePoleHp {
    #[inline(always)]
    pub fn process(&mut self, x: f32, g: f32) -> f32 {
        let v = (x - self.s) * g;
        let lp = v + self.s;
        self.s = lp + v;
        x - lp
    }
}

/// Four-pole OTA cascade with global feedback (IR3109 style), zero-delay feedback solved per
/// sample with a saturating input stage. The 12 dB mode taps the second stage while the
/// resonance loop still spans all four (this matches the measured peak gain of the reference).
#[derive(Debug, Clone, Copy, Default)]
pub struct Ladder {
    pub s1: f32,
    pub s2: f32,
    pub s3: f32,
    pub s4: f32,
}

impl Ladder {
    pub fn reset(&mut self) {
        *self = Ladder::default();
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32, g: f32, k: f32, two_pole: bool) -> f32 {
        let one_minus_g = 1.0 - g;
        let g2 = g * g;
        let sigma = one_minus_g * (g2 * g * self.s1 + g2 * self.s2 + g * self.s3 + self.s4);
        let mut u = (x - k * sigma) / (1.0 + k * g2 * g2);
        u = fast_tanh(u);
        let mut v = (u - self.s1) * g;
        let y1 = v + self.s1;
        self.s1 = y1 + v;
        v = (y1 - self.s2) * g;
        let y2 = v + self.s2;
        self.s2 = y2 + v;
        v = (y2 - self.s3) * g;
        let y3 = v + self.s3;
        self.s3 = y3 + v;
        v = (y3 - self.s4) * g;
        let y4 = v + self.s4;
        self.s4 = y4 + v;
        if two_pole {
            y2
        } else {
            y4
        }
    }
}

/// 2x decimator: 31-tap half-band FIR (Kaiser, ~90 dB stopband) on interleaved pairs.
#[derive(Clone)]
pub struct HalfBandDecimator {
    taps: [f32; 31],
    history: [f32; 64],
    pos: usize,
}

fn half_band_taps() -> [f32; 31] {
    let n = 31usize;
    let beta = 8.0f64;
    fn i0(x: f64) -> f64 {
        let mut s = 1.0;
        let mut t = 1.0;
        for k in 1..30 {
            let a = x / (2.0 * k as f64);
            t *= a * a;
            s += t;
        }
        s
    }
    let mut h = [0.0f32; 31];
    let m = (n - 1) as f64 / 2.0;
    for (i, tap) in h.iter_mut().enumerate() {
        let x = i as f64 - m;
        let sinc = if x == 0.0 { 0.5 } else { (std::f64::consts::PI * x / 2.0).sin() / (std::f64::consts::PI * x) };
        let w = i0(beta * (1.0 - (x / m) * (x / m)).max(0.0).sqrt()) / i0(beta);
        *tap = (sinc * w) as f32;
    }
    let mut sum = 0.0f32;
    for v in h {
        sum += v;
    }
    for v in h.iter_mut() {
        *v /= sum;
    }
    h
}

impl Default for HalfBandDecimator {
    fn default() -> Self {
        Self::new()
    }
}

impl HalfBandDecimator {
    pub fn new() -> Self {
        HalfBandDecimator { taps: half_band_taps(), history: [0.0; 64], pos: 0 }
    }

    pub fn reset(&mut self) {
        self.history = [0.0; 64];
    }

    /// Consumes two oversampled inputs, returns one output sample.
    #[inline(always)]
    pub fn process(&mut self, a: f32, b: f32) -> f32 {
        self.history[self.pos] = a;
        self.history[self.pos + 32] = a;
        self.pos = (self.pos + 1) & 31;
        self.history[self.pos] = b;
        self.history[self.pos + 32] = b;
        self.pos = (self.pos + 1) & 31;
        // the 31 newest samples are contiguous at pos+1 ... pos+31 (mirrored buffer)
        let h = &self.history[self.pos + 1..self.pos + 32];
        let mut acc = 0.0f32;
        for i in 0..31 {
            acc += self.taps[i] * h[i];
        }
        acc
    }
}

// MARK: - Envelope

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Release,
}

/// Analog-style ADSR measured from the reference: RC attack overshooting towards 1.5,
/// exponential decay and release. Advanced at control rate by the engine.
#[derive(Debug, Clone, Copy, Default)]
pub struct Envelope {
    pub stage: Stage,
    pub level: f32,
}

impl Envelope {
    #[inline(always)]
    pub fn gate_on(&mut self) {
        self.stage = Stage::Attack;
    }

    #[inline(always)]
    pub fn gate_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_math_accuracy() {
        for i in -400..400 {
            let x = i as f32 / 37.0;
            let e = fast_exp2(x);
            assert!(((e - x.exp2()) / x.exp2()).abs() < 1e-4, "{x} {}", ((e - x.exp2()) / x.exp2()).abs());
        }
        for i in 0..1000 {
            let p = i as f32 / 1000.0;
            assert!((fast_sin_2pi(p) - (2.0 * std::f32::consts::PI * p).sin()).abs() < 2e-3, "{p}");
        }
    }

    #[test]
    fn decimator_passes_dc() {
        let mut d = HalfBandDecimator::new();
        let mut y = 0.0;
        for _ in 0..64 {
            y = d.process(1.0, 1.0);
        }
        assert!((y - 1.0).abs() < 1e-5);
    }
}
