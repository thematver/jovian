//! The Jup-8 V effects section: three slots in series or parallel, each running one of eleven
//! effects. Parameters are read as normalized preset values.
//!
//! - FX_Enable 0 passes the input through (10 ms crossfade; tails are cleared on re-enable).
//! - FX_Routing 0 "Serie": FX1 -> FX2 -> FX3. 1 "Parallel": every slot gets the dry input and
//!   the active slots' outputs are averaged, so a single active slot sounds as in series.
//! - FXn_Bypass 1 ("On") = slot active. Type and bypass changes fade the slot out, reset the new
//!   effect and fade it in (10 ms each way).
//! - FXn_Dry_Wet: out = dry·(1 - w) + wet·w, smoothed (for Param EQ it scales the band gains).
//! - `mod[0..3]` offset FXn_Dry_Wet, `mod[3..6]` offset the main parameter of each slot's effect.

pub mod delay;
pub mod dsp;
pub mod dynamics;
pub mod filters;
pub mod modulation;
pub mod reverb;

use delay::Delay;
use dsp::*;
use dynamics::{BitCrusher, Compressor, Overdrive};
use filters::{MultiFilter, ParamEq};
use modulation::{Chorus, Flanger, Phaser, StereoPan};
use reverb::Reverb;

/// Effect type names in `FXn_ModuleType` order (0 = none).
pub const TYPE_NAMES: [&str; 12] = [
    "None", "Reverb", "Delay", "Chorus", "Flanger", "Phaser", "Overdrive", "Compressor", "BitCrusher", "Filter",
    "Param EQ", "Stereo Pan",
];

/// Parameter id prefix (after `FXn_`) of each effect type.
pub const TYPE_PREFIXES: [&str; 12] = [
    "", "Dattorro_", "Delay_", "Chorus_", "Flanger_", "Phaser_", "TS808_", "Compressor_", "BitCrusher_",
    "MultiModeFilter_", "ParamEq_", "StereoPan_",
];

pub struct FxRack {
    p_enable: FxParam,
    p_routing: FxParam,
    slots: [FxSlot; 3],
    dry_l: [f32; FX_CHUNK],
    dry_r: [f32; FX_CHUNK],
    acc_l: [f32; FX_CHUNK],
    acc_r: [f32; FX_CHUNK],
    tmp_l: [f32; FX_CHUNK],
    tmp_r: [f32; FX_CHUNK],
    sample_rate: f64,
    enable_gain: f32,
    enable_step: f32,
    fresh: bool,
    idle: bool,
}

impl Default for FxRack {
    fn default() -> Self {
        Self::new()
    }
}

impl FxRack {
    pub fn new() -> Self {
        FxRack {
            p_enable: FxParam::new("FX_Enable"),
            p_routing: FxParam::new("FX_Routing"),
            slots: [FxSlot::new(0), FxSlot::new(1), FxSlot::new(2)],
            dry_l: [0.0; FX_CHUNK],
            dry_r: [0.0; FX_CHUNK],
            acc_l: [0.0; FX_CHUNK],
            acc_r: [0.0; FX_CHUNK],
            tmp_l: [0.0; FX_CHUNK],
            tmp_r: [0.0; FX_CHUNK],
            sample_rate: 0.0,
            enable_gain: 0.0,
            enable_step: 0.002,
            fresh: true,
            idle: false,
        }
    }

    /// Allocates all buffers (off the render thread). Processing runs in 32-sample chunks, so any
    /// block size works.
    pub fn prepare(&mut self, sample_rate: f64, _max_frames: usize) {
        self.sample_rate = sample_rate;
        self.enable_step = 1.0 / (0.01 * sample_rate) as f32;
        for s in self.slots.iter_mut() {
            s.prepare(sample_rate);
        }
        self.reset();
    }

    /// Clears every delay line and filter state; no allocation.
    pub fn reset(&mut self) {
        for s in self.slots.iter_mut() {
            s.reset();
        }
        self.fresh = true;
        self.idle = false;
    }

    /// Gain reduction in dB (>= 0) of the compressor running in `slot` (0...2); 0 otherwise.
    pub fn compressor_reduction_db(&self, slot: usize) -> f32 {
        self.slots.get(slot).map(|s| s.compressor_reduction_db()).unwrap_or(0.0)
    }

    /// Render-thread safe: processes the stereo signal in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], params: &[f32], tempo: f64, m: &[f32; 6]) {
        let frames = left.len();
        if self.sample_rate <= 0.0 || frames == 0 {
            return;
        }
        let on = self.p_enable.index(params, 0.0) == 1;
        if self.fresh {
            self.enable_gain = if on { 1.0 } else { 0.0 };
        }
        self.fresh = false;
        if !on && self.enable_gain <= 0.0 {
            self.idle = true;
            return;
        }
        if self.idle {
            for s in self.slots.iter_mut() {
                s.reset();
            }
            self.idle = false;
        }
        let bpm = if tempo.is_finite() && tempo >= 1.0 { tempo.min(999.0) } else { 120.0 };
        let parallel = self.p_routing.index(params, 0.0) == 1;
        for (i, s) in self.slots.iter_mut().enumerate() {
            s.begin(params, bpm, m[i], m[3 + i]);
        }

