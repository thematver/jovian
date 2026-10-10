//! The complete instrument: 16 voices, global LFO, hybrid LFOs, modulation matrix, voice assign
//! modes, arpeggiator and the effects rack. Everything on the render path is allocation free.

use super::arp::Arpeggiator;
use super::modulation::{self as m, dest, HybridLfoSettings, HybridLfoState, LFO2_ADDR, LFO3_ADDR};
use super::patch::Patch;
use super::primitives::*;
use super::voice::{Voice, VoiceControl};
use super::voice_math as vm;
use crate::fx::FxRack;
use crate::params::{addr as a, Values, PARAMS};

/// Host transport information for one render call.
#[derive(Debug, Clone, Copy)]
pub struct HostTime {
    pub tempo: f64,
    pub beat_position: f64,
    pub is_playing: bool,
}

impl Default for HostTime {
    fn default() -> Self {
        HostTime { tempo: 120.0, beat_position: 0.0, is_playing: false }
    }
}

pub const MAX_VOICES: usize = 16;
/// Host samples per control-rate update.
pub const CONTROL_BLOCK: usize = 16;
const DEST_STRIDE: usize = dest::COUNT;

/// Fixed-capacity list of held notes in press order.
#[derive(Clone)]
struct NoteList {
    notes: [i32; 32],
    velocities: [f32; 32],
    count: usize,
}

impl NoteList {
    const CAPACITY: usize = 32;

    fn new() -> Self {
        NoteList { notes: [0; 32], velocities: [0.0; 32], count: 0 }
    }

    fn remove_all(&mut self) {
        self.count = 0;
    }

    fn append(&mut self, note: i32, velocity: f32) {
        if self.count == Self::CAPACITY {
            self.remove_at(0);
        }
        self.notes[self.count] = note;
        self.velocities[self.count] = velocity;
        self.count += 1;
    }

    fn remove_at(&mut self, i: usize) {
        if i >= self.count {
            return;
        }
        for j in i..self.count - 1 {
            self.notes[j] = self.notes[j + 1];
            self.velocities[j] = self.velocities[j + 1];
        }
        self.count -= 1;
    }

    fn remove(&mut self, note: i32) {
        let mut i = 0;
        while i < self.count {
            if self.notes[i] == note {
                self.remove_at(i);
            } else {
                i += 1;
            }
        }
    }

    /// Note priority: 0 low, 1 high, 2 last.
    fn pick(&self, priority: usize) -> (i32, f32) {
        if self.count == 0 {
            return (60, 0.8);
        }
        let mut best = self.count - 1;
        if priority != 2 {
            for i in 0..self.count {
                let better = if priority == 0 { self.notes[i] < self.notes[best] } else { self.notes[i] > self.notes[best] };
                if better {
                    best = i;
                }
            }
        }
        (self.notes[best], self.velocities[best])
    }
}

/// Live state the editor can show (written by the render thread, read anywhere).
#[derive(Debug, Clone, Copy, Default)]
pub struct Meters {
    /// Bit per voice that is sounding.
    pub active_voices: u16,
    /// Bit per voice whose key is down.
    pub gated_voices: u16,
    /// Global LFO 1 output (-1...1).
    pub lfo: f32,
    /// Envelope levels of the most recently started voice.
    pub env1: f32,
    pub env2: f32,
}

pub struct SynthEngine {
    /// Normalized parameter values (Jup-8 V storage). The host wrapper writes these.
    pub params: Values,
    fx: FxRack,

    sample_rate: f64,
    os_rate: f64,
    patch: Patch,
    lfo2_set: HybridLfoSettings,
    lfo3_set: HybridLfoSettings,

    voices: [Voice; MAX_VOICES],
    controls: [VoiceControl; MAX_VOICES],
    dest: [[f32; DEST_STRIDE]; MAX_VOICES],
    global_dest: [f32; DEST_STRIDE],
    os_l: [f32; 2 * CONTROL_BLOCK],
    os_r: [f32; 2 * CONTROL_BLOCK],
    fx_mod: [f32; 6],
    dec_l: HalfBandDecimator,
    dec_r: HalfBandDecimator,

    // active matrix slots for this block
    slot_count: usize,
    slot_source: [u8; 24],
    slot_dest: [usize; 24],
    slot_amount: [f32; 24],
    curves: [[f32; 4]; 4],

