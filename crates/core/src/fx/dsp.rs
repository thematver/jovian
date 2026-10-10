//! Building blocks shared by the effects. Everything is allocation-free once prepared.

use crate::params::{Curve, Kind, ParamSpec, PARAMS};

/// Samples per processing chunk: smoothed parameters and filter coefficients update at this rate.
pub const FX_CHUNK: usize = 32;

/// Zeroes values small enough to decay into denormals (used on every recursive path).
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-15 {
        0.0
    } else {
        x
    }
}

#[inline(always)]
pub fn db_to_gain(db: f32) -> f32 {
    (db * 0.115_129_25).exp()
}

/// One-pole smoothing coefficient for time constant `tau` (s), advanced every `step` samples.
pub fn smooth_coef(tau: f64, sample_rate: f64, step: usize) -> f32 {
    (1.0 - (-(step as f64) / (tau * sample_rate)).exp()) as f32
}

/// Cubic soft clip: unity slope at 0, reaches ±limit with zero slope at ±1.5·limit.
#[inline(always)]
pub fn soft_clip(x: f32, limit: f32) -> f32 {
    let u = x / (1.5 * limit);
    if u >= 1.0 {
        return limit;
    }
    if u <= -1.0 {
        return -limit;
    }
    1.5 * limit * (u - u * u * u * (1.0 / 3.0))
}

/// sin(2π·p) for p in [0, 1); max error about 0.001 (parabola with one refinement step).
#[inline(always)]
pub fn sine(p: f32) -> f32 {
    let t = if p < 0.5 { p } else { p - 1.0 };
    let y = 8.0 * t * (1.0 - 2.0 * t.abs());
    y * (0.775 + 0.225 * y.abs())
}

/// Triangle in phase with `sine` (0 at p = 0, +1 at p = 0.25).
#[inline(always)]
pub fn triangle(p: f32) -> f32 {
    let t = if p < 0.5 { p } else { p - 1.0 };
    let a = t.abs();
    let v = if a <= 0.25 { 4.0 * a } else { 2.0 - 4.0 * a };
    if t < 0.0 {
        -v
    } else {
        v
    }
}

/// Wraps a phase that is at most one cycle out of [0, 1).
#[inline(always)]
pub fn wrap(p: f32) -> f32 {
    if p >= 1.0 {
        p - 1.0
    } else if p < 0.0 {
        p + 1.0
    } else {
        p
    }
}

/// TPT one-pole coefficient G = g / (1 + g) for a cutoff in Hz.
#[inline(always)]
pub fn one_pole_g(hz: f32, sample_rate: f32) -> f32 {
    let g = (std::f32::consts::PI * hz.max(1.0).min(0.47 * sample_rate) / sample_rate).tan();
    g / (1.0 + g)
}

/// Zero-delay-feedback one-pole filter (lowpass, highpass and allpass outputs).
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePole {
    pub s: f32,
}

impl OnePole {
    #[inline(always)]
    pub fn lp(&mut self, x: f32, g: f32) -> f32 {
        let v = (x - self.s) * g;
        let y = v + self.s;
        self.s = y + v;
        y
    }
    #[inline(always)]
    pub fn hp(&mut self, x: f32, g: f32) -> f32 {
        x - self.lp(x, g)
    }
    pub fn flush(&mut self) {
        self.s = flush(self.s);
    }
}

/// Coefficients of a trapezoidal state-variable filter (A. Simper, "SvfLinearTrapOptimised2").
/// Output = m0·input + m1·band + m2·low; the shelf/bell forms match the RBJ cookbook responses.
#[derive(Debug, Clone, Copy)]
pub struct SvfCoef {
    pub a1: f32,
    pub a2: f32,
    pub a3: f32,
    pub m0: f32,
    pub m1: f32,
    pub m2: f32,
}

impl Default for SvfCoef {
    fn default() -> Self {
        SvfCoef { a1: 1.0, a2: 0.0, a3: 0.0, m0: 1.0, m1: 0.0, m2: 0.0 }
    }
}

