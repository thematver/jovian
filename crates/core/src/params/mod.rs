//! The parameter table. Every parameter is stored normalized (0...1) exactly as in a Jup-8 V
//! preset; `id` is the Jup-8 V preset key, so values map 1:1 between the formats.

mod table;

pub use table::{addr, PARAMS};

use crate::dsp::voice_math;

/// Number of plugin parameters (addresses are `0..COUNT`).
pub const COUNT: usize = 419;

/// Response curve from a normalized 0...1 value to the physical range, matching the mapping
/// names used by Jup-8 V (`X(n)`, `Exp(k)`, `BipolarExp(k)`, `reverse`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    Linear,
    /// `X(n)`: x^n
    Power(f64),
    /// `Exp(k)`: (e^(kx) - 1) / (e^k - 1)
    Exp(f64),
    /// `BipolarExp(k)`: two mirrored `Exp(|k|)` halves around 0.5. Negative k is steepest in the
    /// middle, positive k is flattest in the middle (fine control around zero).
    BipolarExp(f64),
    /// `reverse`: linear, from max down to min.
    Reverse,
}

#[inline(always)]
fn exp_shape(u: f64, k: f64) -> f64 {
    if k.abs() < 1e-9 {
        u
    } else {
        ((k * u).exp() - 1.0) / (k.exp() - 1.0)
    }
}

#[inline(always)]
fn exp_shape_inverse(y: f64, k: f64) -> f64 {
    if k.abs() < 1e-9 {
        y
    } else {
        (1.0 + y * (k.exp() - 1.0)).ln() / k
    }
}

impl Curve {
    /// Shapes a normalized value; the result is still 0...1.
    #[inline(always)]
    pub fn shape(self, x: f64) -> f64 {
        match self {
            Curve::Linear | Curve::Reverse => x,
            Curve::Power(n) => x.max(0.0).powf(n),
            Curve::Exp(k) => exp_shape(x, k),
            Curve::BipolarExp(k) => {
                let d = 2.0 * x - 1.0;
                let s = if d < 0.0 { -1.0 } else { 1.0 };
                if k >= 0.0 {
                    0.5 + s * 0.5 * exp_shape(d.abs(), k)
                } else {
                    0.5 + s * 0.5 * (1.0 - exp_shape(1.0 - d.abs(), -k))
                }
            }
        }
    }

