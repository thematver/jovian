//! Chorus, flanger, phaser and auto-panner.

use super::dsp::*;

/// 1-3 voice chorus: each voice reads the delay line at Delay + Depth·(1 + lfo)/2 with the LFO
/// phases spread evenly; in stereo mode the right channel's LFOs run a quarter cycle later.
/// Feedback returns the voice average into the line.
/// Main (modulated) parameter: Feedback.
pub struct Chorus {
    p_delay: FxParam,
    p_depth: FxParam,
    p_feedback: FxParam,
    p_freq: FxParam,
    p_shape: FxParam,
    p_stereo: FxParam,
    p_voices: FxParam,
    line_l: DelayLine,
    line_r: DelayLine,
    sr: f32,
    c_sample: f32,
    base_t: f32,
    depth_t: f32,
    fb_t: f32,
    base: f32,
    depth: f32,
    fb: f32,
    inc: f64,
    phase: f64,
    voices: usize,
    triangle: bool,
    stereo: bool,
    fresh: bool,
}

impl Chorus {
    pub fn new(slot: usize) -> Self {
        Chorus {
            p_delay: FxParam::slot(slot, "Chorus_Delay"),
            p_depth: FxParam::slot(slot, "Chorus_Depth"),
            p_feedback: FxParam::slot(slot, "Chorus_Feedback"),
            p_freq: FxParam::slot(slot, "Chorus_Freq"),
            p_shape: FxParam::slot(slot, "Chorus_LFOShape"),
            p_stereo: FxParam::slot(slot, "Chorus_StereoMode"),
            p_voices: FxParam::slot(slot, "Chorus_Voices"),
            line_l: DelayLine::default(),
            line_r: DelayLine::default(),
            sr: 48000.0,
            c_sample: 0.001,
            base_t: 100.0,
            depth_t: 0.0,
            fb_t: 0.0,
            base: 100.0,
            depth: 0.0,
            fb: 0.0,
            inc: 0.0,
            phase: 0.0,
            voices: 2,
            triangle: false,
            stereo: true,
            fresh: true,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.c_sample = smooth_coef(0.03, sample_rate, 1);
        let n = (0.032 * sample_rate) as usize;
        self.line_l.allocate(n);
        self.line_r.allocate(n);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.line_l.clear();
        self.line_r.clear();
        self.phase = 0.0;
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, _tempo: f64) {
        let ms = self.sr / 1000.0;
        self.base_t = self.p_delay.float(p, 0.0) * ms;
        self.depth_t = self.p_depth.float(p, 0.0) * ms;
        self.fb_t = self.p_feedback.float(p, main);
        self.inc = self.p_freq.value(p, 0.0) / self.sr as f64;
        self.triangle = self.p_shape.index(p, 0.0) == 1;
        self.stereo = self.p_stereo.index(p, 0.0) == 1;
        self.voices = self.p_voices.index(p, 0.0) + 1;
        if self.fresh {
            self.base = self.base_t;
            self.depth = self.depth_t;
            self.fb = self.fb_t;
            self.fresh = false;
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let (c, b_t, d_t, f_t, tri) = (self.c_sample, self.base_t, self.depth_t, self.fb_t, self.triangle);
        let nv = self.voices;
        let inv = 1.0 / nv as f32;
        let off: f32 = if self.stereo { 0.25 } else { 0.0 };
        let mut ph = self.phase;
        for i in 0..l.len() {
            self.base += (b_t - self.base) * c;
            self.depth += (d_t - self.depth) * c;
            self.fb += (f_t - self.fb) * c;
            ph += self.inc;
            if ph >= 1.0 {
                ph -= 1.0;
            }
            let half = 0.5 * self.depth;
            let b1 = self.base - 1.0; // read-before-write: delay D is linear(D - 1)
            let mut sum_l = 0.0f32;
            let mut sum_r = 0.0f32;
            let mut pv = ph as f32;
            for _ in 0..nv {
                let p_r = wrap(pv + off);
                let ml = if tri { triangle(pv) } else { sine(pv) };
                let mr = if tri { triangle(p_r) } else { sine(p_r) };
                sum_l += self.line_l.linear(b1 + half * (1.0 + ml));
                sum_r += self.line_r.linear(b1 + half * (1.0 + mr));
                pv = wrap(pv + inv);
            }
            sum_l *= inv;
            sum_r *= inv;
            self.line_l.write(flush(l[i] + self.fb * sum_l));
            self.line_r.write(flush(r[i] + self.fb * sum_r));
            l[i] = sum_l;
            r[i] = sum_r;
        }
        self.phase = ph;
    }
}

/// Flanger: short modulated delay (MinDelay + Depth·(1 + lfo)/2) with signed feedback through
/// HP and LP filters; in stereo mode the right LFO runs a quarter cycle later.
/// Main (modulated) parameter: LFO rate.
pub struct Flanger {
    p_min: FxParam,
    p_depth: FxParam,
    p_feedback: FxParam,
    p_polarity: FxParam,
    p_stereo: FxParam,
    p_lp: FxParam,
    p_hp: FxParam,
    p_shape: FxParam,
    p_freq: FxParam,
    line_l: DelayLine,
    line_r: DelayLine,
    sr: f32,
    c_sample: f32,
    c_chunk: f32,
    min_t: f32,
    depth_t: f32,
    fb_t: f32,
    lp_t: f32,
    hp_t: f32,
    min_d: f32,
    depth: f32,
    fb: f32,
    lp_hz: f32,
    hp_hz: f32,
    inc: f64,
    phase: f64,
    triangle: bool,
    stereo: bool,
    fresh: bool,
    yl: f32,
    yr: f32,
    hp_l: OnePole,
    hp_r: OnePole,
    lp_l: OnePole,
    lp_r: OnePole,
}

impl Flanger {
    pub fn new(slot: usize) -> Self {
        Flanger {
            p_min: FxParam::slot(slot, "Flanger_MinDelay"),
            p_depth: FxParam::slot(slot, "Flanger_DepthFlanger"),
            p_feedback: FxParam::slot(slot, "Flanger_Chorus_Feedback"),
            p_polarity: FxParam::slot(slot, "Flanger_Chorus_FBPolarity"),
            p_stereo: FxParam::slot(slot, "Flanger_Chorus_StereoMode"),
            p_lp: FxParam::slot(slot, "Flanger_Chorus_LPFfc"),
            p_hp: FxParam::slot(slot, "Flanger_Chorus_HPFfc"),
            p_shape: FxParam::slot(slot, "Flanger_Chorus_LFOShape"),
            p_freq: FxParam::slot(slot, "Flanger_Chorus_Freq"),
            line_l: DelayLine::default(),
            line_r: DelayLine::default(),
            sr: 48000.0,
            c_sample: 0.001,
            c_chunk: 0.03,
            min_t: 0.0,
            depth_t: 0.0,
            fb_t: 0.0,
            lp_t: 20000.0,
            hp_t: 30.0,
            min_d: 0.0,
            depth: 0.0,
            fb: 0.0,
            lp_hz: 20000.0,
            hp_hz: 30.0,
            inc: 0.0,
            phase: 0.0,
            triangle: true,
            stereo: true,
            fresh: true,
            yl: 0.0,
            yr: 0.0,
            hp_l: OnePole::default(),
            hp_r: OnePole::default(),
            lp_l: OnePole::default(),
            lp_r: OnePole::default(),
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.c_sample = smooth_coef(0.03, sample_rate, 1);
        self.c_chunk = smooth_coef(0.03, sample_rate, FX_CHUNK);
        let n = (0.022 * sample_rate) as usize;
        self.line_l.allocate(n);
        self.line_r.allocate(n);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.line_l.clear();
        self.line_r.clear();
        self.hp_l = OnePole::default();
        self.hp_r = OnePole::default();
        self.lp_l = OnePole::default();
        self.lp_r = OnePole::default();
        self.yl = 0.0;
        self.yr = 0.0;
        self.phase = 0.0;
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, _tempo: f64) {
        let ms = self.sr / 1000.0;
        self.min_t = self.p_min.float(p, 0.0) * ms;
        self.depth_t = self.p_depth.float(p, 0.0) * ms;
        self.fb_t = self.p_feedback.float(p, 0.0) * if self.p_polarity.index(p, 0.0) == 1 { -1.0 } else { 1.0 };
        self.lp_t = self.p_lp.float(p, 0.0);
        self.hp_t = self.p_hp.float(p, 0.0);
        self.inc = self.p_freq.value(p, main) / self.sr as f64;
        self.triangle = self.p_shape.index(p, 0.0) == 1;
        self.stereo = self.p_stereo.index(p, 0.0) == 1;
        if self.fresh {
            self.min_d = self.min_t;
            self.depth = self.depth_t;
            self.fb = self.fb_t;
            self.lp_hz = self.lp_t;
            self.hp_hz = self.hp_t;
            self.fresh = false;
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        self.lp_hz += (self.lp_t - self.lp_hz) * self.c_chunk;
        self.hp_hz += (self.hp_t - self.hp_hz) * self.c_chunk;
        let g_lp = one_pole_g(self.lp_hz, self.sr);
        let g_hp = one_pole_g(self.hp_hz, self.sr);
        let (c, m_t, d_t, f_t, tri) = (self.c_sample, self.min_t, self.depth_t, self.fb_t, self.triangle);
        let off: f32 = if self.stereo { 0.25 } else { 0.0 };
        let (mut ph, mut a, mut b) = (self.phase, self.yl, self.yr);
        for i in 0..l.len() {
            self.min_d += (m_t - self.min_d) * c;
            self.depth += (d_t - self.depth) * c;
            self.fb += (f_t - self.fb) * c;
            ph += self.inc;
            if ph >= 1.0 {
                ph -= 1.0;
            }
            let pl = ph as f32;
            let pr = wrap(pl + off);
            let ml = if tri { triangle(pl) } else { sine(pl) };
            let mr = if tri { triangle(pr) } else { sine(pr) };
            let half = 0.5 * self.depth;
            let fl = self.lp_l.lp(self.hp_l.hp(a, g_hp), g_lp);
            let fr = self.lp_r.lp(self.hp_r.hp(b, g_hp), g_lp);
            self.line_l.write(flush(soft_clip(l[i] + self.fb * fl, 2.0)));
            self.line_r.write(flush(soft_clip(r[i] + self.fb * fr, 2.0)));
            a = self.line_l.linear(self.min_d + half * (1.0 + ml));
            b = self.line_r.linear(self.min_d + half * (1.0 + mr));
            l[i] = a;
            r[i] = b;
        }
        self.phase = ph;
        self.yl = a;
        self.yr = b;
        self.hp_l.flush();
        self.hp_r.flush();
        self.lp_l.flush();
        self.lp_r.flush();
    }
}

const MAX_POLES: usize = 12;
const SWEEP_OCTAVES: f32 = 3.0;

/// Phaser: cascade of 2...12 first-order (TPT) allpasses whose break frequency sweeps
/// ±3 octaves × LFO Amount around Frequency, with feedback from the last stage. The right
/// channel's LFO is offset by Stereo degrees. Coefficients are interpolated across each chunk.
/// Main (modulated) parameter: LFO rate.
pub struct Phaser {
    p_feedback: FxParam,
    p_freq: FxParam,
    p_amount: FxParam,
    p_wave: FxParam,
    p_sync: FxParam,
    p_synced: FxParam,
    p_sync_type: FxParam,
    p_rate: FxParam,
    p_poles: FxParam,
    p_stereo: FxParam,
    bars: Vec<f64>,
    st: [f32; 2 * MAX_POLES],
    sr: f32,
    c_chunk: f32,
    fb_t: f32,
    freq_t: f32,
    amount_t: f32,
    fb: f32,
    freq: f32,
    amount: f32,
    inc: f64,
    phase: f64,
    offset: f64,
    last_r: f64,
    wave: usize,
    poles: usize,
    g_l: f32,
    g_r: f32,
    yl: f32,
    yr: f32,
    sh_l: f32,
    sh_r: f32,
    rng: Random,
    fresh: bool,
}

impl Phaser {
    pub fn new(slot: usize) -> Self {
        Phaser {
            p_feedback: FxParam::slot(slot, "Phaser_Feedback"),
            p_freq: FxParam::slot(slot, "Phaser_Freq"),
            p_amount: FxParam::slot(slot, "Phaser_LFOAmount"),
            p_wave: FxParam::slot(slot, "Phaser_LFOWaveform"),
            p_sync: FxParam::slot(slot, "Phaser_LFO_MidiSync"),
            p_synced: FxParam::slot(slot, "Phaser_LFO_RateSynced"),
            p_sync_type: FxParam::slot(slot, "Phaser_LFO_RateSyncedType"),
            p_rate: FxParam::slot(slot, "Phaser_LFO_RateUnSynced"),
            p_poles: FxParam::slot(slot, "Phaser_NPoles"),
            p_stereo: FxParam::slot(slot, "Phaser_Stereo"),
            bars: bars(&format!("FX{}_Phaser_LFO_RateSynced", slot + 1)),
            st: [0.0; 2 * MAX_POLES],
            sr: 48000.0,
            c_chunk: 0.03,
            fb_t: 0.0,
            freq_t: 740.0,
            amount_t: 1.0,
            fb: 0.0,
            freq: 740.0,
            amount: 1.0,
            inc: 0.0,
            phase: 0.0,
            offset: 0.0,
            last_r: 0.0,
            wave: 0,
            poles: 6,
            g_l: 0.0,
            g_r: 0.0,
            yl: 0.0,
            yr: 0.0,
            sh_l: 0.0,
            sh_r: 0.0,
            rng: Random::default(),
            fresh: true,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.c_chunk = smooth_coef(0.03, sample_rate, FX_CHUNK);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.st = [0.0; 2 * MAX_POLES];
        self.yl = 0.0;
        self.yr = 0.0;
        self.phase = 0.0;
        self.last_r = 0.0;
        self.fresh = true;
    }

    fn wrap_d(p: f64) -> f64 {
        if p >= 1.0 {
            p - 1.0
        } else {
            p
        }
    }

    pub fn update(&mut self, p: &[f32], main: f32, tempo: f64) {
        let hz = if self.p_sync.index(p, 0.0) == 1 {
            let i = self.p_synced.index(p, main);
            1.0 / sync_seconds(self.bars[i.min(self.bars.len() - 1)], self.p_sync_type.index(p, 0.0), tempo)
        } else {
            self.p_rate.value(p, main)
        };
        self.inc = hz / self.sr as f64;
        self.fb_t = self.p_feedback.float(p, 0.0);
        self.freq_t = self.p_freq.float(p, 0.0);
        self.amount_t = self.p_amount.float(p, 0.0);
        self.wave = self.p_wave.index(p, 0.0);
        self.poles = 2 * (self.p_poles.index(p, 0.0) + 1);
        self.offset = self.p_stereo.value(p, 0.0) / 360.0;
        if self.fresh {
            self.fb = self.fb_t;
            self.freq = self.freq_t;
            self.amount = self.amount_t;
            self.g_l = self.coef(self.lfo(self.phase, self.sh_l));
            self.g_r = self.coef(self.lfo(Self::wrap_d(self.phase + self.offset), self.sh_r));
            self.fresh = false;
        }
    }

    #[inline(always)]
    fn lfo(&self, p: f64, sh: f32) -> f32 {
        let x = p as f32;
        match self.wave {
            0 => sine(x),
            1 => triangle(x),
            2 => 1.0 - 2.0 * x,
            3 => 2.0 * x - 1.0,
            4 => {
                if x < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            _ => sh,
        }
    }

    #[inline(always)]
    fn coef(&self, m: f32) -> f32 {
        let hz = self.freq * (m * self.amount * SWEEP_OCTAVES).exp2();
        one_pole_g(hz, self.sr)
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        self.fb += (self.fb_t - self.fb) * self.c_chunk;
        self.freq += (self.freq_t - self.freq) * self.c_chunk;
        self.amount += (self.amount_t - self.amount) * self.c_chunk;
        // Advance the LFO to the end of the chunk; sample & hold draws on each cycle start.
        self.phase += self.inc * n as f64;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.sh_l = self.rng.bipolar();
        }
        let p_r = Self::wrap_d(self.phase + self.offset);
        if p_r < self.last_r {
            self.sh_r = self.rng.bipolar();
        }
        self.last_r = p_r;
        let g_l1 = self.coef(self.lfo(self.phase, self.sh_l));
        let g_r1 = self.coef(self.lfo(p_r, self.sh_r));
        let dl = (g_l1 - self.g_l) / n as f32;
        let dr = (g_r1 - self.g_r) / n as f32;
        let (mut a, mut b, mut ya, mut yb) = (self.g_l, self.g_r, self.yl, self.yr);
        let (np, f) = (self.poles, self.fb);
        let (s, s_r) = self.st.split_at_mut(MAX_POLES);
        for i in 0..n {
            a += dl;
            b += dr;
            let mut u = soft_clip(l[i] + f * ya, 4.0);
            let mut v = soft_clip(r[i] + f * yb, 4.0);
            for k in 0..np {
                let x1 = (u - s[k]) * a;
                let lp1 = x1 + s[k];
                s[k] = lp1 + x1;
                u = 2.0 * lp1 - u;
                let x2 = (v - s_r[k]) * b;
                let lp2 = x2 + s_r[k];
                s_r[k] = lp2 + x2;
                v = 2.0 * lp2 - v;
            }
            ya = u;
            yb = v;
            l[i] = u;
            r[i] = v;
        }
        self.g_l = g_l1;
        self.g_r = g_r1;
        self.yl = flush(ya);
        self.yr = flush(yb);
        for k in self.st.iter_mut() {
            *k = flush(*k);
        }
    }
}

/// Auto-panner: equal-power balance driven by a sine LFO, Amount = depth (0 centre ... 1 hard).
/// Rate in Hz or synced (binary, triplet, dotted).
/// Main (modulated) parameter: Amount.
pub struct StereoPan {
    p_amount: FxParam,
    p_synced: FxParam,
    p_type: FxParam,
    p_rate: FxParam,
    bars: Vec<f64>,
    sr: f32,
    c_sample: f32,
    amount_t: f32,
    amount: f32,
    inc: f64,
    phase: f64,
    fresh: bool,
}

impl StereoPan {
    pub fn new(slot: usize) -> Self {
        StereoPan {
            p_amount: FxParam::slot(slot, "StereoPan_Amount"),
            p_synced: FxParam::slot(slot, "StereoPan_LFOSimple_RateSynced"),
            p_type: FxParam::slot(slot, "StereoPan_LFOSimple_RateType"),
            p_rate: FxParam::slot(slot, "StereoPan_LFOSimple_RateUnSynced"),
            bars: bars(&format!("FX{}_StereoPan_LFOSimple_RateSynced", slot + 1)),
            sr: 48000.0,
            c_sample: 0.001,
            amount_t: 0.0,
            amount: 0.0,
            inc: 0.0,
            phase: 0.0,
            fresh: true,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.c_sample = smooth_coef(0.02, sample_rate, 1);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, tempo: f64) {
        let kind = self.p_type.index(p, 0.0);
        let hz = if kind == 0 {
            self.p_rate.value(p, 0.0)
        } else {
            let i = self.p_synced.index(p, 0.0);
            1.0 / sync_seconds(self.bars[i.min(self.bars.len() - 1)], kind - 1, tempo)
        };
        self.inc = hz / self.sr as f64;
        self.amount_t = self.p_amount.float(p, main);
        if self.fresh {
            self.amount = self.amount_t;
            self.fresh = false;
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let (c, a_t) = (self.c_sample, self.amount_t);
        let mut ph = self.phase;
        for i in 0..l.len() {
            self.amount += (a_t - self.amount) * c;
            ph += self.inc;
            if ph >= 1.0 {
                ph -= 1.0;
            }
            let q = (self.amount * sine(ph as f32) + 1.0) * 0.125; // pan angle in cycles, 0...0.25
            l[i] *= 1.414_213_6 * sine(q + 0.25);
            r[i] *= 1.414_213_6 * sine(q);
        }
        self.phase = ph;
    }
}
