//! Dattorro plate (J. Dattorro, "Effect Design Part 1", JAES 1997): predelay, input band-limit
//! (LP + HP), four input diffusers and the figure-eight tank with modulated allpasses, damping
//! and decay; the stereo outputs are tapped as in the paper. Lengths are the paper's values at
//! 29761 Hz, rescaled to the sample rate; Size scales the tank. Decay is the tank gain, Damping
//! the paper's damping coefficient, MSMix the side gain (0 mono ... 0.5 full width).
//! Main (modulated) parameter: Decay.

use super::dsp::*;

const BASE_RATE: f32 = 29761.0;
const INPUT_LENGTHS: [f32; 4] = [142.0, 107.0, 379.0, 277.0];
const TANK_LENGTHS: [f32; 8] = [672.0, 4453.0, 1800.0, 3720.0, 908.0, 4217.0, 2656.0, 3163.0];
const EXCURSION: f32 = 8.0;
const MAX_SIZE: f32 = 1.5;
const MAX_PREDELAY: f32 = 0.2;

pub struct Reverb {
    p_decay: FxParam,
    p_damping: FxParam,
    p_size: FxParam,
    p_hp: FxParam,
    p_lp: FxParam,
    p_ms: FxParam,
    p_pre: FxParam,
    /// 0 predelay, 1...4 input diffusers, 5...12 tank.
    d: [DelayLine; 13],
    sr: f32,
    k29: f32,
    c_sample: f32,
    c_chunk: f32,
    in_len: [usize; 4],
    lfo_inc: f64,
    phase: f64,
    decay_t: f32,
    damp_t: f32,
    size_t: f32,
    hp_t: f32,
    lp_t: f32,
    width_t: f32,
    pre_t: f32,
    decay: f32,
    damp: f32,
    size: f32,
    hp_hz: f32,
    lp_hz: f32,
    width: f32,
    pre: f32,
    fresh: bool,
    lp_in: OnePole,
    hp_in: OnePole,
    damp_l: f32,
    damp_r: f32,
    fb_r: f32,
}