    pub fn unshape(self, y: f64) -> f64 {
        let y = y.clamp(0.0, 1.0);
        match self {
            Curve::Linear | Curve::Reverse => y,
            Curve::Power(n) => y.powf(1.0 / n),
            Curve::Exp(k) => exp_shape_inverse(y, k),
            Curve::BipolarExp(k) => {
                let d = 2.0 * y - 1.0;
                let s = if d < 0.0 { -1.0 } else { 1.0 };
                if k >= 0.0 {
                    0.5 + s * 0.5 * exp_shape_inverse(d.abs(), k)
                } else {
                    0.5 + s * 0.5 * (1.0 - exp_shape_inverse(1.0 - d.abs(), -k))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    None,
    Hz,
    Ms,
    Seconds,
    Semitones,
    Percent,
    Decibels,
    Degrees,
    Bits,
    Ratio,
    Bpm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Lfo,
    Vco,
    VcoMod,
    Vcf,
    Vca,
    Env1,
    Env2,
    Master,
    Voice,
    Dispersion,
    Arp,
    Bender,
    ModWheel,
    Lfo2,
    Lfo3,
    Matrix,
    Keyboard,
    Fx,
    Fx1,
    Fx2,
    Fx3,
}

impl Group {
    pub const ALL: [Group; 21] = [
        Group::Lfo,
        Group::Vco,
        Group::VcoMod,
        Group::Vcf,
        Group::Vca,
        Group::Env1,
        Group::Env2,
        Group::Master,
        Group::Voice,
        Group::Dispersion,
        Group::Arp,
        Group::Bender,
        Group::ModWheel,
        Group::Lfo2,
        Group::Lfo3,
        Group::Matrix,
        Group::Keyboard,
        Group::Fx,
        Group::Fx1,
        Group::Fx2,
        Group::Fx3,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::Lfo => "LFO",
            Group::Vco => "VCO",
            Group::VcoMod => "VCO Modulator",
            Group::Vcf => "VCF",
            Group::Vca => "VCA",
            Group::Env1 => "Envelope 1",
            Group::Env2 => "Envelope 2",
            Group::Master => "Master",
            Group::Voice => "Voice",
            Group::Dispersion => "Dispersion",
            Group::Arp => "Arpeggio",
            Group::Bender => "Bender",
            Group::ModWheel => "Mod Wheel",
            Group::Lfo2 => "LFO 2",
            Group::Lfo3 => "LFO 3",
            Group::Matrix => "Mod Matrix",
            Group::Keyboard => "Keyboard Curves",
            Group::Fx => "Effects",
            Group::Fx1 => "FX 1",
            Group::Fx2 => "FX 2",
            Group::Fx3 => "FX 3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Continuous { curve: Curve, min: f64, max: f64, unit: Unit },
    Discrete(&'static [&'static str]),
}

/// One plugin parameter. Discrete values are stored as `index / (count - 1)` and read back as
/// `min(floor(x * count), count - 1)`, like Jup-8 V does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub default: f64,
    pub kind: Kind,
}

impl ParamSpec {
    pub fn is_discrete(&self) -> bool {
        matches!(self.kind, Kind::Discrete(_))
    }

    pub fn items(&self) -> &'static [&'static str] {
        match self.kind {
            Kind::Discrete(items) => items,
            Kind::Continuous { .. } => &[],
        }
    }

    pub fn item_count(&self) -> usize {
        self.items().len()
    }

    /// Physical value of a normalized continuous parameter (or the index of a discrete one).
    #[inline(always)]
    pub fn physical(&self, x: f64) -> f64 {
        match self.kind {
            Kind::Continuous { curve, min, max, .. } => {
                if let Curve::Reverse = curve {
                    max - (max - min) * x
                } else {
                    min + (max - min) * curve.shape(x)
                }
            }
            Kind::Discrete(items) => index(x, items.len()) as f64,
        }
    }

    /// Physical value of a normalized `f32` as stored in the live parameter array.
    #[inline(always)]
    pub fn physical_f32(&self, x: f32) -> f64 {
        self.physical(x as f64)
    }

    /// Index of a discrete parameter for a normalized `f32`.
    #[inline(always)]
    pub fn index_f32(&self, x: f32) -> usize {
        index(x as f64, self.item_count())
    }

    pub fn normalized_from_physical(&self, v: f64) -> f64 {
        match self.kind {
            Kind::Continuous { curve, min, max, .. } => {
                if max == min {
                    return 0.0;
                }
                if let Curve::Reverse = curve {
                    ((max - v) / (max - min)).clamp(0.0, 1.0)
                } else {
                    curve.unshape((v - min) / (max - min))
                }
            }
            Kind::Discrete(items) => normalized_index(v.round() as i64, items.len()),
        }
    }

    pub fn display_string(&self, x: f64) -> String {
        if let Some(s) = special_display(self.id, x) {
            return s;
        }
        match self.kind {
            Kind::Discrete(items) => items[index(x, items.len())].to_string(),
            Kind::Continuous { min, max, unit, .. } => format_value(self.physical(x), unit, min == 0.0 && max == 1.0),
        }
    }

    /// Parses a display string (with or without unit) back to a normalized value.
    pub fn parse_display(&self, s: &str) -> Option<f64> {
        let t = s.trim();
        match self.kind {
            Kind::Discrete(items) => items
                .iter()
                .position(|i| i.eq_ignore_ascii_case(t))
                .map(|i| normalized_index(i as i64, items.len())),
            Kind::Continuous { min, max, unit, .. } => {
                let lower = t.to_ascii_lowercase();
                let num: String = lower
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
                    .collect();
                let mut v: f64 = num.parse().ok()?;
                if special_display(self.id, 0.5).is_some() && self.id != "LpfCut" {
                    return Some((v / 10.0).clamp(0.0, 1.0));
                }
                if self.id == "LpfCut" {
                    if lower.contains("khz") {
                        v *= 1000.0;
                    }
                    let x = ((v - voice_math::CUTOFF_OFFSET_HZ).max(1.0) / voice_math::CUTOFF_BASE_HZ).log2()
                        / voice_math::CUTOFF_OCTAVES;
                    return Some(x.clamp(0.0, 1.0));
                }
                match unit {
                    Unit::Hz if lower.contains("khz") => v *= 1000.0,
                    Unit::Ms if lower.ends_with(" s") || (lower.ends_with('s') && !lower.ends_with("ms")) => v *= 1000.0,
                    Unit::Seconds if lower.contains("ms") => v /= 1000.0,
                    Unit::None if min == 0.0 && max == 1.0 => v /= 10.0,
                    _ => {}
                }
                Some(self.normalized_from_physical(v).clamp(0.0, 1.0))
            }
        }
    }
}

/// Index of a discrete value: `min(floor(x * count), count - 1)`.
#[inline(always)]
pub fn index(x: f64, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let i = (x * count as f64).floor();
    if i <= 0.0 {
        0
    } else {
        (i as usize).min(count - 1)
    }
}

/// Normalized value of item `i` of `count`.
#[inline(always)]
pub fn normalized_index(i: i64, count: usize) -> f64 {
    if count <= 1 {
        return 0.0;
    }
    i.clamp(0, count as i64 - 1) as f64 / (count - 1) as f64
}

pub fn format_value(v: f64, unit: Unit, unitless01: bool) -> String {
    match unit {
        Unit::Hz => {
            if v >= 1000.0 {
                format!("{:.2} kHz", v / 1000.0)
            } else if v < 10.0 {
                format!("{:.2} Hz", v)
            } else {
                format!("{:.1} Hz", v)
            }
        }
        Unit::Ms => {
            if v >= 1000.0 {
                format!("{:.2} s", v / 1000.0)
            } else if v < 10.0 {
                format!("{:.2} ms", v)
            } else {
                format!("{:.0} ms", v)
            }
        }
        Unit::Seconds => {
            if v >= 1.0 {
                format!("{:.2} s", v)
            } else {
                format!("{:.0} ms", v * 1000.0)
            }
        }
        Unit::Semitones => format!("{:.2} st", v),
        Unit::Percent => format!("{:.0} %", v),
        Unit::Decibels => format!("{:.1} dB", v),
        Unit::Degrees => format!("{:.0}°", v),
        Unit::Bits => format!("{:.1} bits", v),
        Unit::Ratio => format!("{:.1}x", v),
        Unit::Bpm => format!("{:.0} BPM", v),
        Unit::None => {
            if unitless01 {
                format!("{:.1}", v * 10.0)
            } else {
                format!("{:.2}", v)
            }
        }
    }
}

fn special_display(id: &str, x: f64) -> Option<String> {
    match id {
        "LpfCut" => Some(format_value(voice_math::cutoff_hz(x), Unit::Hz, false)),
        "VCA_EnvAmount" | "LpfRes" | "OscCrossMod" | "OscMix" | "LpfEnvAmount" | "LfoFiltAmount"
        | "LPF_KeyFollowAmount" | "VCA_LfoAmount" | "PanSpread" | "BendVCFAmount" | "LfoModVCOAmount"
        | "LfoModVcfAmount" => Some(format!("{:.1}", x * 10.0)),
        _ => None,
    }
}

/// Address of a parameter by its Jup-8 V id.
pub fn address(id: &str) -> Option<usize> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static MAP: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    MAP.get_or_init(|| PARAMS.iter().enumerate().map(|(i, p)| (p.id, i)).collect())
        .get(id)
        .copied()
}

/// Spec of a parameter by its Jup-8 V id (panics for unknown ids: they are compile-time constants).
pub fn spec(id: &str) -> &'static ParamSpec {
    &PARAMS[address(id).unwrap_or_else(|| panic!("unknown parameter {id}"))]
}

