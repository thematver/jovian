//! Jup-8 V modulation extras: the modulation matrix, keyboard response curves and the two
//! "hybrid" LFOs (LFO 2/3).

use super::primitives::Noise;
use crate::params::{addr, PARAMS};

/// Destinations of the Jup-8 V modulation matrix (`ModDestV1` list order, None excluded).
pub mod dest {
    pub const VCO1_COARSE: usize = 0;
    pub const VCO2_COARSE: usize = 1;
    pub const VCO12_COARSE: usize = 2;
    pub const VCO2_FINE: usize = 3;
    pub const VCO_LFO: usize = 4;
    pub const VCO_ENV: usize = 5;
    pub const VCO1_PW: usize = 6;
    pub const VCO2_PW: usize = 7;
    pub const PW_AMOUNT: usize = 8;
    pub const CROSS_MOD: usize = 9;
    pub const MIX: usize = 10;
    pub const HPF: usize = 11;
    pub const CUTOFF: usize = 12;
    pub const RESO: usize = 13;
    pub const VCF_ENV: usize = 14;
    pub const VCF_LFO: usize = 15;
    pub const VCF_KEY: usize = 16;
    pub const VCA_LEVEL: usize = 17;
    pub const VCA_LFO: usize = 18;
    pub const E1A: usize = 19;
    pub const E1D: usize = 20;
    pub const E1S: usize = 21;
    pub const E1R: usize = 22;
    pub const E2A: usize = 23;
    pub const E2D: usize = 24;
    pub const E2S: usize = 25;
    pub const E2R: usize = 26;
    pub const DETUNE: usize = 27;
    pub const PAN: usize = 28;
    pub const PORTA: usize = 29;
    pub const LFO1_RATE: usize = 30;
    pub const LFO1_DELAY: usize = 31;
    pub const LFO2_RATE: usize = 32;
    pub const LFO2_PHASE: usize = 33;
    pub const LFO2_WARP: usize = 34;
    pub const LFO2_FADE: usize = 35;
    pub const LFO3_RATE: usize = 36;
    pub const LFO3_PHASE: usize = 37;
    pub const LFO3_WARP: usize = 38;
    pub const LFO3_FADE: usize = 39;
    pub const ARP_RATE: usize = 40;
    pub const FX1_MIX: usize = 47;
    pub const FX2_MIX: usize = 48;
    pub const FX3_MIX: usize = 49;
    pub const FX1_PARAM: usize = 50;
    pub const FX2_PARAM: usize = 51;
    pub const FX3_PARAM: usize = 52;
    pub const COUNT: usize = 53;
}

/// One fixed source/destination slot of the matrix.
#[derive(Debug, Clone, Copy)]
pub struct Slot {
    /// 1 LFO2, 2 LFO3, 3 Mod Mixer, 4 Seq Mod, 5 Velocity, 6 Aftertouch, 7 Keyboard, 8 Mod Wheel, 9 MPE Slide
    pub source: u8,
    pub dest_address: usize,
    pub amount_address: usize,
}

/// Source number and destination slot numbers, in Jup-8 V's layout.
pub const SLOT_LAYOUT: [(u8, &[u8]); 9] = [
    (1, &[1, 2]),
    (2, &[3, 4]),
    (3, &[5, 6, 7]),
    (4, &[8, 9, 10]),
    (5, &[11, 12, 13]),
    (6, &[14, 15, 16]),
    (7, &[17, 18, 19]),
    (8, &[20, 21, 22]),
    (9, &[23, 24]),
];

pub fn slots() -> &'static [Slot; 24] {
    use std::sync::OnceLock;
    static SLOTS: OnceLock<[Slot; 24]> = OnceLock::new();
    SLOTS.get_or_init(|| {
        let mut out = [Slot { source: 0, dest_address: 0, amount_address: 0 }; 24];
        let mut n = 0;
        for (src, dests) in SLOT_LAYOUT {
            for &d in dests {
                out[n] = Slot {
                    source: src,
                    dest_address: crate::params::address(&format!("ModMatrixDest{d}")).unwrap(),
                    amount_address: crate::params::address(&format!("ModMatrixAmount_Src{src}_Dest{d}")).unwrap(),
                };
                n += 1;
            }
        }
        out
    })
}

