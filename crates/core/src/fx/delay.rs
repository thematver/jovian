//! Stereo / ping-pong delay with HP and LP filters inside the feedback loop. Time is free
//! (2...2000 ms) or synced to the host tempo (1/32...8 bars, binary/triplet/dotted); the read
//! position glides towards a new time (tape-like). StereoWidth lengthens the right channel by
//! that ratio (stereo mode); StereoWidthPP sets how far the ping-pong echoes are panned.
//! The buffer holds 8 s; longer synced values are clamped. A reset is O(1): reads beyond the
//! samples written since the reset return silence.
//! Main (modulated) parameter: delay time (DelayTimeSynced when synced, DelayTimeMs otherwise).

use super::dsp::*;

const MAX_SECONDS: f64 = 8.0;

pub struct Delay {
    p_ms: FxParam,
    p_synced: FxParam,
    p_feedback: FxParam,
    p_hp: FxParam,
    p_lp: FxParam,
    p_sync: FxParam,
    p_sync_type: FxParam,
    p_mode: FxParam,
    p_width: FxParam,
    p_width_pp: FxParam,
    bars: Vec<f64>,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    cap: usize,
    w: usize,
    filled: usize,
    sr: f32,
    c_time: f32,
    c_gain: f32,
    c_chunk: f32,
    t_lt: f32,
    t_rt: f32,
    fb_t: f32,
    hp_t: f32,
    lp_t: f32,
    pp_t: f32,
    t_l: f32,
    t_r: f32,
    fb: f32,
    hp_hz: f32,
    lp_hz: f32,
    pp: f32,
    ping_pong: bool,
    fresh: bool,
    hp_l: OnePole,
    hp_r: OnePole,
    lp_l: OnePole,
    lp_r: OnePole,
}