impl Reverb {
    pub fn new(slot: usize) -> Self {
        Reverb {
            p_decay: FxParam::slot(slot, "Dattorro_Decay"),
            p_damping: FxParam::slot(slot, "Dattorro_Damping"),
            p_size: FxParam::slot(slot, "Dattorro_E_Size"),
            p_hp: FxParam::slot(slot, "Dattorro_HPBandWith"),
            p_lp: FxParam::slot(slot, "Dattorro_LPBandWith"),
            p_ms: FxParam::slot(slot, "Dattorro_MSMix"),
            p_pre: FxParam::slot(slot, "Dattorro_Predelay"),
            d: Default::default(),
            sr: 48000.0,
            k29: 1.0,
            c_sample: 0.001,
            c_chunk: 0.03,
            in_len: [0; 4],
            lfo_inc: 0.0,
            phase: 0.0,
            decay_t: 0.5,
            damp_t: 0.0,
            size_t: 1.0,
            hp_t: 30.0,
            lp_t: 20000.0,
            width_t: 1.0,
            pre_t: 0.0,
            decay: 0.5,
            damp: 0.0,
            size: 1.0,
            hp_hz: 30.0,
            lp_hz: 20000.0,
            width: 1.0,
            pre: 0.0,
            fresh: true,
            lp_in: OnePole::default(),
            hp_in: OnePole::default(),
            damp_l: 0.0,
            damp_r: 0.0,
            fb_r: 0.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sr = sample_rate as f32;
        self.k29 = self.sr / BASE_RATE;
        self.c_sample = smooth_coef(0.05, sample_rate, 1);
        self.c_chunk = smooth_coef(0.08, sample_rate, FX_CHUNK);
        self.lfo_inc = 1.0 / sample_rate;
        self.d[0].allocate((MAX_PREDELAY * self.sr) as usize + 4);
        for i in 0..4 {
            self.d[1 + i].allocate((INPUT_LENGTHS[i] * self.k29) as usize + 4);
        }
        let k = self.k29;
        self.in_len = [(142.0 * k) as usize, (107.0 * k) as usize, (379.0 * k) as usize, (277.0 * k) as usize];
        let exc = (EXCURSION * k) as usize + 4;
        for i in 0..8 {
            self.d[5 + i].allocate((TANK_LENGTHS[i] * k * MAX_SIZE) as usize + exc);
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        for l in self.d.iter_mut() {
            l.clear();
        }
        self.lp_in = OnePole::default();
        self.hp_in = OnePole::default();
        self.damp_l = 0.0;
        self.damp_r = 0.0;
        self.fb_r = 0.0;
        self.phase = 0.0;
        self.fresh = true;
    }

    pub fn update(&mut self, p: &[f32], main: f32, _tempo: f64) {
        self.decay_t = self.p_decay.float(p, main);
        self.damp_t = self.p_damping.float(p, 0.0).min(0.99);
        self.size_t = self.p_size.float(p, 0.0);
        self.hp_t = self.p_hp.float(p, 0.0);
        self.lp_t = self.p_lp.float(p, 0.0);
        self.width_t = (2.0 * self.p_ms.float(p, 0.0)).min(1.0);
        self.pre_t = self.p_pre.float(p, 0.0) * self.sr;
        if self.fresh {
            self.decay = self.decay_t;
            self.damp = self.damp_t;
            self.size = self.size_t;
            self.hp_hz = self.hp_t;
            self.lp_hz = self.lp_t;
            self.width = self.width_t;
            self.pre = self.pre_t;
            self.fresh = false;
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let c = self.c_chunk;
        self.decay += (self.decay_t - self.decay) * c;
        self.damp += (self.damp_t - self.damp) * c;
        self.size += (self.size_t - self.size) * c;
        self.hp_hz += (self.hp_t - self.hp_hz) * c;
        self.lp_hz += (self.lp_t - self.lp_hz) * c;
        self.width += (self.width_t - self.width) * c;

        let g_lp = one_pole_g(self.lp_hz, self.sr);
        let g_hp = one_pole_g(self.hp_hz, self.sr);
        let dec = self.decay;
        let dd2 = (self.decay + 0.15).clamp(0.25, 0.5);
        // Damping coefficient is defined at 29761 Hz: same cutoff at any rate.
        let pd = if self.damp > 0.0 { self.damp.powf(BASE_RATE / self.sr) } else { 0.0 };
        let k = self.k29 * self.size;
        let (t5, t6, t7, t8) = (672.0 * k, 4453.0 * k, 1800.0 * k, 3720.0 * k);
        let (t9, t10, t11, t12) = (908.0 * k, 4217.0 * k, 2656.0 * k, 3163.0 * k);
        let exc = EXCURSION * self.k29;
        let o = |x: f32| (x * k) as usize;
        let (ol0, ol1, ol2, ol3, ol4, ol5, ol6) = (o(266.0), o(2974.0), o(1913.0), o(1996.0), o(1990.0), o(187.0), o(1066.0));
        let (or0, or1, or2, or3, or4, or5, or6) = (o(353.0), o(3627.0), o(1228.0), o(2673.0), o(2111.0), o(335.0), o(121.0));
        let [i1, i2, i3, i4] = self.in_len;
        let side = 0.5 * self.width;
        let (cs, pre_target, inc) = (self.c_sample, self.pre_t, self.lfo_inc);

        let mut lp = self.lp_in;
        let mut hp = self.hp_in;
        let (mut dl, mut dr, mut fr, mut ph, mut pr) = (self.damp_l, self.damp_r, self.fb_r, self.phase, self.pre);
        let d = &mut self.d;

        for i in 0..l.len() {
            d[0].write(0.5 * (l[i] + r[i]));
            pr += (pre_target - pr) * cs;
            let mut s = d[0].linear(pr);
            s = hp.hp(lp.lp(s, g_lp), g_hp);
            s = allpass(&mut d[1], s, i1, 0.75);
            s = allpass(&mut d[2], s, i2, 0.75);
            s = allpass(&mut d[3], s, i3, 0.625);
            s = allpass(&mut d[4], s, i4, 0.625);

            ph += inc;
            if ph >= 1.0 {
                ph -= 1.0;
            }
            let ml = sine(ph as f32) * exc;
            let mr = sine(wrap(ph as f32 + 0.25)) * exc;

            // Left half of the tank.
            let mut a = allpass_frac(&mut d[5], s + fr, t5 + ml, -0.7);
            d[6].write(flush(a));
            a = d[6].linear(t6);
            dl = a + pd * (dl - a);
            a = allpass_frac(&mut d[7], dl * dec, t7, dd2);
            d[8].write(flush(a));
            let fl = d[8].linear(t8) * dec;

            // Right half.
            let mut b = allpass_frac(&mut d[9], s + fl, t9 + mr, -0.7);
            d[10].write(flush(b));
            b = d[10].linear(t10);
            dr = b + pd * (dr - b);
            b = allpass_frac(&mut d[11], dr * dec, t11, dd2);
            d[12].write(flush(b));
            fr = d[12].linear(t12) * dec;

            let yl = d[10].at(ol0) + d[10].at(ol1) - d[11].at(ol2) + d[12].at(ol3)
                - d[6].at(ol4)
                - d[7].at(ol5)
                - d[8].at(ol6);
            let yr = d[6].at(or0) + d[6].at(or1) - d[7].at(or2) + d[8].at(or3)
                - d[10].at(or4)
                - d[11].at(or5)
                - d[12].at(or6);
            let mid = 0.3 * (yl + yr);
            let sd = 0.3 * (yl - yr) * side * 2.0;
            l[i] = mid + sd;
            r[i] = mid - sd;
        }

        lp.flush();
        hp.flush();
        self.lp_in = lp;
        self.hp_in = hp;
        self.damp_l = flush(dl);
        self.damp_r = flush(dr);
        self.fb_r = flush(fr);
        self.phase = ph;
        self.pre = pr;
    }
}