/// Keyboard1 = velocity, 2 = aftertouch, 3 = key tracking, 4 = mod wheel; each is (Y0, X1, Y1, Y2).
pub const CURVE_ADDRESSES: [[usize; 4]; 4] = [
    [addr::Keyboard1_Y0, addr::Keyboard1_X1, addr::Keyboard1_Y1, addr::Keyboard1_Y2],
    [addr::Keyboard2_Y0, addr::Keyboard2_X1, addr::Keyboard2_Y1, addr::Keyboard2_Y2],
    [addr::Keyboard3_Y0, addr::Keyboard3_X1, addr::Keyboard3_Y1, addr::Keyboard3_Y2],
    [addr::Keyboard4_Y0, addr::Keyboard4_X1, addr::Keyboard4_Y1, addr::Keyboard4_Y2],
];

/// Two-segment response curve through (0, y0), (x1, y1), (1, y2).
#[inline(always)]
pub fn curve(x: f32, c: [f32; 4]) -> f32 {
    let [y0, x1, y1, y2] = c;
    let x = x.clamp(0.0, 1.0);
    if x <= x1 {
        return if x1 <= 0.0 { y1 } else { y0 + (y1 - y0) * x / x1 };
    }
    if x1 >= 1.0 {
        y1
    } else {
        y1 + (y2 - y1) * (x - x1) / (1.0 - x1)
    }
}

/// Bar-based rate lists of Jup-8 V (bars per cycle / per step).
pub mod rate_lists {
    pub const LFO1_BARS: [f64; 20] = [
        8.0, 6.0, 4.0, 3.0, 2.0, 1.5, 1.0, 0.75, 0.5, 0.375, 1.0 / 3.0, 5.0 / 16.0, 0.25, 3.0 / 16.0, 1.0 / 6.0, 0.125,
        1.0 / 12.0, 1.0 / 16.0, 1.0 / 24.0, 1.0 / 32.0,
    ];
    pub const HYBRID_BARS: [f64; 9] = [8.0, 4.0, 2.0, 1.0, 0.5, 0.25, 0.125, 1.0 / 16.0, 1.0 / 32.0];
    pub const ARP_BARS: [f64; 16] = [
        1.0, 0.75, 0.5, 0.375, 1.0 / 3.0, 5.0 / 16.0, 0.25, 3.0 / 16.0, 1.0 / 6.0, 0.125, 1.0 / 12.0, 1.0 / 16.0, 1.0 / 24.0,
        1.0 / 32.0, 1.0 / 48.0, 1.0 / 64.0,
    ];

    #[inline(always)]
    pub fn hz(bars: f64, tempo: f64) -> f64 {
        tempo / 240.0 / bars.max(1e-6)
    }
}

/// Addresses of a hybrid LFO (Jup-8 V calls them "LFO 2/3", stored as LFO1_* / LFO2_*).
#[derive(Debug, Clone, Copy)]
pub struct HybridLfoAddresses {
    pub wave: usize,
    pub rate_mode: usize,
    pub rate: usize,
    pub rate_sync: usize,
    pub fade: usize,
    pub warp: usize,
    pub phase: usize,
    pub looped: usize,
    pub retrig: usize,
    pub unipolar: usize,
    pub poly: usize,
}

pub const LFO2_ADDR: HybridLfoAddresses = HybridLfoAddresses {
    wave: addr::LFO1_Waveform,
    rate_mode: addr::LFO1_RateMode,
    rate: addr::LFO1_RateUnSynced,
    rate_sync: addr::LFO1_RateSynced,
    fade: addr::LFO1_Fade,
    warp: addr::LFO1_PhaseDist,
    phase: addr::LFO1_PhaseInit,
    looped: addr::LFO1_LoopMode,
    retrig: addr::LFO1_Retrig,
    unipolar: addr::LFO1_Unipolar,
    poly: addr::LFO1_Poly,
};

pub const LFO3_ADDR: HybridLfoAddresses = HybridLfoAddresses {
    wave: addr::LFO2_Waveform,
    rate_mode: addr::LFO2_RateMode,
    rate: addr::LFO2_RateUnSynced,
    rate_sync: addr::LFO2_RateSynced,
    fade: addr::LFO2_Fade,
    warp: addr::LFO2_PhaseDist,
    phase: addr::LFO2_PhaseInit,
    looped: addr::LFO2_LoopMode,
    retrig: addr::LFO2_Retrig,
    unipolar: addr::LFO2_Unipolar,
    poly: addr::LFO2_Poly,
};

