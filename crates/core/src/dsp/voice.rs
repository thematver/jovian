//! One synth voice: VCO-1 / VCO-2 (sync, cross-mod) -> mixer -> HPF -> VCF -> VCA.

use super::modulation::HybridLfoState;
use super::primitives::*;
use super::voice_math as vm;

/// Control-rate targets computed by the engine for one voice.
#[derive(Debug, Clone, Copy)]
pub struct VoiceControl {
    pub inc1: f32,
    pub inc2: f32,
    pub g: f32,
    pub k: f32,
    pub hp_g: f32,
    pub duty1: f32,
    pub duty2: f32,
    pub mix1: f32,
    pub mix2: f32,
    pub amp: f32,
    pub pan_l: f32,
    pub pan_r: f32,
}

impl Default for VoiceControl {
    fn default() -> Self {
        VoiceControl {
            inc1: 0.0,
            inc2: 0.0,
            g: 0.5,
            k: 0.0,
            hp_g: 0.0,
            duty1: 0.5,
            duty2: 0.5,
            mix1: 0.707,
            mix2: 0.707,
            amp: 0.0,
            pan_l: 0.707,
            pan_r: 0.707,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Voice {
    // assignment
    pub active: bool,
    pub gate: bool,
    pub sustained: bool,
    pub note: i32,
    pub velocity: f32,
    pub age: u64,
    pub stack_index: usize,
    pub stack_size: usize,
    pub has_played: bool,

    // pitch (semitones, MIDI note scale)
    pub pitch: f64,
    pub target_pitch: f64,

    // oscillators
    pub phase1: f32,
    pub phase2: f32,
    pub xm: f32,
    pub noise: Noise,

    // filters
    pub hpf: OnePoleHp,
    pub ladder: Ladder,

    // envelopes & per-voice modulators
    pub env1: Envelope,
    pub env2: Envelope,
    pub lfo_fade: f32,
    pub lfo2: HybridLfoState,
    pub lfo3: HybridLfoState,

    // fixed per-voice "analog" deviations in [-1, 1] (dispersion)
    pub r_pitch1: f32,
    pub r_pitch2: f32,
    pub r_pw: f32,
    pub r_env: f32,
    pub r_cut: f32,
    pub r_res: f32,
    pub r_mod: f32,
    pub pan_pos: f32,

    // control-rate results, interpolated across the block
    pub c: VoiceControl,
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            active: false,
            gate: false,
            sustained: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            stack_index: 0,
            stack_size: 1,
            has_played: false,
            pitch: 60.0,
            target_pitch: 60.0,
            phase1: 0.0,
            phase2: 0.0,
            xm: 0.0,
            noise: Noise::default(),
            hpf: OnePoleHp::default(),
            ladder: Ladder::default(),
            env1: Envelope::default(),
            env2: Envelope::default(),
            lfo_fade: 1.0,
            lfo2: HybridLfoState::default(),
            lfo3: HybridLfoState::default(),
            r_pitch1: 0.0,
            r_pitch2: 0.0,
            r_pw: 0.0,
            r_env: 0.0,
            r_cut: 0.0,
            r_res: 0.0,
            r_mod: 0.0,
            pan_pos: 0.0,
            c: VoiceControl::default(),
        }
    }
}

impl Voice {
    pub fn randomize(&mut self, seed: u32) {
        let mut n = Noise::new(seed | 1);
        for _ in 0..4 {
            n.next();
        }
        self.r_pitch1 = n.next();
        self.r_pitch2 = n.next();
        self.r_pw = n.next();
        self.r_env = n.next();
        self.r_cut = n.next();
        self.r_res = n.next();
        self.r_mod = n.next();
        self.noise = Noise::new(seed.wrapping_mul(747_796_405) | 1);
        self.lfo2.noise = Noise::new(seed.wrapping_add(12345));
        self.lfo3.noise = Noise::new(seed.wrapping_add(54321));
        self.phase1 = n.next().abs();
        self.phase2 = n.next().abs();
    }

    /// The control values have never been set (the voice has not rendered yet).
    #[inline(always)]
    pub fn needs_control_copy(&self) -> bool {
        self.c.inc1 == 0.0
    }

