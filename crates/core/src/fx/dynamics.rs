//! Overdrive, compressor and bit crusher.

use super::dsp::*;

const BIAS: f64 = 0.15;
const TANH_BIAS: f64 = 0.148_885_033_623_317_95; // tanh(BIAS)

/// TS808-style overdrive: the clean signal plus a clipped copy of its 720 Hz pre-emphasised,
/// 6 kHz band-limited version, followed by the tone lowpass, a DC blocker and the output level.
/// The clipper is an asymmetric tanh with first-order antiderivative anti-aliasing.
/// Drive 0...1 = stage gain 2...60; Tone 0...1 = 500 Hz...12.5 kHz; Level 0.5 = unity.
/// Main (modulated) parameter: Drive.
pub struct Overdrive {
    p_drive: FxParam,
    p_tone: FxParam,
    p_level: FxParam,
    sr: f32,
    c_sample: f32,
    c_chunk: f32,
    g_pre: f32,
    g_band: f32,
    g_dc: f32,
    gain_t: f32,
    tone_t: f32,
    level_t: f32,
    gain: f32,
    tone: f32,
    level: f32,
    fresh: bool,
    pre_l: OnePole,
    pre_r: OnePole,
    band_l: OnePole,
    band_r: OnePole,
    tone_l: OnePole,
    tone_r: OnePole,
    dc_l: OnePole,
    dc_r: OnePole,
    x_l: f64,
    x_r: f64,
    f_l: f64,
    f_r: f64,
}

#[inline(always)]
fn clip(v: f64) -> f64 {
    (v + BIAS).tanh() - TANH_BIAS
}

#[inline(always)]
fn antiderivative(v: f64) -> f64 {
    let a = (v + BIAS).abs();
    a + (-2.0 * a).exp().ln_1p() - std::f64::consts::LN_2 - v * TANH_BIAS // log(cosh(v + b)) - v·tanh(b)
}

/// First-order ADAA of `clip`.
#[inline(always)]
fn adaa(v: f64, x1: &mut f64, f1: &mut f64) -> f64 {
    let f = antiderivative(v);
    let dv = v - *x1;
    let y = if dv.abs() > 1e-6 { (f - *f1) / dv } else { clip(0.5 * (v + *x1)) };
    *x1 = v;
    *f1 = f;
    y
}