impl SvfCoef {
    #[inline(always)]
    pub fn g(hz: f32, sample_rate: f32) -> f32 {
        (std::f32::consts::PI * hz.max(5.0).min(0.47 * sample_rate) / sample_rate).tan()
    }

    fn set(&mut self, g: f32, k: f32) {
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    pub fn lowpass(g: f32, q: f32) -> Self {
        let mut c = SvfCoef::default();
        c.set(g, 1.0 / q);
        c.m0 = 0.0;
        c.m1 = 0.0;
        c.m2 = 1.0;
        c
    }

    pub fn highpass(g: f32, q: f32) -> Self {
        let mut c = SvfCoef::default();
        let k = 1.0 / q;
        c.set(g, k);
        c.m0 = 1.0;
        c.m1 = -k;
        c.m2 = -1.0;
        c
    }

    /// Bandpass normalized to 0 dB at the centre frequency.
    pub fn bandpass(g: f32, q: f32) -> Self {
        let mut c = SvfCoef::default();
        let k = 1.0 / q;
        c.set(g, k);
        c.m0 = 0.0;
        c.m1 = k;
        c.m2 = 0.0;
        c
    }

    pub fn bell(g: f32, q: f32, db: f32) -> Self {
        let mut c = SvfCoef::default();
        let a = 10f32.powf(db / 40.0);
        let k = 1.0 / (q * a);
        c.set(g, k);
        c.m0 = 1.0;
        c.m1 = k * (a * a - 1.0);
        c.m2 = 0.0;
        c
    }

    pub fn low_shelf(g: f32, q: f32, db: f32) -> Self {
        let mut c = SvfCoef::default();
        let a = 10f32.powf(db / 40.0);
        let k = 1.0 / q;
        c.set(g / a.sqrt(), k);
        c.m0 = 1.0;
        c.m1 = k * (a - 1.0);
        c.m2 = a * a - 1.0;
        c
    }

    pub fn high_shelf(g: f32, q: f32, db: f32) -> Self {
        let mut c = SvfCoef::default();
        let a = 10f32.powf(db / 40.0);
        let k = 1.0 / q;
        c.set(g * a.sqrt(), k);
        c.m0 = a * a;
        c.m1 = k * (1.0 - a) * a;
        c.m2 = 1.0 - a * a;
        c
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SvfState {
    pub ic1: f32,
    pub ic2: f32,
}

impl SvfState {
    #[inline(always)]
    pub fn tick(&mut self, v0: f32, c: &SvfCoef) -> f32 {
        let v3 = v0 - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        c.m0 * v0 + c.m1 * v1 + c.m2 * v2
    }

    pub fn flush(&mut self) {
        self.ic1 = flush(self.ic1);
        self.ic2 = flush(self.ic2);
    }
}

/// Power-of-two circular buffer. After `write`, `at(0)` is the newest sample and `at(d)` the
/// sample written d calls earlier. `allocate` is called off the render thread.
#[derive(Debug, Clone)]
pub struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    w: usize,
}

impl Default for DelayLine {
    fn default() -> Self {
        DelayLine { buf: vec![0.0; 4], mask: 3, w: 0 }
    }
}

impl DelayLine {
    /// Reallocates for at least `length` samples of history.
    pub fn allocate(&mut self, length: usize) {
        let mut size = 4;
        while size < length + 4 {
            size <<= 1;
        }
        self.buf = vec![0.0; size];
        self.mask = size - 1;
        self.w = 0;
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.w = 0;
    }

    #[inline(always)]
    pub fn write(&mut self, x: f32) {
        self.w = (self.w + 1) & self.mask;
        self.buf[self.w] = x;
    }

    #[inline(always)]
    pub fn at(&self, d: usize) -> f32 {
        self.buf[self.w.wrapping_sub(d) & self.mask]
    }

    /// Linearly interpolated read, d >= 0.
    #[inline(always)]
    pub fn linear(&self, d: f32) -> f32 {
        let i = d as isize as usize;
        let f = d - i as f32;
        let a = self.buf[self.w.wrapping_sub(i) & self.mask];
        let b = self.buf[self.w.wrapping_sub(i).wrapping_sub(1) & self.mask];
        a + (b - a) * f
    }
}

/// Schroeder allpass of `d` samples (d >= 1): v = x + g·v[n-d], y = v[n-d] - g·v.
#[inline(always)]
pub fn allpass(line: &mut DelayLine, x: f32, d: usize, g: f32) -> f32 {
    let z = line.at(d - 1);
    let v = x + g * z;
    line.write(flush(v));
    z - g * v
}

/// Allpass with a fractional (modulated) length, d >= 1.
#[inline(always)]
pub fn allpass_frac(line: &mut DelayLine, x: f32, d: f32, g: f32) -> f32 {
    let z = line.linear(d - 1.0);
    let v = x + g * z;
    line.write(flush(v));
    z - g * v
}

/// xorshift32 noise for sample & hold LFOs.
#[derive(Debug, Clone, Copy)]
pub struct Random {
    pub s: u32,
}

impl Default for Random {
    fn default() -> Self {
        Random { s: 0x9E37_79B9 }
    }
}

impl Random {
    #[inline(always)]
    pub fn bipolar(&mut self) -> f32 {
        self.s ^= self.s << 13;
        self.s ^= self.s >> 17;
        self.s ^= self.s << 5;
        (self.s as i32) as f32 * (1.0 / 2_147_483_648.0)
    }
}

/// Physical mapping of one effect parameter.
#[derive(Debug, Clone, Copy)]
pub struct FxParam {
    pub addr: usize,
    spec: &'static ParamSpec,
}

impl FxParam {
    pub fn new(id: &str) -> Self {
        let addr = crate::params::address(id).unwrap_or_else(|| panic!("unknown parameter {id}"));
        FxParam { addr, spec: &PARAMS[addr] }
    }

    pub fn slot(slot: usize, name: &str) -> Self {
        Self::new(&format!("FX{}_{}", slot + 1, name))
    }

    /// Normalized value plus a modulation offset, clamped to 0...1.
    #[inline(always)]
    pub fn norm(&self, p: &[f32], m: f32) -> f64 {
        let x = (p[self.addr] + m) as f64;
        if x.is_nan() {
            0.0
        } else {
            x.clamp(0.0, 1.0)
        }
    }

    #[inline(always)]
    pub fn value(&self, p: &[f32], m: f32) -> f64 {
        let x = self.norm(p, m);
        match self.spec.kind {
            Kind::Continuous { curve: Curve::Reverse, min, max, .. } => max - (max - min) * x,
            Kind::Continuous { curve, min, max, .. } => min + (max - min) * curve.shape(x),
            Kind::Discrete(items) => (items.len().max(1) - 1) as f64 * x,
        }
    }

    #[inline(always)]
    pub fn float(&self, p: &[f32], m: f32) -> f32 {
        self.value(p, m) as f32
    }

    #[inline(always)]
    pub fn index(&self, p: &[f32], m: f32) -> usize {
        crate::params::index(self.norm(p, m), self.spec.item_count())
    }
}

/// Note values of a synced-rate item list in bars of 4/4 ("1/16" -> 0.0625).
pub fn bars(id: &str) -> Vec<f64> {
    crate::params::spec(id)
        .items()
        .iter()
        .map(|s| {
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() == 2 {
                if let (Ok(a), Ok(b)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    return a / b;
                }
            }
            s.parse::<f64>().unwrap_or(1.0)
        })
        .collect()
}

/// Duration of `bars` at `bpm`; `kind` 0 binary, 1 triplet (×2/3), 2 dotted (×1.5).
#[inline(always)]
pub fn sync_seconds(bars: f64, kind: usize, bpm: f64) -> f64 {
    let t = if kind == 1 {
        2.0 / 3.0
    } else if kind == 2 {
        1.5
    } else {
        1.0
    };
    bars * t * 240.0 / bpm
}