    /// Jumps to new control values without interpolation (amplitude starts from zero).
    pub fn copy_control(&mut self, c: &VoiceControl) {
        self.c = *c;
        self.c.amp = 0.0;
    }

    /// Renders `out_l.len()` oversampled samples, adding into the buffers. Inc/G/amp/pan move
    /// linearly from the previous control values to the new targets.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn render(
        &mut self,
        out_l: &mut [f32],
        out_r: &mut [f32],
        t: &VoiceControl,
        wave1: usize,
        wave2: usize,
        sync: usize,
        cross_mod: f32,
        two_pole: bool,
    ) {
        let count = out_l.len();
        let inv_n = 1.0 / count as f32;
        let mut c = self.c;
        let d_inc1 = (t.inc1 - c.inc1) * inv_n;
        let d_inc2 = (t.inc2 - c.inc2) * inv_n;
        let d_g = (t.g - c.g) * inv_n;
        let d_amp = (t.amp - c.amp) * inv_n;
        let d_pan_l = (t.pan_l - c.pan_l) * inv_n;
        let d_pan_r = (t.pan_r - c.pan_r) * inv_n;
        c.k = t.k;
        c.hp_g = t.hp_g;
        c.duty1 = t.duty1;
        c.duty2 = t.duty2;
        c.mix1 = t.mix1;
        c.mix2 = t.mix2;
        let k = c.k;
        let comp = 1.0 + 0.11 * k;
        let xm_scale = vm::CROSS_MOD_OCTAVES as f32 * cross_mod;
        let sine_level = (vm::VCO2_SINE_LEVEL / vm::OSC_LEVEL) as f32;
        let noise_level = (vm::NOISE_LEVEL / vm::OSC_LEVEL) as f32;

        for i in 0..count {
            c.inc1 += d_inc1;
            c.inc2 += d_inc2;
            c.g += d_g;
            c.amp += d_amp;
            c.pan_l += d_pan_l;
            c.pan_r += d_pan_r;

            // VCO-2 (master for cross-mod, and for sync when VCO-1 is the slave)
            self.phase2 += c.inc2;
            let mut wrapped2 = false;
            if self.phase2 >= 1.0 {
                self.phase2 -= 1.0;
                wrapped2 = true;
            }

            // VCO-1 with exponential cross modulation from VCO-2
            let mut i1 = c.inc1;
            if xm_scale > 0.0 {
                i1 *= fast_exp2(xm_scale * self.xm);
            }
            i1 = i1.min(0.45);
            self.phase1 += i1;
            let mut wrapped1 = false;
            if self.phase1 >= 1.0 {
                self.phase1 -= 1.0;
                wrapped1 = true;
            }
            if sync == 0 && wrapped2 {
                self.phase1 = (self.phase2 * i1 / c.inc2.max(1e-9)).min(0.999);
            } else if sync == 2 && wrapped1 {
                self.phase2 = (self.phase1 * c.inc2 / i1.max(1e-9)).min(0.999);
            }

            let o1 = match wave1 {
                0 => triangle(self.phase1),
                1 => saw(self.phase1, i1),
                2 => pulse(self.phase1, i1, c.duty1),
                _ => pulse(self.phase1, i1, 0.5),
            };
            let o2 = match wave2 {
                0 => {
                    let s = fast_sin_2pi(self.phase2);
                    self.xm = s * 0.4;
                    s * sine_level
                }
                1 => {
                    let o = saw(self.phase2, c.inc2);
                    self.xm = 0.5 * o + 0.5; // the saw drives cross-mod unipolar, like the pulse
                    o
                }
                2 => {
                    let o = pulse(self.phase2, c.inc2, c.duty2);
                    self.xm = 0.5 * o + 0.5;
                    o
                }
                _ => {
                    let nz = self.noise.next();
                    self.xm = nz * 0.4;
                    nz * noise_level
                }
            };

            let mut s = o1 * c.mix1 + o2 * c.mix2;
            s = self.hpf.process(s, c.hp_g);
            let y = self.ladder.process(s * comp * 0.35, c.g, k, two_pole) * (1.0 / 0.35);
            let out = y * c.amp;
            out_l[i] += out * c.pan_l;
            out_r[i] += out * c.pan_r;
        }
        self.c = c;
    }
}