    // performance state
    mod_wheel: f32,
    bend: f32,
    aftertouch: f32,
    slide: f32,
    sustain_pedal: bool,
    key_down: [bool; 128],
    held: NoteList,
    last_velocity: f32,
    last_note: i32,
    age_counter: u64,
    round_robin: usize,
    last_started: usize,

    // LFO 1 (global)
    lfo_phase: f64,
    lfo_value: f32,
    lfo_sh: f32,
    lfo_noise: Noise,
    lfo2_global: HybridLfoState,
    lfo3_global: HybridLfoState,

    // arpeggiator
    arp: Arpeggiator,
    arp_was_on: bool,

    pub meters: Meters,
}

impl Default for SynthEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SynthEngine {
    pub fn new() -> Self {
        let _ = m::slots(); // initialize the lazily built tables off the render thread
        let mut voices = [Voice::default(); MAX_VOICES];
        for (i, v) in voices.iter_mut().enumerate() {
            v.randomize((i as u32).wrapping_mul(2_654_435).wrapping_add(17));
            let sign = if i % 2 == 0 { -1.0 } else { 1.0 };
            v.pan_pos = sign * (i / 2 + 1) as f32 / (MAX_VOICES / 2) as f32;
        }
        let mut e = SynthEngine {
            params: crate::params::defaults(),
            fx: FxRack::new(),
            sample_rate: 48_000.0,
            os_rate: 96_000.0,
            patch: Patch::default(),
            lfo2_set: HybridLfoSettings::default(),
            lfo3_set: HybridLfoSettings::default(),
            voices,
            controls: [VoiceControl::default(); MAX_VOICES],
            dest: [[0.0; DEST_STRIDE]; MAX_VOICES],
            global_dest: [0.0; DEST_STRIDE],
            os_l: [0.0; 2 * CONTROL_BLOCK],
            os_r: [0.0; 2 * CONTROL_BLOCK],
            fx_mod: [0.0; 6],
            dec_l: HalfBandDecimator::new(),
            dec_r: HalfBandDecimator::new(),
            slot_count: 0,
            slot_source: [0; 24],
            slot_dest: [0; 24],
            slot_amount: [0.0; 24],
            curves: [[0.0, 0.5, 0.5, 1.0]; 4],
            mod_wheel: 0.0,
            bend: 0.0,
            aftertouch: 0.0,
            slide: 0.0,
            sustain_pedal: false,
            key_down: [false; 128],
            held: NoteList::new(),
            last_velocity: 0.8,
            last_note: 60,
            age_counter: 0,
            round_robin: 0,
            last_started: 0,
            lfo_phase: 0.0,
            lfo_value: 0.0,
            lfo_sh: 0.0,
            lfo_noise: Noise::new(0x1234_5678),
            lfo2_global: HybridLfoState::default(),
            lfo3_global: HybridLfoState::default(),
            arp: Arpeggiator::default(),
            arp_was_on: false,
            meters: Meters::default(),
        };
        e.prepare(48_000.0, 4096);
        e
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Call off the render thread before rendering (and whenever the format changes).
    pub fn prepare(&mut self, sample_rate: f64, max_frames: usize) {
        self.sample_rate = sample_rate;
        self.os_rate = sample_rate * 2.0;
        self.fx.prepare(sample_rate, max_frames);
        self.reset();
    }

    pub fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            v.active = false;
            v.gate = false;
            v.env1 = Envelope::default();
            v.env2 = Envelope::default();
            v.ladder.reset();
            v.hpf = OnePoleHp::default();
            v.c.amp = 0.0;
        }
        self.held.remove_all();
        self.key_down = [false; 128];
        self.arp.reset();
        self.dec_l.reset();
        self.dec_r.reset();
        self.fx.reset();
    }

    /// Gain reduction (dB) of the compressor in `slot` (0...2), for the effect page meter.
    pub fn compressor_reduction_db(&self, slot: usize) -> f32 {
        self.fx.compressor_reduction_db(slot)
    }

    // MARK: - MIDI

    /// Handles one MIDI 1.0 channel message (any channel).
    pub fn handle_midi(&mut self, status: u8, d1: u8, d2: u8) {
        match status & 0xF0 {
            0x90 if d2 > 0 => self.note_on(d1 as i32, d2 as f32 / 127.0),
            0x80 | 0x90 => self.note_off(d1 as i32),
            0xB0 => match d1 {
                1 => self.mod_wheel = d2 as f32 / 127.0,
                64 => self.set_sustain(d2 >= 64),
                74 => self.slide = d2 as f32 / 127.0,
                120 | 123 => self.all_notes_off(d1 == 120),
                _ => {}
            },
            0xD0 => self.aftertouch = d1 as f32 / 127.0,
            0xA0 => self.aftertouch = self.aftertouch.max(d2 as f32 / 127.0),
            0xE0 => self.bend = (((d2 as i32) << 7 | d1 as i32) - 8192) as f32 / 8192.0,
            _ => {}
        }
    }

    pub fn set_mod_wheel(&mut self, v: f32) {
        self.mod_wheel = v.clamp(0.0, 1.0);
    }

    pub fn set_pitch_bend(&mut self, v: f32) {
        self.bend = v.clamp(-1.0, 1.0);
    }

    pub fn set_aftertouch(&mut self, v: f32) {
        self.aftertouch = v.clamp(0.0, 1.0);
    }

    pub fn note_on(&mut self, note: i32, velocity: f32) {
        if !(0..128).contains(&note) {
            return;
        }
        self.key_down[note as usize] = true;
        if self.patch.arp_on {
            self.arp.add(note, velocity);
            return;
        }
        self.start_note(note, velocity);
    }

    pub fn note_off(&mut self, note: i32) {
        if !(0..128).contains(&note) {
            return;
        }
        self.key_down[note as usize] = false;
        if self.patch.arp_on {
            if !self.sustain_pedal {
                self.arp.remove(note);
            }
            return;
        }
        let pedal = self.sustain_pedal;
        self.stop_note(note, pedal);
    }

    pub fn all_notes_off(&mut self, hard: bool) {
        self.held.remove_all();
        self.arp.reset();
        self.key_down = [false; 128];
        for v in self.voices.iter_mut() {
            v.gate = false;
            v.sustained = false;
            v.env1.gate_off();
            v.env2.gate_off();
            if hard {
                v.active = false;
                v.env2.level = 0.0;
                v.env2.stage = Stage::Idle;
            }
        }
    }

    fn set_sustain(&mut self, on: bool) {
        self.sustain_pedal = on;
        if on {
            return;
        }
        if self.patch.arp_on {
            let kd = self.key_down;
            self.arp.keep_only(|n| kd[n as usize]);
            return;
        }
        for i in 0..MAX_VOICES {
            if self.voices[i].sustained {
                self.voices[i].sustained = false;
                if !self.key_down[self.voices[i].note as usize] {
                    self.release_voice(i);
                }
            }
        }
        let mut i = 0;
        while i < self.held.count {
            if !self.key_down[self.held.notes[i] as usize] {
                self.held.remove_at(i);
            } else {
                i += 1;
            }
        }
        if self.is_mono_mode() && self.held.count == 0 {
            for v in 0..MAX_VOICES {
                if self.voices[v].gate {
                    self.release_voice(v);
                }
            }
        }
    }

    // MARK: - Voice assignment

    fn is_mono_mode(&self) -> bool {
        self.patch.assign == 0 || (self.patch.assign == 1 && self.patch.uni_voices == 0)
    }

    fn voice_limit(&self) -> usize {
        self.patch.polyphony.clamp(1, MAX_VOICES)
    }

    fn start_note(&mut self, note: i32, velocity: f32) {
        self.last_note = note;
        self.last_velocity = velocity;
        self.held.remove(note);
        self.held.append(note, velocity);
        if self.patch.lfo_retrig {
            self.lfo_phase = 0.0;
        }
        if !self.lfo2_set.poly {
            self.lfo2_global.trigger(&self.lfo2_set);
        }
        if !self.lfo3_set.poly {
            self.lfo3_global.trigger(&self.lfo3_set);
        }

        if self.is_mono_mode() {
            let stack = if self.patch.assign == 0 { 1 } else { self.voice_limit() };
            let (tn, tv) = self.held.pick(self.patch.prio);
            for j in 0..stack {
                self.assign(j, tn, tv, j, stack, true);
            }
            for j in stack..MAX_VOICES {
                if self.voices[j].gate {
                    self.release_voice(j);
                }
            }
            return;
        }
        let stack = if self.patch.assign == 1 { self.patch.uni_voices.min(self.voice_limit()).max(1) } else { 1 };
        let slots = (self.voice_limit() / stack).max(1) * stack;
        // re-striking a held/sounding note reuses its voices
        let mut picks = [0usize; MAX_VOICES];
        let mut count = 0;
        for i in 0..slots {
            if self.voices[i].active && self.voices[i].note == note && count < stack {
                picks[count] = i;
                count += 1;
            }
        }
        while count < stack {
            let v = self.free_voice(slots, &picks[..count]);
            picks[count] = v;
            count += 1;
        }
        for j in 0..stack {
            self.assign(picks[j], note, velocity, j, stack, true);
        }
    }

    /// Picks a voice: idle first, then the quietest released one, then the oldest held one.
    fn free_voice(&mut self, limit: usize, exclude: &[usize]) -> usize {
        let excluded = |i: usize| exclude.contains(&i);
        let rotate = self.patch.assign == 2; // Poly 1 rotates, Poly 2 / unison fill from the bottom
        for n in 0..limit {
            let i = if rotate { (self.round_robin + n) % limit } else { n };
            if !self.voices[i].active && !excluded(i) {
                if rotate {
                    self.round_robin = (i + 1) % limit;
                }
                return i;
            }
        }
        let mut best: isize = -1;
        let mut best_level = f32::MAX;
        for i in 0..limit {
            if !self.voices[i].gate && !excluded(i) && self.voices[i].env2.level < best_level {
                best_level = self.voices[i].env2.level;
                best = i as isize;
            }
        }
        if best >= 0 {
            return best as usize;
        }
        let mut oldest = u64::MAX;
        for i in 0..limit {
            if !excluded(i) && self.voices[i].age < oldest {
                oldest = self.voices[i].age;
                best = i as isize;
            }
        }
        best.max(0) as usize
    }

    fn assign(&mut self, i: usize, note: i32, velocity: f32, stack_index: usize, stack_size: usize, retrigger: bool) {
        self.age_counter = self.age_counter.wrapping_add(1);
        let porta = self.patch.porta_on && self.patch.porta_ms > 0.0;
        let lfo_delay = self.patch.lfo_delay;
        let (l2, l3) = (self.lfo2_set, self.lfo3_set);
        let v = &mut self.voices[i];
        let was_active = v.active;
        v.note = note;
        v.velocity = velocity;
        v.age = self.age_counter;
        v.stack_index = stack_index;
        v.stack_size = stack_size;
        v.gate = true;
        v.sustained = false;
        v.active = true;
        v.target_pitch = note as f64;
        if !(porta && v.has_played) {
            v.pitch = note as f64;
        }
        v.has_played = true;
        if retrigger || !was_active {
            v.env1.gate_on();
            v.env2.gate_on();
            v.lfo_fade = if lfo_delay > 0.001 { 0.0 } else { 1.0 };
            if l2.poly {
                v.lfo2.trigger(&l2);
            }
            if l3.poly {
                v.lfo3.trigger(&l3);
            }
        }
        self.last_started = i;
    }

    fn release_voice(&mut self, i: usize) {
        let v = &mut self.voices[i];
        v.gate = false;
        v.env1.gate_off();
        v.env2.gate_off();
    }

    fn stop_note(&mut self, note: i32, pedal: bool) {
        if self.is_mono_mode() {
            if pedal {
                for v in self.voices.iter_mut() {
                    if v.gate {
                        v.sustained = true;
                    }
                }
                return;
            }
            self.held.remove(note);
            if self.held.count > 0 {
                let (tn, tv) = self.held.pick(self.patch.prio);
                let stack = if self.patch.assign == 0 { 1 } else { self.voice_limit() };
                if self.voices[0].note != tn {
                    for j in 0..stack {
                        self.assign(j, tn, tv, j, stack, false);
                    }
                }
            } else {
                for i in 0..MAX_VOICES {
                    if self.voices[i].gate {
                        self.release_voice(i);
                    }
                }
            }
            return;
        }
        self.held.remove(note);
        for i in 0..MAX_VOICES {
            if self.voices[i].gate && self.voices[i].note == note {
                if pedal {
                    self.voices[i].sustained = true;
                } else {
                    self.release_voice(i);
                }
            }
        }
    }

    // MARK: - Arpeggiator clock

    fn run_arp(&mut self, dt: f64) {
        if self.patch.arp_on != self.arp_was_on {
            // switching modes hands the held keys over
            if self.patch.arp_on {
                for i in 0..self.held.count {
                    self.arp.add(self.held.notes[i], self.held.velocities[i]);
                }
                self.held.remove_all();
                for i in 0..MAX_VOICES {
                    if self.voices[i].gate {
                        self.release_voice(i);
                    }
                }
            } else {
                if let Some(n) = self.arp.sounding {
                    self.stop_note(n, false);
                }
                for i in 0..self.arp.count {
                    let n = self.arp.notes[i];
                    if self.key_down[n as usize] {
                        let v = self.arp.velocities[i];
                        self.start_note(n, v);
                    }
                }
                self.arp.reset();
            }
            self.arp_was_on = self.patch.arp_on;
        }
        if !self.patch.arp_on {
            return;
        }
        let ev = self.arp.advance(dt, self.patch.arp_hz, self.patch.arp_mode, self.patch.arp_octaves);
        if let Some(off) = ev.off {
            self.stop_note(off, false);
        }
        if let Some(on) = ev.on {
            self.start_note(on, ev.velocity);
        }
    }

    // MARK: - Rendering

    /// Renders `left.len()` samples of stereo output, replacing the buffer contents.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32], time: HostTime) {
        let frames = left.len().min(right.len());
        let mut done = 0;
        let tempo = if time.tempo > 0.0 { time.tempo } else { 120.0 };
        while done < frames {
            let n = CONTROL_BLOCK.min(frames - done);
            self.control(n, tempo);
            let os = 2 * n;
            self.os_l[..os].fill(0.0);
            self.os_r[..os].fill(0.0);
            let (w1, w2, sync, two) = (self.patch.osc1_wave, self.patch.osc2_wave, self.patch.sync, self.patch.two_pole);
            for i in 0..MAX_VOICES {
                if !self.voices[i].active {
                    continue;
                }
                let xm_amt = (self.patch.cross_mod + self.dest[i][dest::CROSS_MOD] as f64).clamp(0.0, 1.0) as f32;
                let c = self.controls[i];
                let v = &mut self.voices[i];
                v.render(&mut self.os_l[..os], &mut self.os_r[..os], &c, w1, w2, sync, xm_amt, two);
                if v.env2.stage == Stage::Idle && !v.gate {
                    v.active = false;
                }
            }
            let g = self.patch.gain as f32;
            for s in 0..n {
                left[done + s] = self.dec_l.process(self.os_l[2 * s], self.os_l[2 * s + 1]) * g;
                right[done + s] = self.dec_r.process(self.os_r[2 * s], self.os_r[2 * s + 1]) * g;
            }
            done += n;
        }
        for i in 0..3 {
            self.fx_mod[i] = self.global_dest[dest::FX1_MIX + i];
            self.fx_mod[3 + i] = self.global_dest[dest::FX1_PARAM + i];
        }
        self.fx.process(&mut left[..frames], &mut right[..frames], &self.params, tempo, &self.fx_mod);
        self.update_meters();
    }

    fn update_meters(&mut self) {
        let mut act = 0u16;
        let mut gated = 0u16;
        for (i, v) in self.voices.iter().enumerate() {
            if v.active {
                act |= 1 << i;
            }
            if v.gate {
                gated |= 1 << i;
            }
        }
        let v = &self.voices[self.last_started];
        self.meters = Meters {
            active_voices: act,
            gated_voices: gated,
            lfo: self.lfo_value,
            env1: if v.active { v.env1.level } else { 0.0 },
            env2: if v.active { v.env2.level } else { 0.0 },
        };
    }

    /// Control-rate update for the next `frames` host samples.
    fn control(&mut self, frames: usize, tempo: f64) {
        let p = self.params;
        self.patch.update(&p, tempo);
        self.lfo2_set.update(&p, &LFO2_ADDR, tempo);
        self.lfo3_set.update(&p, &LFO3_ADDR, tempo);
        let dt = frames as f64 / self.sample_rate;
        let os_n = 2 * frames;

        self.run_arp(dt);
        self.update_matrix_slots(&p);

        // global modulation sources (last played note) and destinations
        let gl2 = self.lfo2_global.advance(&self.lfo2_set, dt, 1.0, 0.0);
        let gl3 = self.lfo3_global.advance(&self.lfo3_set, dt, 1.0, 0.0);
        let mut gd = [0.0f32; DEST_STRIDE];
        self.accumulate(&mut gd, gl2, gl3, self.last_velocity, self.last_note, 1.0);
        self.global_dest = gd;

        // LFO 1
        let rate_mul = (self.global_dest[dest::LFO1_RATE] as f64 * 8.0).exp2();
        self.lfo_phase += self.patch.lfo_hz * rate_mul * dt;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= self.lfo_phase.floor();
            self.lfo_sh = self.lfo_noise.next();
        }
        self.lfo_value = match self.patch.lfo_wave {
            0 => (-(2.0 * std::f64::consts::PI * self.lfo_phase).cos()) as f32,
            1 => (1.0 - 2.0 * self.lfo_phase) as f32,
            2 => {
                if self.lfo_phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            _ => self.lfo_sh,
        };
        let lfo_delay = (self.patch.lfo_delay * (1.0 + self.global_dest[dest::LFO1_DELAY] as f64 * 4.0)).max(0.0);
        let fade_step = (if lfo_delay > 0.001 { dt / lfo_delay } else { 1.0 }) as f32;

        let pt = self.patch;
        let bend_semis = self.bend as f64 * pt.bend_st;
        let mw_vib = if pt.mw_vco { (self.lfo_value * self.mod_wheel) as f64 * pt.mw_vco_st } else { 0.0 };
        let mw_cut = if pt.mw_vcf {
            (self.lfo_value * self.mod_wheel) as f64 * pt.mw_vcf_amt * vm::MOD_WHEEL_TO_CUTOFF
        } else {
            0.0
        };
        let bend_cut = if pt.bend_f { self.bend as f64 * pt.bend_f_amt * pt.bend_f_amt * vm::BEND_TO_CUTOFF } else { 0.0 };
        let porta_coef =
            if pt.porta_on && pt.porta_ms > 0.0 { 1.0 - (-dt * 4.6 / (pt.porta_ms / 1000.0)).exp() } else { 1.0 };
        let os_rate = self.os_rate;
        let nyq = os_rate * 0.45;
        let lfo_value = self.lfo_value;
        let pi = std::f64::consts::PI;

        for i in 0..MAX_VOICES {
            if !self.voices[i].active {
                continue;
            }
            let mut d = [0.0f32; DEST_STRIDE];
            let mod_scale = 1.0 + pt.disp_mod as f32 * self.voices[i].r_mod * 0.2;
            let l2 = if self.lfo2_set.poly {
                let s = self.lfo2_set;
                self.voices[i].lfo2.advance(&s, dt, 1.0, 0.0)
            } else {
                gl2
            };
            let l3 = if self.lfo3_set.poly {
                let s = self.lfo3_set;
                self.voices[i].lfo3.advance(&s, dt, 1.0, 0.0)
            } else {
                gl3
            };
            if self.slot_count > 0 {
                let (vel, note) = (self.voices[i].velocity, self.voices[i].note);
                self.accumulate(&mut d, l2, l3, vel, note, mod_scale);
            }
            self.dest[i] = d;
            let modv = |base: usize, di: usize| -> f32 { (p[base] + d[di]).clamp(0.0, 1.0) };
            let phys = |base: usize, di: usize| -> f64 {
                if d[di] == 0.0 {
                    PARAMS[base].physical_f32(p[base])
                } else {
                    PARAMS[base].physical_f32(modv(base, di))
                }
            };

            let v = &mut self.voices[i];
            // envelopes at control rate
            let note = v.pitch;
            let env_scale = 1.0 + pt.disp_env * v.r_env as f64 * 0.25;
            let kf1 = if pt.e1_kf { vm::env_key_follow(note) } else { 1.0 };
            let kf2 = if pt.e2_kf { vm::env_key_follow(note) } else { 1.0 };
            let e1 = step_envelope(
                &mut v.env1,
                phys(a::Env1_Attack, dest::E1A) * kf1 * env_scale,
                phys(a::Env1_Decay, dest::E1D) * kf1 * env_scale,
                phys(a::Env1_Sustain, dest::E1S) / 100.0,
                phys(a::Env1_Release, dest::E1R) * kf1 * env_scale,
                os_n,
                os_rate,
            );
            let e2 = step_envelope(
                &mut v.env2,
                phys(a::Env2_Attack, dest::E2A) * kf2 * env_scale,
                phys(a::Env2_Decay, dest::E2D) * kf2 * env_scale,
                phys(a::Env2_Sustain, dest::E2S) / 100.0,
                phys(a::Env2Release, dest::E2R) * kf2 * env_scale,
                os_n,
                os_rate,
            );
            let env1 = if pt.e1_inv { -e1 } else { e1 };

            // LFO 1 with per-voice delay fade
            if v.lfo_fade < 1.0 {
                v.lfo_fade = 1f32.min(v.lfo_fade + fade_step);
            }
            let lfo = (lfo_value * v.lfo_fade) as f64;

            // portamento
            v.pitch += (v.target_pitch - v.pitch) * porta_coef;

            // pitch
            let stack = v.stack_size;
            let detune = if stack > 1 {
                pt.detune_st
                    * (1.0 + d[dest::DETUNE] as f64 * 4.0)
                    * (2.0 * v.stack_index as f64 / (stack - 1) as f64 - 1.0)
            } else {
                0.0
            };
            let vib = lfo * phys(a::OscLfoAmount, dest::VCO_LFO) * vm::LFO_PITCH_SCALE;
            let env_p = env1 * phys(a::OscEnvAmount, dest::VCO_ENV);
            let mod_p = vib + env_p;
            let dsel = pt.vco_dest;
            let disp = pt.disp_pitch * 0.5;
            let mut p1 = v.pitch + pt.osc1_semis + detune + mw_vib + disp * v.r_pitch1 as f64;
            let mut p2 = v.pitch + pt.osc2_semis + pt.osc2_fine + detune + mw_vib + disp * v.r_pitch2 as f64;
            if dsel >= 1 {
                p1 += mod_p;
            }
            if dsel <= 1 {
                p2 += mod_p;
            }
            if pt.bend_v1 {
                p1 += bend_semis;
            }
            if pt.bend_v2 {
                p2 += bend_semis;
            }
            p1 += (d[dest::VCO1_COARSE] + d[dest::VCO12_COARSE]) as f64 * 36.0;
            p2 += (d[dest::VCO2_COARSE] + d[dest::VCO12_COARSE]) as f64 * 36.0 + d[dest::VCO2_FINE] as f64 * 2.0;
            let f1 = pt.a4 * ((p1 - 69.0) / 12.0).exp2();
            let f2 = if pt.osc2_low {
                let mut low_semis =
                    pt.osc2_semis + pt.osc2_fine + (d[dest::VCO2_COARSE] + d[dest::VCO12_COARSE]) as f64 * 36.0;
                if dsel <= 1 {
                    low_semis += mod_p;
                }
                vm::VCO2_LOW_BASE_HZ * (low_semis / 12.0).exp2()
            } else {
                pt.a4 * ((p2 - 69.0) / 12.0).exp2()
            };

            // pulse width
            let pw_amt = phys(a::OscPwmAmount, dest::PW_AMOUNT) / 100.0;
            let pw_src = match pt.pw_mode {
                0 => env1.abs(),
                1 => 1.0,
                _ => 0.5 + 0.5 * lfo,
            };
            let pw_disp = pt.disp_pw * v.r_pw as f64 * 0.03;
            let duty = vm::pulse_duty(pw_amt * pw_src) + pw_disp;
            let duty1 = (duty + d[dest::VCO1_PW] as f64 * 0.45).clamp(0.05, 0.95);
            let duty2 = (duty + d[dest::VCO2_PW] as f64 * 0.45).clamp(0.05, 0.95);

            // mixer
            let mix = modv(a::OscMix, dest::MIX) as f64 * pi / 2.0;

            // HPF
            let hp = phys(a::HPF_Cutoff, dest::HPF).min(nyq);
            let gh = (pi * hp / os_rate).tan();

            // VCF
            let env_src = if pt.vcf_env_sel == 0 { env1 } else { e2 };
            let mut x = modv(a::LpfCut, dest::CUTOFF) as f64;
            x += modv(a::LpfEnvAmount, dest::VCF_ENV) as f64 * vm::ENV_TO_CUTOFF * env_src;
            x += modv(a::LfoFiltAmount, dest::VCF_LFO) as f64 * vm::LFO_TO_CUTOFF * lfo;
            x += mw_cut + bend_cut + pt.disp_cut * v.r_cut as f64 * 0.04;
            let key = modv(a::LPF_KeyFollowAmount, dest::VCF_KEY) as f64 * vm::KEY_FOLLOW_OCT_PER_OCT * (note - 60.0) / 12.0;
            let fc = (vm::cutoff_hz(x) * key.exp2()).max(8.0).min(nyq);
            let g = (pi * fc / os_rate).tan();
            let reso = modv(a::LpfRes, dest::RESO) as f64 + pt.disp_res * v.r_res as f64 * 0.05;

            // VCA
            let level = (modv(a::VCA_EnvAmount, dest::VCA_LEVEL) as f64).powf(vm::VCA_LEVEL_EXPONENT);
            let dip = (modv(a::VCA_LfoAmount, dest::VCA_LFO) as f64).powf(vm::VCA_LFO_EXPONENT) * (1.0 - lfo) / 2.0;
            let stack_gain = 1.0; // the reference sums unison voices without normalization

            // pan
            let spread = modv(a::PanSpread, dest::PAN) as f64;
            let pan_pos =
                if stack > 1 { 2.0 * v.stack_index as f64 / (stack - 1) as f64 - 1.0 } else { v.pan_pos as f64 };
            let angle = (spread * pan_pos + 1.0) * pi / 4.0;

            let c = VoiceControl {
                inc1: (f1 / os_rate) as f32,
                inc2: (f2 / os_rate).min(0.45) as f32,
                g: (g / (1.0 + g)) as f32,
                k: vm::resonance_k(reso.clamp(0.0, 1.0)) as f32,
                hp_g: (gh / (1.0 + gh)) as f32,
                duty1: duty1 as f32,
                duty2: duty2 as f32,
                mix1: mix.cos() as f32,
                mix2: mix.sin() as f32,
                amp: (e2 * level * (1.0 - dip) * stack_gain) as f32,
                pan_l: (angle.cos() * 1.414_213_56) as f32,
                pan_r: (angle.sin() * 1.414_213_56) as f32,
            };
            self.controls[i] = c;
            if v.needs_control_copy() {
                v.copy_control(&c);
            }
        }
    }

    // MARK: - Matrix

    fn update_matrix_slots(&mut self, p: &Values) {
        self.slot_count = 0;
        for s in m::slots().iter() {
            let di = PARAMS[s.dest_address].index_f32(p[s.dest_address]) as isize - 1;
            if di < 0 {
                continue;
            }
            let amt = PARAMS[s.amount_address].physical_f32(p[s.amount_address]) as f32;
            if amt.abs() <= 1e-5 {
                continue;
            }
            self.slot_source[self.slot_count] = s.source;
            self.slot_dest[self.slot_count] = di as usize;
            self.slot_amount[self.slot_count] = amt;
            self.slot_count += 1;
        }
        for (k, a) in m::CURVE_ADDRESSES.iter().enumerate() {
            self.curves[k] = [p[a[0]], p[a[1]], p[a[2]], p[a[3]]];
        }
    }

    #[inline(always)]
    fn accumulate(&self, d: &mut [f32; DEST_STRIDE], lfo2: f32, lfo3: f32, velocity: f32, note: i32, scale: f32) {
        for s in 0..self.slot_count {
            let src = match self.slot_source[s] {
                1 => lfo2,
                2 => lfo3,
                5 => m::curve(velocity, self.curves[0]),
                6 => m::curve(self.aftertouch, self.curves[1]),
                7 => m::curve(note as f32 / 127.0, self.curves[2]),
                8 => m::curve(self.mod_wheel, self.curves[3]),
                9 => self.slide,
                _ => 0.0,
            };
            let di = self.slot_dest[s];
            if di < DEST_STRIDE {
                d[di] += self.slot_amount[s] * src * scale;
            }
        }
    }
}