impl Delay {
    pub fn new(slot: usize) -> Self {
        Delay {
            p_ms: FxParam::slot(slot, "Delay_DelayTimeMs"),
            p_synced: FxParam::slot(slot, "Delay_DelayTimeSynced"),
            p_feedback: FxParam::slot(slot, "Delay_Feedback"),
            p_hp: FxParam::slot(slot, "Delay_HPFreq"),
            p_lp: FxParam::slot(slot, "Delay_LPFreq"),
            p_sync: FxParam::slot(slot, "Delay_MidiSync"),
            p_sync_type: FxParam::slot(slot, "Delay_RateSyncedType"),
            p_mode: FxParam::slot(slot, "Delay_StereoMode"),
            p_width: FxParam::slot(slot, "Delay_StereoWidth"),
            p_width_pp: FxParam::slot(slot, "Delay_StereoWidthPP"),
            bars: bars(&format!("FX{}_Delay_DelayTimeSynced", slot + 1)),
            buf_l: vec![0.0; 8],
            buf_r: vec![0.0; 8],
            cap: 8,
            w: 0,
            filled: 0,
            sr: 48000.0,
            c_time: 0.001,
            c_gain: 0.001,
            c_chunk: 0.03,
            t_lt: 1000.0,
            t_rt: 1000.0,
            fb_t: 0.0,
            hp_t: 20.0,
            lp_t: 20000.0,
            pp_t: 1.0,
            t_l: 1000.0,
            t_r: 1000.0,
            fb: 0.0,
            hp_hz: 20.0,
            lp_hz: 20000.0,
            pp: 1.0,
            ping_pong: false,
            fresh: true,
            hp_l: OnePole::default(),
            hp_r: OnePole::default(),
            lp_l: OnePole::default(),
            lp_r: OnePole::default(),
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.cap = (MAX_SECONDS * sample_rate) as usize + 8;
        self.buf_l = vec![0.0; self.cap];
        self.buf_r = vec![0.0; self.cap];
        self.c_time = smooth_coef(0.06, sample_rate, 1);
        self.c_gain = smooth_coef(0.02, sample_rate, 1);
        self.c_chunk = smooth_coef(0.03, sample_rate, FX_CHUNK);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.w = 0;
        self.filled = 0;
        self.hp_l = OnePole::default();
        self.hp_r = OnePole::default();
        self.lp_l = OnePole::default();
        self.lp_r = OnePole::default();
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, tempo: f64) {
        let synced = self.p_sync.index(p, 0.0) == 1;
        let seconds = if synced {
            let i = self.p_synced.index(p, main);
            sync_seconds(self.bars[i.min(self.bars.len() - 1)], self.p_sync_type.index(p, 0.0), tempo)
        } else {
            self.p_ms.value(p, main) / 1000.0
        };
        let max_d = (self.cap - 8) as f32;
        let t = (seconds as f32 * self.sr).max(2.0).min(max_d);
        self.ping_pong = self.p_mode.index(p, 0.0) == 1;
        self.t_lt = t;
        self.t_rt = if self.ping_pong { t } else { (t * (1.0 + self.p_width.float(p, 0.0))).min(max_d) };
        self.fb_t = self.p_feedback.float(p, 0.0);
        self.hp_t = self.p_hp.float(p, 0.0);
        self.lp_t = self.p_lp.float(p, 0.0);
        self.pp_t = self.p_width_pp.float(p, 0.0);
        if self.fresh {
            self.t_l = self.t_lt;
            self.t_r = self.t_rt;
            self.fb = self.fb_t;
            self.hp_hz = self.hp_t;
            self.lp_hz = self.lp_t;
            self.pp = self.pp_t;
            self.fresh = false;
        }
    }

    /// 4-point Hermite read `d` samples back (d >= 2); taps older than the history written since
    /// the last reset read as silence.
    #[inline(always)]
    fn read(b: &[f32], w: usize, cap: usize, filled: usize, d: f32) -> f32 {
        let i = d as isize;
        if i - 1 > filled as isize {
            return 0.0;
        }
        let f = d - i as f32;
        let cap_i = cap as isize;
        let mut j = w as isize - i + 1;
        if j < 0 {
            j += cap_i;
        }
        let y0 = b[j as usize];
        j -= 1;
        if j < 0 {
            j += cap_i;
        }
        let mut y1 = b[j as usize];
        j -= 1;
        if j < 0 {
            j += cap_i;
        }
        let mut y2 = b[j as usize];
        j -= 1;
        if j < 0 {
            j += cap_i;
        }
        let mut y3 = b[j as usize];
        let filled = filled as isize;
        if i + 2 > filled {
            y3 = 0.0;
            if i + 1 > filled {
                y2 = 0.0;
            }
            if i > filled {
                y1 = 0.0;
            }
        }
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
        ((c3 * f + c2) * f + c1) * f + y1
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        self.hp_hz += (self.hp_t - self.hp_hz) * self.c_chunk;
        self.lp_hz += (self.lp_t - self.lp_hz) * self.c_chunk;
        self.pp += (self.pp_t - self.pp) * self.c_chunk;
        let g_hp = one_pole_g(self.hp_hz, self.sr);
        let g_lp = one_pole_g(self.lp_hz, self.sr);
        let a = 0.5 * (1.0 + self.pp);
        let b = 0.5 * (1.0 - self.pp);
        let (ct, cg, tlt, trt, fb_target, ping) = (self.c_time, self.c_gain, self.t_lt, self.t_rt, self.fb_t, self.ping_pong);
        let cap = self.cap;

        for i in 0..l.len() {
            self.t_l += (tlt - self.t_l) * ct;
            self.t_r += (trt - self.t_r) * ct;
            self.fb += (fb_target - self.fb) * cg;
            let o_l = Self::read(&self.buf_l, self.w, cap, self.filled, self.t_l);
            let o_r = Self::read(&self.buf_r, self.w, cap, self.filled, self.t_r);
            let (mut v_l, mut v_r);
            if ping {
                v_l = 0.5 * (l[i] + r[i]) + self.fb * o_r;
                v_r = self.fb * o_l;
                l[i] = a * o_l + b * o_r;
                r[i] = b * o_l + a * o_r;
            } else {
                v_l = l[i] + self.fb * o_l;
                v_r = r[i] + self.fb * o_r;
                l[i] = o_l;
                r[i] = o_r;
            }
            v_l = self.lp_l.lp(self.hp_l.hp(v_l, g_hp), g_lp);
            v_r = self.lp_r.lp(self.hp_r.hp(v_r, g_hp), g_lp);
            self.buf_l[self.w] = flush(soft_clip(v_l, 2.0));
            self.buf_r[self.w] = flush(soft_clip(v_r, 2.0));
            self.w += 1;
            if self.w == cap {
                self.w = 0;
            }
            if self.filled < cap {
                self.filled += 1;
            }
        }
        self.hp_l.flush();
        self.hp_r.flush();
        self.lp_l.flush();
        self.lp_r.flush();
    }
}
