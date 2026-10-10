//! Multi-mode filter and parametric EQ.

use super::dsp::*;

/// Multi-mode filter. LP/HP/BP: 1...3 cascaded trapezoidal SVF stages (12/24/36 dB); for LP and
/// HP the first stages use the Butterworth Qs of that order and the last stage carries the
/// resonance; BP stages are peak-normalized. CombFB / CombFF: comb at CombFreq whose gain comes
/// from the Q knob: g = 1 - 1/(2Q), RMS-normalized.
/// Main (modulated) parameter: Freq (LP/HP/BP) or CombFreq (comb modes).
pub struct MultiFilter {
    p_freq: FxParam,
    p_q: FxParam,
    p_mode: FxParam,
    p_slope: FxParam,
    p_comb: FxParam,
    sr: f32,
    c_chunk: f32,
    c_sample: f32,
    line: [DelayLine; 2],
    /// 3 stages L, then 3 stages R
    st: [SvfState; 6],
    c0: SvfCoef,
    c1: SvfCoef,
    c2: SvfCoef,
    mode: usize,
    stages: usize,
    log_f_t: f32,
    q_t: f32,
    comb_t: f32,
    log_f: f32,
    q: f32,
    comb: f32,
    fresh: bool,
}

impl MultiFilter {
    pub fn new(slot: usize) -> Self {
        MultiFilter {
            p_freq: FxParam::slot(slot, "MultiModeFilter_Freq"),
            p_q: FxParam::slot(slot, "MultiModeFilter_Q"),
            p_mode: FxParam::slot(slot, "MultiModeFilter_Mode"),
            p_slope: FxParam::slot(slot, "MultiModeFilter_Slope"),
            p_comb: FxParam::slot(slot, "MultiModeFilter_CombFreq"),
            sr: 48000.0,
            c_chunk: 0.03,
            c_sample: 0.001,
            line: Default::default(),
            st: [SvfState::default(); 6],
            c0: SvfCoef::default(),
            c1: SvfCoef::default(),
            c2: SvfCoef::default(),
            mode: 0,
            stages: 1,
            log_f_t: 7.0,
            q_t: 0.707,
            comb_t: 100.0,
            log_f: 7.0,
            q: 0.707,
            comb: 100.0,
            fresh: true,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.c_chunk = smooth_coef(0.03, sample_rate, FX_CHUNK);
        self.c_sample = smooth_coef(0.03, sample_rate, 1);
        let n = (sample_rate / 20.0) as usize + 4; // lowest comb frequency 20 Hz
        self.line[0].allocate(n);
        self.line[1].allocate(n);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.line[0].clear();
        self.line[1].clear();
        self.st = [SvfState::default(); 6];
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, _tempo: f64) {
        let m = self.p_mode.index(p, 0.0);
        let is_comb = m >= 3;
        // FB stores output, FF stores input: start a comb with an empty line.
        if m != self.mode && is_comb {
            self.line[0].clear();
            self.line[1].clear();
        }
        self.mode = m;
        self.stages = self.p_slope.index(p, 0.0) + 1;
        self.log_f_t = self.p_freq.float(p, if is_comb { 0.0 } else { main }).ln();
        self.q_t = self.p_q.float(p, 0.0);
        self.comb_t = self.p_comb.float(p, if is_comb { main } else { 0.0 });
        if self.fresh {
            self.log_f = self.log_f_t;
            self.q = self.q_t;
            self.comb = self.comb_t;
            self.fresh = false;
        }
    }

    fn update_coefficients(&mut self) {
        let g = SvfCoef::g(self.log_f.exp(), self.sr);
        let lp = self.mode == 0;
        let make = |q: f32| if lp { SvfCoef::lowpass(g, q) } else { SvfCoef::highpass(g, q) };
        if self.mode == 2 {
            self.c0 = SvfCoef::bandpass(g, self.q);
            self.c1 = self.c0;
            self.c2 = self.c0;
        } else if self.stages == 1 {
            self.c0 = make(self.q);
        } else if self.stages == 2 {
            let (q1, q2) = (0.541_196, (self.q * 1.847_759).min(40.0));
            self.c0 = make(q1);
            self.c1 = make(q2);
        } else {
            let (q1, q2, q3) = (0.517_638, 0.707_107, (self.q * 2.732_051).min(40.0));
            self.c0 = make(q1);
            self.c1 = make(q2);
            self.c2 = make(q3);
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        self.log_f += (self.log_f_t - self.log_f) * self.c_chunk;
        self.q += (self.q_t - self.q) * self.c_chunk;
        if self.mode >= 3 {
            self.process_comb(l, r);
            return;
        }
        self.update_coefficients();
        let (a, b, c, ns) = (self.c0, self.c1, self.c2, self.stages);
        let st = &mut self.st;
        for i in 0..l.len() {
            let mut x = st[0].tick(l[i], &a);
            let mut y = st[3].tick(r[i], &a);
            if ns > 1 {
                x = st[1].tick(x, &b);
                y = st[4].tick(y, &b);
            }
            if ns > 2 {
                x = st[2].tick(x, &c);
                y = st[5].tick(y, &c);
            }
            l[i] = x;
            r[i] = y;
        }
        for s in st.iter_mut() {
            s.flush();
        }
    }

    fn process_comb(&mut self, l: &mut [f32], r: &mut [f32]) {
        let g = 1.0 - 0.5 / self.q.max(0.5);
        let feedback = self.mode == 3;
        let norm = if feedback { (1.0 - g * g).sqrt() } else { 1.0 / (1.0 + g * g).sqrt() };
        let (c_t, cs) = (self.comb_t, self.c_sample);
        for i in 0..l.len() {
            self.comb += (c_t - self.comb) * cs;
            let d = self.sr / self.comb;
            if feedback {
                let yl = l[i] + g * self.line[0].linear(d - 1.0);
                let yr = r[i] + g * self.line[1].linear(d - 1.0);
                self.line[0].write(flush(soft_clip(yl, 4.0)));
                self.line[1].write(flush(soft_clip(yr, 4.0)));
                l[i] = yl * norm;
                r[i] = yr * norm;
            } else {
                self.line[0].write(l[i]);
                self.line[1].write(r[i]);
                l[i] = (l[i] + g * self.line[0].linear(d)) * norm;
                r[i] = (r[i] + g * self.line[1].linear(d)) * norm;
            }
        }
    }
}

const BANDS: usize = 5;
const EQ_NAMES: [&str; 15] = [
    "LFShelvCutoff",
    "LFShelvGain",
    "LFShelvQ",
    "PeakCutoff1",
    "PeakGain1",
    "PeakBandwidth1",
    "PeakCutoff2",
    "PeakGain2",
    "PeakBandwidth2",
    "PeakCutoffLast",
    "PeakGainLast",
    "PeakBandwidthLast",
    "HFShelvCutoff",
    "HFShelvGain",
    "HFShelvQ",
];

/// Five-band parametric EQ: low shelf, three bells, high shelf (RBJ-cookbook responses realised
/// as trapezoidal SVFs). All band gains are multiplied by Scale. Parameters glide per chunk.
/// Main (modulated) parameter: Scale.
pub struct ParamEq {
    p_band: [FxParam; 15],
    p_scale: FxParam,
    /// ln(Hz), dB, Q per band
    target: [f32; 15],
    value: [f32; 15],
    coef: [SvfCoef; BANDS],
    /// 5 bands L, then 5 bands R
    st: [SvfState; 2 * BANDS],
    sr: f32,
    c_chunk: f32,
    scale_t: f32,
    scale: f32,
    /// Jup-8 V shows the slot's Dry/Wet as the EQ's "Scale": it scales the band gains.
    pub dry_wet_scale: f32,
    settled: bool,
    fresh: bool,
}

impl ParamEq {
    pub fn new(slot: usize) -> Self {
        ParamEq {
            p_band: EQ_NAMES.map(|n| FxParam::slot(slot, &format!("ParamEq_{n}"))),
            p_scale: FxParam::slot(slot, "ParamEq_Scale"),
            target: [0.0; 15],
            value: [0.0; 15],
            coef: [SvfCoef::default(); BANDS],
            st: [SvfState::default(); 2 * BANDS],
            sr: 48000.0,
            c_chunk: 0.03,
            scale_t: 1.0,
            scale: 1.0,
            dry_wet_scale: 1.0,
            settled: false,
            fresh: true,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.c_chunk = smooth_coef(0.03, sample_rate, FX_CHUNK);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.st = [SvfState::default(); 2 * BANDS];
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, _tempo: f64) {
        for b in 0..BANDS {
            self.target[3 * b] = self.p_band[3 * b].float(p, 0.0).ln();
            self.target[3 * b + 1] = self.p_band[3 * b + 1].float(p, 0.0);
            self.target[3 * b + 2] = self.p_band[3 * b + 2].float(p, 0.0);
        }
        self.scale_t = self.p_scale.float(p, main) * self.dry_wet_scale;
        if self.fresh {
            self.value = self.target;
            self.scale = self.scale_t;
            self.fresh = false;
            self.compute_coefficients();
        }
        self.settled = false;
    }

    fn compute_coefficients(&mut self) {
        for b in 0..BANDS {
            let g = SvfCoef::g(self.value[3 * b].exp(), self.sr);
            let db = self.value[3 * b + 1] * self.scale;
            let q = self.value[3 * b + 2].max(0.05);
            self.coef[b] = match b {
                0 => SvfCoef::low_shelf(g, q, db),
                b if b == BANDS - 1 => SvfCoef::high_shelf(g, q, db),
                _ => SvfCoef::bell(g, q, db),
            };
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        if !self.settled {
            let mut delta = (self.scale_t - self.scale).abs();
            self.scale += (self.scale_t - self.scale) * self.c_chunk;
            for k in 0..15 {
                let d = self.target[k] - self.value[k];
                self.value[k] += d * self.c_chunk;
                delta = delta.max(d.abs());
            }
            if delta < 1e-4 {
                self.value = self.target;
                self.scale = self.scale_t;
                self.settled = true;
            }
            self.compute_coefficients();
        }
        let c = self.coef;
        let (s, s_r) = self.st.split_at_mut(BANDS);
        for i in 0..l.len() {
            let (mut x, mut y) = (l[i], r[i]);
            for b in 0..BANDS {
                x = s[b].tick(x, &c[b]);
                y = s_r[b].tick(y, &c[b]);
            }
            l[i] = x;
            r[i] = y;
        }
        for k in self.st.iter_mut() {
            k.flush();
        }
    }
}