/// Advances an envelope by `samples` oversampled steps at once (exact for the RC segments).
#[inline(always)]
fn step_envelope(e: &mut Envelope, a: f64, d: f64, s: f64, r: f64, samples: usize, sr: f64) -> f64 {
    let n = samples as f64;
    match e.stage {
        Stage::Idle => return 0.0,
        Stage::Attack => {
            let tau = a.max(0.05) * vm::ATTACK_TAU_FACTOR * 0.001 * sr;
            let target = vm::ATTACK_TARGET;
            let lv = target + (e.level as f64 - target) * (-n / tau).exp();
            if lv >= 1.0 {
                e.level = 1.0;
                e.stage = Stage::Decay;
            } else {
                e.level = lv as f32;
            }
        }
        Stage::Decay => {
            let tau = d.max(0.05) / vm::DECAY_TAU_DIVISOR * 0.001 * sr;
            e.level = (s + (e.level as f64 - s) * (-n / tau).exp()) as f32;
        }
        Stage::Release => {
            let tau = r.max(0.05) / vm::RELEASE_TAU_DIVISOR * 0.001 * sr;
            e.level = (e.level as f64 * (-n / tau).exp()) as f32;
            if e.level < 1e-5 {
                e.level = 0.0;
                e.stage = Stage::Idle;
            }
        }
    }
    e.level as f64
}