        let target: f32 = if on { 1.0 } else { 0.0 };
        let mut pos = 0;
        while pos < frames {
            let n = FX_CHUNK.min(frames - pos);
            let l = &mut left[pos..pos + n];
            let r = &mut right[pos..pos + n];
            self.dry_l[..n].copy_from_slice(l);
            self.dry_r[..n].copy_from_slice(r);
            if parallel {
                self.mix_parallel(l, r);
            } else {
                for s in self.slots.iter_mut() {
                    s.process(l, r);
                }
            }
            if self.enable_gain != target {
                let step = if on { self.enable_step } else { -self.enable_step };
                let mut g = self.enable_gain;
                for i in 0..n {
                    g = (g + step).clamp(0.0, 1.0);
                    l[i] = self.dry_l[i] + g * (l[i] - self.dry_l[i]);
                    r[i] = self.dry_r[i] + g * (r[i] - self.dry_r[i]);
                }
                self.enable_gain = g;
            }
            pos += n;
        }
    }

    /// Parallel routing: dry + sum of the slots' (output - dry), divided by the number of active
    /// slots (counted with their crossfade gains, so fades stay smooth).
    fn mix_parallel(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        self.acc_l[..n].fill(0.0);
        self.acc_r[..n].fill(0.0);
        let mut start = 0.0f32;
        let mut end = 0.0f32;
        for si in 0..3 {
            let f0 = self.slots[si].fade;
            self.tmp_l[..n].copy_from_slice(&self.dry_l[..n]);
            self.tmp_r[..n].copy_from_slice(&self.dry_r[..n]);
            if !self.slots[si].process(&mut self.tmp_l[..n], &mut self.tmp_r[..n]) {
                continue;
            }
            start += f0;
            end += self.slots[si].fade;
            for i in 0..n {
                self.acc_l[i] += self.tmp_l[i] - self.dry_l[i];
                self.acc_r[i] += self.tmp_r[i] - self.dry_r[i];
            }
        }
        let mut k = 1.0 / start.max(1.0);
        let dk = (1.0 / end.max(1.0) - k) / n as f32;
        for i in 0..n {
            k += dk;
            l[i] = self.dry_l[i] + self.acc_l[i] * k;
            r[i] = self.dry_r[i] + self.acc_r[i] * k;
        }
    }
}

/// One effect slot. Owns an instance of every effect type (all allocated in `prepare`), so a type
/// change only resets state.
struct FxSlot {
    p_type: FxParam,
    p_bypass: FxParam,
    p_dry_wet: FxParam,
    reverb: Reverb,
    delay: Delay,
    chorus: Chorus,
    flanger: Flanger,
    phaser: Phaser,
    overdrive: Overdrive,
    compressor: Compressor,
    crusher: BitCrusher,
    filter: MultiFilter,
    eq: ParamEq,
    pan: StereoPan,
    dry_l: [f32; FX_CHUNK],
    dry_r: [f32; FX_CHUNK],
    /// Effect type currently running (0 = none) and its crossfade gain.
    current: usize,
    fade: f32,
    want: usize,
    /// Bit per effect type touched since its last reset; only those need clearing.
    dirty: u32,
    wet_t: f32,
    wet: f32,
    fade_step: f32,
    c_wet: f32,
    fresh: bool,
    params: [f32; crate::params::COUNT],
    tempo: f64,
    main_mod: f32,
}

impl FxSlot {
    fn new(slot: usize) -> Self {
        FxSlot {
            p_type: FxParam::slot(slot, "ModuleType"),
            p_bypass: FxParam::slot(slot, "Bypass"),
            p_dry_wet: FxParam::slot(slot, "Dry_Wet"),
            reverb: Reverb::new(slot),
            delay: Delay::new(slot),
            chorus: Chorus::new(slot),
            flanger: Flanger::new(slot),
            phaser: Phaser::new(slot),
            overdrive: Overdrive::new(slot),
            compressor: Compressor::new(slot),
            crusher: BitCrusher::new(slot),
            filter: MultiFilter::new(slot),
            eq: ParamEq::new(slot),
            pan: StereoPan::new(slot),
            dry_l: [0.0; FX_CHUNK],
            dry_r: [0.0; FX_CHUNK],
            current: 0,
            fade: 0.0,
            want: 0,
            dirty: 0,
            wet_t: 0.0,
            wet: 0.0,
            fade_step: 0.002,
            c_wet: 0.001,
            fresh: true,
            params: [0.0; crate::params::COUNT],
            tempo: 120.0,
            main_mod: 0.0,
        }
    }

    fn compressor_reduction_db(&self) -> f32 {
        if self.current == 7 {
            self.compressor.reduction_db
        } else {
            0.0
        }
    }

