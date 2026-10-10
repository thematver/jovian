//! The plugin's patch: modelled parameter values plus everything else an imported Jup-8 V preset
//! carried (sequencer, mod mixer, macros, MIDI maps, ...), kept untouched so that exporting gives
//! the original back.

use super::arturia::{ArturiaPreset, DEFAULT_VERSION};
use crate::params::{self, Values, PARAMS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchState {
    pub name: String,
    pub pack: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
    /// Normalized values of the modelled parameters as last loaded (exact preset values).
    #[serde(default)]
    pub values: BTreeMap<String, f64>,
    /// Jup-8 V parameters this engine does not model.
    #[serde(default)]
    pub extras: BTreeMap<String, f64>,
    #[serde(default, with = "base64_map")]
    pub blobs: BTreeMap<String, Vec<u8>>,
}

fn default_version() -> String {
    DEFAULT_VERSION.to_string()
}

impl Default for PatchState {
    fn default() -> Self {
        PatchState::new("Init", "User")
    }
}

impl PatchState {
    pub fn new(name: &str, pack: &str) -> Self {
        PatchState {
            name: name.to_string(),
            pack: pack.to_string(),
            author: String::new(),
            comment: String::new(),
            version: DEFAULT_VERSION.to_string(),
            meta: BTreeMap::new(),
            values: PARAMS.iter().map(|p| (p.id.to_string(), p.default)).collect(),
            extras: BTreeMap::new(),
            blobs: BTreeMap::new(),
        }
    }

    /// Imports a Jup-8 V preset.
    pub fn from_preset(p: &ArturiaPreset) -> Self {
        let mut s = PatchState::new(&p.name, &p.pack);
        s.author = p.author.clone();
        s.comment = p.comment.clone();
        s.meta = p.meta.clone();
        s.blobs = p.blobs.clone();
        let tmpl = template();
        for spec in PARAMS {
            let v = p
                .normalized_for_current_version(spec.id)
                .or_else(|| tmpl.get(spec.id).copied())
                .unwrap_or(spec.default);
            s.values.insert(spec.id.to_string(), v);
        }
        s.extras = p.params.iter().filter(|(k, _)| params::address(k).is_none()).map(|(k, v)| (k.clone(), *v)).collect();
        // keep the original version only for what we pass through; modelled values are converted
        s.version = DEFAULT_VERSION.to_string();
        s
    }

    /// Normalized value of a modelled parameter.
    pub fn value(&self, id: &str) -> f64 {
        self.values.get(id).copied().unwrap_or_else(|| params::spec(id).default)
    }

    /// The live values for the engine.
    pub fn to_values(&self) -> Values {
        let mut v = params::defaults();
        for (i, p) in PARAMS.iter().enumerate() {
            if let Some(x) = self.values.get(p.id) {
                v[i] = *x as f32;
            }
        }
        v
    }

    /// Copies live values back into the patch. Values that still denote the same setting as the
    /// stored ones (same item of a discrete parameter, same `f32` of a continuous one) keep their
    /// exact stored value, so a loaded preset is written back unchanged.
    pub fn capture(&mut self, live: &Values) {
        for (i, p) in PARAMS.iter().enumerate() {
            let x = live[i];
            let keep = match self.values.get(p.id) {
                Some(old) if p.is_discrete() => p.index_f32(*old as f32) == p.index_f32(x),
                Some(old) => *old as f32 == x,
                None => false,
            };
            if !keep {
                self.values.insert(p.id.to_string(), x as f64);
            }
        }
    }

    /// Builds a complete Jup-8 V preset: neutral template < carried-over extras < modelled values.
    pub fn arturia_preset(&self) -> ArturiaPreset {
        let mut params: BTreeMap<String, f64> = template().clone();
        for (k, v) in &self.extras {
            params.insert(k.clone(), *v);
        }
        for (k, v) in &self.values {
            if params::address(k).is_some() {
                params.insert(k.clone(), v.clamp(0.0, 1.0));
            }
        }
        let mut m = self.meta.clone();
        m.entry("OriginalPresetName".into()).or_insert_with(|| self.name.clone());
        m.entry("OriginalPackName".into()).or_insert_with(|| self.pack.clone());
        m.entry("Type".into()).or_insert_with(|| "Custom".into());
        // exports land in Jup-8 V's user area; don't pose as one of its factory banks
        let bank = if ["Factory", "Vintage Factory", ""].contains(&self.pack.as_str()) { "Jovian-8" } else { &self.pack };
        let mut p = ArturiaPreset::new(&self.name, bank);
        p.author = self.author.clone();
        p.comment = self.comment.clone();
        p.params = params;
        p.meta = m;
        p.blobs = self.blobs.clone();
        p.version = self.version.clone();
        p
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

/// The full Jup-8 V parameter set with neutral values (base for written presets).
pub fn template() -> &'static BTreeMap<String, f64> {
    use std::sync::OnceLock;
    static T: OnceLock<BTreeMap<String, f64>> = OnceLock::new();
    T.get_or_init(|| {
        include_str!("arturia_template.tsv")
            .lines()
            .filter_map(|l| {
                let (k, v) = l.split_once('\t')?;
                Some((k.to_string(), v.parse().ok()?))
            })
            .collect()
    })
}

mod base64_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(d: &[u8]) -> String {
        let mut out = String::with_capacity(d.len().div_ceil(3) * 4);
        for c in d.chunks(3) {
            let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
            let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
            for k in 0..4 {
                if k <= c.len() {
                    out.push(ALPHABET[(n >> (18 - 6 * k) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(s: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(s.len() / 4 * 3);
        let mut acc = 0u32;
        let mut bits = 0;
        for ch in s.bytes() {
            if ch == b'=' {
                break;
            }
            let v = ALPHABET.iter().position(|&a| a == ch)? as u32;
            acc = acc << 6 | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits & 0xFF) as u8);
            }
        }
        Some(out)
    }

    pub fn serialize<S: Serializer>(m: &BTreeMap<String, Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        let enc: BTreeMap<&String, String> = m.iter().map(|(k, v)| (k, encode(v))).collect();
        enc.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Vec<u8>>, D::Error> {
        let enc = BTreeMap::<String, String>::deserialize(d)?;
        Ok(enc.into_iter().filter_map(|(k, v)| Some((k, decode(&v)?))).collect())
    }

    #[test]
    fn round_trip() {
        for len in 0..10 {
            let d: Vec<u8> = (0..len).map(|i| (i * 37 + 5) as u8).collect();
            assert_eq!(decode(&encode(&d)).unwrap(), d);
        }
        assert_eq!(encode(b"Man"), "TWFu");
        assert_eq!(encode(b"Ma"), "TWE=");
    }
}