impl Overdrive {
    pub fn new(slot: usize) -> Self {
        Overdrive {
            p_drive: FxParam::slot(slot, "TS808_Drive"),
            p_tone: FxParam::slot(slot, "TS808_Tone"),
            p_level: FxParam::slot(slot, "TS808_Level"),
            sr: 48000.0,
            c_sample: 0.001,
            c_chunk: 0.03,
            g_pre: 0.0,
            g_band: 0.0,
            g_dc: 0.0,
            gain_t: 2.0,
            tone_t: 12500.0,
            level_t: 1.0,
            gain: 2.0,
            tone: 12500.0,
            level: 1.0,
            fresh: true,
            pre_l: OnePole::default(),
            pre_r: OnePole::default(),
            band_l: OnePole::default(),
            band_r: OnePole::default(),
            tone_l: OnePole::default(),
            tone_r: OnePole::default(),
            dc_l: OnePole::default(),
            dc_r: OnePole::default(),
            x_l: 0.0,
            x_r: 0.0,
            f_l: 0.0,
            f_r: 0.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.c_sample = smooth_coef(0.02, sample_rate, 1);
        self.c_chunk = smooth_coef(0.02, sample_rate, FX_CHUNK);
        self.g_pre = one_pole_g(720.0, self.sr);
        self.g_band = one_pole_g(6000.0, self.sr);
        self.g_dc = one_pole_g(12.0, self.sr);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.pre_l = OnePole::default();
        self.pre_r = OnePole::default();
        self.band_l = OnePole::default();
        self.band_r = OnePole::default();
        self.tone_l = OnePole::default();
        self.tone_r = OnePole::default();
        self.dc_l = OnePole::default();
        self.dc_r = OnePole::default();
        self.x_l = 0.0;
        self.x_r = 0.0;
        self.f_l = antiderivative(0.0);
        self.f_r = self.f_l;
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, _tempo: f64) {
        self.gain_t = 2.0 * 30f32.powf(self.p_drive.norm(p, main) as f32);
        self.tone_t = 500.0 * 25f32.powf(self.p_tone.norm(p, 0.0) as f32);
        let lv = 2.0 * self.p_level.norm(p, 0.0) as f32;
        self.level_t = lv * lv;
        if self.fresh {
            self.gain = self.gain_t;
            self.tone = self.tone_t;
            self.level = self.level_t;
            self.fresh = false;
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        self.tone += (self.tone_t - self.tone) * self.c_chunk;
        let g_tone = one_pole_g(self.tone, self.sr);
        let (c, g_t, l_t, gp, gb, gd) = (self.c_sample, self.gain_t, self.level_t, self.g_pre, self.g_band, self.g_dc);
        for i in 0..l.len() {
            self.gain += (g_t - self.gain) * c;
            self.level += (l_t - self.level) * c;
            let (a, b) = (l[i], r[i]);
            let va = (self.band_l.lp(self.pre_l.hp(a, gp), gb) * self.gain) as f64;
            let vb = (self.band_r.lp(self.pre_r.hp(b, gp), gb) * self.gain) as f64;
            let mut ya = a + adaa(va, &mut self.x_l, &mut self.f_l) as f32;
            let mut yb = b + adaa(vb, &mut self.x_r, &mut self.f_r) as f32;
            ya = self.dc_l.hp(self.tone_l.lp(ya, g_tone), gd);
            yb = self.dc_r.hp(self.tone_r.lp(yb, g_tone), gd);
            l[i] = 0.5 * self.level * ya;
            r[i] = 0.5 * self.level * yb;
        }
        for f in [
            &mut self.pre_l,
            &mut self.pre_r,
            &mut self.band_l,
            &mut self.band_r,
            &mut self.tone_l,
            &mut self.tone_r,
            &mut self.dc_l,
            &mut self.dc_r,
        ] {
            f.flush();
        }
    }
}

const KNEE: f32 = 6.0;

/// Feed-forward compressor with a stereo-linked peak detector and a 6 dB soft knee; attack and
/// release smooth the gain reduction in dB. Auto make-up adds half of the static reduction at
/// 0 dBFS. Main (modulated) parameter: Release.
pub struct Compressor {
    p_threshold: FxParam,
    p_ratio: FxParam,
    p_attack: FxParam,
    p_release: FxParam,
    p_input: FxParam,
    p_makeup: FxParam,
    p_output: FxParam,
    sr: f32,
    c_sample: f32,
    thr: f32,
    slope: f32,
    att: f32,
    rel: f32,
    in_t: f32,
    out_t: f32,
    in_g: f32,
    out_g: f32,
    env: f32,
    fresh: bool,
    /// Largest gain reduction (dB) of the last processed chunk, for the UI meter.
    pub reduction_db: f32,
}

impl Compressor {
    pub fn new(slot: usize) -> Self {
        Compressor {
            p_threshold: FxParam::slot(slot, "Compressor_Threshold"),
            p_ratio: FxParam::slot(slot, "Compressor_Ratio"),
            p_attack: FxParam::slot(slot, "Compressor_Attack"),
            p_release: FxParam::slot(slot, "Compressor_Release"),
            p_input: FxParam::slot(slot, "Compressor_InputGain"),
            p_makeup: FxParam::slot(slot, "Compressor_Makeup"),
            p_output: FxParam::slot(slot, "Compressor_OutputGain"),
            sr: 48000.0,
            c_sample: 0.001,
            thr: 0.0,
            slope: 0.0,
            att: 0.0,
            rel: 0.0,
            in_t: 1.0,
            out_t: 1.0,
            in_g: 1.0,
            out_g: 1.0,
            env: 0.0,
            fresh: true,
            reduction_db: 0.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.c_sample = smooth_coef(0.02, sample_rate, 1);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
        self.reduction_db = 0.0;
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, _tempo: f64) {
        self.thr = self.p_threshold.float(p, 0.0);
        let ratio = self.p_ratio.float(p, 0.0).max(1.0);
        self.slope = 1.0 / ratio - 1.0;
        self.att = (-1000.0 / (self.p_attack.float(p, 0.0).max(0.001) * self.sr)).exp();
        self.rel = (-1000.0 / (self.p_release.float(p, main).max(0.001) * self.sr)).exp();
        let makeup = if self.p_makeup.index(p, 0.0) == 1 { -0.5 * self.thr * (1.0 - 1.0 / ratio) } else { 0.0 };
        self.in_t = db_to_gain(self.p_input.float(p, 0.0));
        self.out_t = db_to_gain(self.p_output.float(p, 0.0) + makeup);
        if self.fresh {
            self.in_g = self.in_t;
            self.out_g = self.out_t;
            self.fresh = false;
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let (c, i_t, o_t, t, s, a, rl) = (self.c_sample, self.in_t, self.out_t, self.thr, self.slope, self.att, self.rel);
        let (w, half) = (KNEE, 0.5 * KNEE);
        let mut e = self.env;
        let mut peak = 0.0f32;
        for i in 0..l.len() {
            self.in_g += (i_t - self.in_g) * c;
            self.out_g += (o_t - self.out_g) * c;
            let x = l[i] * self.in_g;
            let y = r[i] * self.in_g;
            let lvl = 8.685_89 * x.abs().max(y.abs()).max(1e-6).ln();
            let over = lvl - t;
            let mut gr = 0.0f32; // desired reduction, dB (>= 0)
            if over > half {
                gr = -s * over;
            } else if over > -half {
                let k = over + half;
                gr = -s * k * k / (2.0 * w);
            }
            e = if gr > e { a * e + (1.0 - a) * gr } else { rl * e + (1.0 - rl) * gr };
            peak = peak.max(e);
            let g = self.out_g * db_to_gain(-e);
            l[i] = x * g;
            r[i] = y * g;
        }
        self.env = flush(e);
        self.reduction_db = peak;
    }
}

/// Bit crusher: sample & hold at sampleRate / Downsample, then a mid-tread quantizer with
/// 2^(bits-1) steps per unit (fractional bit depths allowed).
/// Main (modulated) parameter: Bitdepth.
pub struct BitCrusher {
    p_bits: FxParam,
    p_down: FxParam,
    step: f32,
    ratio: f32,
    count: f32,
    hold_l: f32,
    hold_r: f32,
}

impl BitCrusher {
    pub fn new(slot: usize) -> Self {
        BitCrusher {
            p_bits: FxParam::slot(slot, "BitCrusher_Bitdepth"),
            p_down: FxParam::slot(slot, "BitCrusher_Downsample"),
            step: 32768.0,
            ratio: 1.0,
            count: 0.0,
            hold_l: 0.0,
            hold_r: 0.0,
        }
    }

    pub fn prepare(&mut self, _sample_rate: f64) {
        self.reset();
    }

    pub fn reset(&mut self) {
        self.count = 0.0;
        self.hold_l = 0.0;
        self.hold_r = 0.0;
    }

    pub fn update(&mut self, p: &[f32], main: f32, _tempo: f64) {
        self.step = (self.p_bits.float(p, main) - 1.0).exp2();
        self.ratio = self.p_down.float(p, 0.0).max(1.0);
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let (q, inv, rt) = (self.step, 1.0 / self.step, self.ratio);
        let (mut c, mut hl, mut hr) = (self.count, self.hold_l, self.hold_r);
        for i in 0..l.len() {
            c += 1.0;
            if c >= rt {
                c -= rt;
                hl = (l[i] * q).round() * inv;
                hr = (r[i] * q).round() * inv;
            }
            l[i] = hl;
            r[i] = hr;
        }
        self.count = c;
        self.hold_l = hl;
        self.hold_r = hr;
    }
}