    fn prepare(&mut self, sample_rate: f64) {
        self.reverb.prepare(sample_rate);
        self.delay.prepare(sample_rate);
        self.chorus.prepare(sample_rate);
        self.flanger.prepare(sample_rate);
        self.phaser.prepare(sample_rate);
        self.overdrive.prepare(sample_rate);
        self.compressor.prepare(sample_rate);
        self.crusher.prepare(sample_rate);
        self.filter.prepare(sample_rate);
        self.eq.prepare(sample_rate);
        self.pan.prepare(sample_rate);
        self.fade_step = 1.0 / (0.01 * sample_rate) as f32;
        self.c_wet = smooth_coef(0.02, sample_rate, 1);
        self.reset();
    }

    fn reset(&mut self) {
        for t in 1..=11 {
            if self.dirty & (1 << t) != 0 {
                self.reset_effect(t);
            }
        }
        self.current = 0;
        self.fade = 0.0;
        self.fresh = true;
    }

    /// Reads the slot parameters once per host block.
    fn begin(&mut self, p: &[f32], tempo: f64, mix_mod: f32, main_mod: f32) {
        self.params.copy_from_slice(&p[..crate::params::COUNT]);
        self.tempo = tempo;
        self.main_mod = main_mod;
        self.want = if self.p_bypass.index(p, 0.0) == 1 { self.p_type.index(p, 0.0) } else { 0 };
        self.wet_t = self.p_dry_wet.float(p, mix_mod) / 100.0;
        if self.want == 10 {
            // Param EQ: Dry/Wet scales the band gains instead of crossfading
            self.eq.dry_wet_scale = self.wet_t;
            self.wet_t = 1.0;
        }
        if self.fresh {
            self.current = self.want;
            self.fade = if self.want == 0 { 0.0 } else { 1.0 };
            self.wet = self.wet_t;
            self.fresh = false;
        }
        if self.current != 0 {
            self.update(self.current);
        }
    }

    /// Processes one chunk in place. Returns false if the slot is empty (signal untouched).
    fn process(&mut self, l: &mut [f32], r: &mut [f32]) -> bool {
        let n = l.len();
        if self.current != self.want && self.fade <= 0.0 {
            self.current = self.want;
            if self.current != 0 {
                self.reset_effect(self.current);
                self.update(self.current);
            }
        }
        if self.current == 0 {
            self.fade = 0.0;
            return false;
        }
        self.dry_l[..n].copy_from_slice(l);
        self.dry_r[..n].copy_from_slice(r);
        self.run(self.current, l, r);
        let step = if self.current == self.want { self.fade_step } else { -self.fade_step };
        let (wt, cw) = (self.wet_t, self.c_wet);
        let (mut f, mut w) = (self.fade, self.wet);
        for i in 0..n {
            f = (f + step).clamp(0.0, 1.0);
            w += (wt - w) * cw;
            let m = f * w;
            l[i] = self.dry_l[i] + m * (l[i] - self.dry_l[i]);
            r[i] = self.dry_r[i] + m * (r[i] - self.dry_r[i]);
        }
        self.fade = f;
        self.wet = w;
        true
    }

    fn update(&mut self, kind: usize) {
        let (m, t) = (self.main_mod, self.tempo);
        let p = &self.params;
        self.dirty |= 1 << kind;
        match kind {
            1 => self.reverb.update(p, m, t),
            2 => self.delay.update(p, m, t),
            3 => self.chorus.update(p, m, t),
            4 => self.flanger.update(p, m, t),
            5 => self.phaser.update(p, m, t),
            6 => self.overdrive.update(p, m, t),
            7 => self.compressor.update(p, m, t),
            8 => self.crusher.update(p, m, t),
            9 => self.filter.update(p, m, t),
            10 => self.eq.update(p, m, t),
            11 => self.pan.update(p, m, t),
            _ => {}
        }
    }

    fn reset_effect(&mut self, kind: usize) {
        self.dirty &= !(1 << kind);
        match kind {
            1 => self.reverb.reset(),
            2 => self.delay.reset(),
            3 => self.chorus.reset(),
            4 => self.flanger.reset(),
            5 => self.phaser.reset(),
            6 => self.overdrive.reset(),
            7 => self.compressor.reset(),
            8 => self.crusher.reset(),
            9 => self.filter.reset(),
            10 => self.eq.reset(),
            11 => self.pan.reset(),
            _ => {}
        }
    }

    fn run(&mut self, kind: usize, l: &mut [f32], r: &mut [f32]) {
        match kind {
            1 => self.reverb.process(l, r),
            2 => self.delay.process(l, r),
            3 => self.chorus.process(l, r),
            4 => self.flanger.process(l, r),
            5 => self.phaser.process(l, r),
            6 => self.overdrive.process(l, r),
            7 => self.compressor.process(l, r),
            8 => self.crusher.process(l, r),
            9 => self.filter.process(l, r),
            10 => self.eq.process(l, r),
            11 => self.pan.process(l, r),
            _ => {}
        }
    }
}