/// The live, normalized parameter values read by the engine.
pub type Values = [f32; COUNT];

/// Default values of every parameter.
pub fn defaults() -> Values {
    let mut v = [0.0f32; COUNT];
    for (i, p) in PARAMS.iter().enumerate() {
        v[i] = p.default as f32;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_consistent() {
        assert_eq!(PARAMS.len(), COUNT);
        for (i, p) in PARAMS.iter().enumerate() {
            assert_eq!(address(p.id), Some(i));
            assert!((0.0..=1.0).contains(&p.default), "{}", p.id);
        }
        assert_eq!(addr::LFO_RateUnSynced, 0);
        assert_eq!(PARAMS[addr::FX3_TS808_Tone].id, "FX3_TS808_Tone");
    }

    #[test]
    fn curves_round_trip() {
        for c in [Curve::Linear, Curve::Power(4.3), Curve::Exp(5.9), Curve::Exp(-3.0), Curve::BipolarExp(-1.3), Curve::BipolarExp(1.2)] {
            for i in 0..=100 {
                let x = i as f64 / 100.0;
                assert!((c.unshape(c.shape(x)) - x).abs() < 1e-9, "{c:?} {x}");
            }
        }
    }

    #[test]
    fn display_strings() {
        let s = spec("Env1_Decay");
        assert_eq!(s.display_string(0.0), "1.50 ms");
        assert_eq!(s.display_string(1.0), "46.50 s");
        assert_eq!(spec("Volume").display_string(1.0), "0.0 dB");
        assert_eq!(spec("LpfRes").display_string(0.5), "5.0");
        assert_eq!(spec("Osc1Wave").display_string(0.34), "Saw");
        let a = spec("Env2_Attack");
        let x = a.parse_display("250 ms").unwrap();
        assert!((a.physical(x) - 250.0).abs() < 1e-6);
        let c = spec("LpfCut");
        let x = c.parse_display("1.00 kHz").unwrap();
        assert!((voice_math::cutoff_hz(x) - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn discrete_index_round_trip() {
        for p in PARAMS.iter().filter(|p| p.is_discrete()) {
            let n = p.item_count();
            for i in 0..n {
                let x = normalized_index(i as i64, n) as f32;
                assert_eq!(p.index_f32(x), i, "{}", p.id);
            }
        }
    }
}