/// Settings of one hybrid LFO for the current control block.
#[derive(Debug, Clone, Copy)]
pub struct HybridLfoSettings {
    pub wave: usize,
    pub hz: f64,
    pub fade_sec: f64,
    pub warp: f64,
    pub phase0: f64,
    pub single: bool,
    pub retrig: bool,
    pub unipolar: bool,
    pub poly: bool,
}

impl Default for HybridLfoSettings {
    fn default() -> Self {
        HybridLfoSettings {
            wave: 0,
            hz: 1.0,
            fade_sec: 0.0,
            warp: 0.0,
            phase0: 0.0,
            single: false,
            retrig: false,
            unipolar: false,
            poly: true,
        }
    }
}

impl HybridLfoSettings {
    pub fn update(&mut self, p: &[f32], a: &HybridLfoAddresses, tempo: f64) {
        let f = PARAMS;
        self.wave = f[a.wave].index_f32(p[a.wave]);
        let mode = f[a.rate_mode].index_f32(p[a.rate_mode]);
        if mode == 0 {
            self.hz = f[a.rate].physical_f32(p[a.rate]);
        } else {
            let bars = rate_lists::HYBRID_BARS[f[a.rate_sync].index_f32(p[a.rate_sync])]
                * if mode == 2 {
                    2.0 / 3.0
                } else if mode == 3 {
                    1.5
                } else {
                    1.0
                };
            self.hz = rate_lists::hz(bars, tempo);
        }
        self.fade_sec = f[a.fade].physical_f32(p[a.fade]);
        self.warp = f[a.warp].physical_f32(p[a.warp]);
        self.phase0 = f[a.phase].physical_f32(p[a.phase]) / 360.0;
        self.single = f[a.looped].index_f32(p[a.looped]) == 0;
        self.retrig = p[a.retrig] >= 0.5;
        self.unipolar = p[a.unipolar] >= 0.5;
        self.poly = p[a.poly] >= 0.5;
    }
}

/// Running state of one hybrid LFO instance (global or per voice).
#[derive(Debug, Clone, Copy)]
pub struct HybridLfoState {
    pub phase: f64,
    pub fade: f32,
    pub sh: f32,
    pub done: bool,
    pub value: f32,
    pub noise: Noise,
}

impl Default for HybridLfoState {
    fn default() -> Self {
        HybridLfoState { phase: 0.0, fade: 1.0, sh: 0.0, done: false, value: 0.0, noise: Noise::default() }
    }
}

impl HybridLfoState {
    pub fn trigger(&mut self, s: &HybridLfoSettings) {
        if s.retrig || s.single {
            self.phase = s.phase0;
            self.done = false;
        }
        self.fade = if s.fade_sec > 0.0 { 0.0 } else { 1.0 };
    }

    #[inline(always)]
    pub fn advance(&mut self, s: &HybridLfoSettings, dt: f64, rate_mul: f64, warp_offset: f64) -> f32 {
        if !self.done {
            self.phase += s.hz * rate_mul * dt;
            if self.phase >= 1.0 {
                if s.single {
                    self.phase = 1.0;
                    self.done = true;
                } else {
                    self.phase -= self.phase.floor();
                    self.sh = self.noise.next();
                }
            }
        }
        if self.fade < 1.0 {
            self.fade = 1f32.min(self.fade + (dt / s.fade_sec.max(1e-4)) as f32);
        }
        // phase distortion ("warp"): bends the phase towards the start or the end of the cycle
        let w = (s.warp + warp_offset).clamp(-1.0, 1.0);
        let mut ph = self.phase.min(0.999_999);
        if w > 0.0 {
            ph = ph.powf(1.0 + 3.0 * w);
        } else if w < 0.0 {
            ph = 1.0 - (1.0 - ph).powf(1.0 - 3.0 * w);
        }
        let mut v = match s.wave {
            0 => (2.0 * std::f64::consts::PI * ph).sin(),
            1 => {
                if ph < 0.25 {
                    4.0 * ph
                } else if ph < 0.75 {
                    2.0 - 4.0 * ph
                } else {
                    4.0 * ph - 4.0
                }
            }
            2 => 2.0 * ph - 1.0,
            3 => 1.0 - 2.0 * ph,
            4 => {
                if ph < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            _ => self.sh as f64,
        };
        if s.unipolar {
            v = 0.5 * v + 0.5;
        }
        self.value = v as f32 * self.fade;
        self.value
    }
}
