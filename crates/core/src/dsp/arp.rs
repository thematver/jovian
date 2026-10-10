//! Jup-8 V arpeggiator: Up, Down, Ordered, Reversed, Inc (up/down repeating the ends),
//! Exc (up/down without repeating), Random; 1-4 octaves; 50 % gate. Fixed capacity.

use super::primitives::Noise;

pub const CAPACITY: usize = 32;
const SEQ_CAPACITY: usize = CAPACITY * 4 * 2;

#[derive(Debug, Clone, Copy, Default)]
pub struct ArpEvent {
    pub off: Option<i32>,
    pub on: Option<i32>,
    pub velocity: f32,
}

#[derive(Clone)]
pub struct Arpeggiator {
    /// Held notes in press order.
    pub notes: [i32; CAPACITY],
    pub velocities: [f32; CAPACITY],
    pub count: usize,
    sequence: [i32; SEQ_CAPACITY],
    seq_vel: [f32; SEQ_CAPACITY],
    seq_length: usize,
    position: isize,
    phase: f64,
    gate_open: bool,
    pub sounding: Option<i32>,
    rng: Noise,
    dirty: bool,
    built_mode: isize,
    built_octaves: isize,
}

impl Default for Arpeggiator {
    fn default() -> Self {
        Arpeggiator {
            notes: [0; CAPACITY],
            velocities: [0.0; CAPACITY],
            count: 0,
            sequence: [0; SEQ_CAPACITY],
            seq_vel: [0.0; SEQ_CAPACITY],
            seq_length: 0,
            position: -1,
            phase: 1.0,
            gate_open: false,
            sounding: None,
            rng: Noise::new(0xA5A5_1234),
            dirty: true,
            built_mode: -1,
            built_octaves: -1,
        }
    }
}

impl Arpeggiator {
    pub fn reset(&mut self) {
        self.count = 0;
        self.seq_length = 0;
        self.position = -1;
        self.phase = 1.0;
        self.gate_open = false;
        self.sounding = None;
        self.dirty = true;
    }

    pub fn add(&mut self, note: i32, velocity: f32) {
        self.remove(note);
        if self.count >= CAPACITY {
            return;
        }
        self.notes[self.count] = note;
        self.velocities[self.count] = velocity;
        self.count += 1;
        if self.count == 1 {
            // first key starts the pattern immediately
            self.phase = 1.0;
            self.position = -1;
        }
        self.dirty = true;
    }

    pub fn remove(&mut self, note: i32) {
        let mut i = 0;
        while i < self.count {
            if self.notes[i] == note {
                for j in i..self.count - 1 {
                    self.notes[j] = self.notes[j + 1];
                    self.velocities[j] = self.velocities[j + 1];
                }
                self.count -= 1;
                self.dirty = true;
            } else {
                i += 1;
            }
        }
    }

    pub fn keep_only(&mut self, is_down: impl Fn(i32) -> bool) {
        let mut i = 0;
        while i < self.count {
            if !is_down(self.notes[i]) {
                let n = self.notes[i];
                self.remove(n);
            } else {
                i += 1;
            }
        }
    }

    fn rebuild(&mut self, mode: usize, octaves: usize) {
        self.dirty = false;
        self.built_mode = mode as isize;
        self.built_octaves = octaves as isize;
        // base list: sorted ascending, or press order for Ordered/Reversed
        let n = self.count;
        let mut b = [0i32; CAPACITY];
        let mut bv = [0usize; CAPACITY];
        for i in 0..n {
            b[i] = self.notes[i];
            bv[i] = i;
        }
        if mode != 2 && mode != 3 {
            for i in 1..n.max(1) {
                let mut j = i;
                while j > 0 && b[j - 1] > b[j] {
                    b.swap(j - 1, j);
                    bv.swap(j - 1, j);
                    j -= 1;
                }
            }
        }
        // expand over octaves (ascending)
        let mut len = 0;
        for o in 0..octaves {
            for i in 0..n {
                self.sequence[len] = b[i] + 12 * o as i32;
                self.seq_vel[len] = self.velocities[bv[i]];
                len += 1;
            }
        }
        match mode {
            1 | 3 => {
                // Down, Reversed
                if len > 1 {
                    self.sequence[..len].reverse();
                    self.seq_vel[..len].reverse();
                }
            }
            4 | 5 => {
                // Inc: up then down including both ends; Exc: without repeating ends
                let inc = mode == 4;
                let mut k = if inc { len as isize - 1 } else { len as isize - 2 };
                let stop = if inc { 0 } else { 1 };
                let mut out = len;
                while k >= stop && out < SEQ_CAPACITY {
                    self.sequence[out] = self.sequence[k as usize];
                    self.seq_vel[out] = self.seq_vel[k as usize];
                    out += 1;
                    k -= 1;
                }
                len = out;
            }
            _ => {}
        }
        self.seq_length = len;
    }

    /// Advances the clock; returns the note to stop and/or the note to start.
    pub fn advance(&mut self, dt: f64, hz: f64, mode: usize, octaves: usize) -> ArpEvent {
        if self.count == 0 {
            let off = self.sounding;
            self.sounding = None;
            self.gate_open = false;
            self.position = -1;
            self.phase = 1.0;
            return ArpEvent { off, on: None, velocity: 0.0 };
        }
        if self.dirty || mode as isize != self.built_mode || octaves as isize != self.built_octaves {
            self.rebuild(mode, octaves);
        }
        let mut ev = ArpEvent::default();
        self.phase += dt * hz.max(0.01);
        if self.gate_open && self.phase >= 0.5 {
            ev.off = self.sounding;
            self.sounding = None;
            self.gate_open = false;
        }
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            if let Some(s) = self.sounding {
                ev.off = Some(s);
            }
            if self.seq_length == 0 {
                return ev;
            }
            if mode == 6 {
                let r = (self.rng.next().abs() * self.seq_length as f32) as usize;
                self.position = r.min(self.seq_length - 1) as isize;
            } else {
                self.position = (self.position + 1) % self.seq_length as isize;
            }
            let note = self.sequence[self.position as usize].clamp(0, 127);
            ev.on = Some(note);
            ev.velocity = self.seq_vel[self.position as usize];
            self.sounding = Some(note);
            self.gate_open = true;
        }
        ev
    }
}
