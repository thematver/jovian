//! The plugin's parameters: one host parameter per Jup-8 V preset key, in table order.
//! Continuous parameters are exposed normalized (0...1, displayed in physical units), discrete
//! ones as stepped lists, so automation maps 1:1 onto Jup-8 V preset values.

use jovian_core::params::{normalized_index, Kind, ParamSpec, PARAMS};
use jovian_core::preset::PatchState;
use nih_plug::prelude::*;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

pub enum Holder {
    Float(FloatParam),
    Int(IntParam),
}

impl Holder {
    fn new(spec: &'static ParamSpec) -> Self {
        match spec.kind {
            Kind::Continuous { .. } => Holder::Float(
                FloatParam::new(spec.name, spec.default as f32, FloatRange::Linear { min: 0.0, max: 1.0 })
                    .with_value_to_string(Arc::new(move |v| spec.display_string(v as f64)))
                    .with_string_to_value(Arc::new(move |s| spec.parse_display(s).map(|x| x as f32))),
            ),
            Kind::Discrete(items) => {
                let n = items.len() as i32;
                let default = jovian_core::params::index(spec.default, items.len()) as i32;
                Holder::Int(
                    IntParam::new(spec.name, default, IntRange::Linear { min: 0, max: (n - 1).max(1) })
                        .with_value_to_string(Arc::new(move |v| {
                            items.get(v.max(0) as usize).map(|s| s.to_string()).unwrap_or_default()
                        }))
                        .with_string_to_value(Arc::new(move |s| {
                            let t = s.trim();
                            items.iter().position(|i| i.eq_ignore_ascii_case(t)).map(|i| i as i32).or_else(|| t.parse().ok())
                        })),
                )
            }
        }
    }

    /// Normalized value including host modulation, in Jup-8 V storage.
    #[inline]
    pub fn normalized(&self) -> f32 {
        match self {
            Holder::Float(p) => p.modulated_normalized_value(),
            Holder::Int(p) => p.modulated_normalized_value(),
        }
    }

    /// Normalized value as set (without host modulation).
    #[inline]
    pub fn unmodulated(&self) -> f32 {
        match self {
            Holder::Float(p) => p.unmodulated_normalized_value(),
            Holder::Int(p) => p.unmodulated_normalized_value(),
        }
    }

    pub fn ptr(&self) -> ParamPtr {
        match self {
            Holder::Float(p) => p.as_ptr(),
            Holder::Int(p) => p.as_ptr(),
        }
    }

    /// The value in the plugin state format (plain value).
    pub fn state_value(spec: &ParamSpec, normalized: f64) -> nih_plug::wrapper::state::ParamValue {
        use nih_plug::wrapper::state::ParamValue;
        match spec.kind {
            Kind::Continuous { .. } => ParamValue::F32(normalized.clamp(0.0, 1.0) as f32),
            Kind::Discrete(items) => ParamValue::I32(jovian_core::params::index(normalized, items.len()) as i32),
        }
    }
}

/// Editor settings that are saved with the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
    /// Window scale in percent.
    pub zoom: u32,
    /// Selected page.
    pub page: u32,
}

impl Default for UiState {
    fn default() -> Self {
        UiState { zoom: 100, page: 0 }
    }
}

pub struct JovianParams {
    /// Indexed by parameter address.
    pub list: Vec<Holder>,
    /// Name, bank and the pass-through data of the current patch (values live in `list`).
    pub patch: RwLock<PatchState>,
    pub ui: RwLock<UiState>,
}

impl Default for JovianParams {
    fn default() -> Self {
        JovianParams {
            list: PARAMS.iter().map(Holder::new).collect(),
            patch: RwLock::new(PatchState::default()),
            ui: RwLock::new(UiState::default()),
        }
    }
}

impl JovianParams {
    /// Live normalized values (without host modulation), for capturing the patch.
    pub fn values(&self) -> jovian_core::params::Values {
        let mut v = [0.0f32; jovian_core::params::COUNT];
        for (i, h) in self.list.iter().enumerate() {
            v[i] = h.unmodulated();
        }
        v
    }

    /// The current patch with the live values.
    pub fn current_patch(&self) -> PatchState {
        let mut p = self.patch.read().clone();
        p.capture(&self.values());
        p
    }

    /// A plugin state that loads `patch` (all parameters plus the patch fields).
    pub fn state_for(&self, patch: &PatchState) -> nih_plug::wrapper::state::PluginState {
        let mut params = BTreeMap::new();
        for spec in PARAMS {
            params.insert(spec.id.to_string(), Holder::state_value(spec, patch.value(spec.id)));
        }
        let mut fields = BTreeMap::new();
        fields.insert("patch".to_string(), patch.to_json());
        fields.insert("ui".to_string(), serde_json::to_string(&*self.ui.read()).unwrap_or_default());
        nih_plug::wrapper::state::PluginState { version: env!("CARGO_PKG_VERSION").to_string(), params, fields }
    }
}

/// Normalized value of item `i` of a discrete parameter.
pub fn item_normalized(addr: usize, i: usize) -> f32 {
    normalized_index(i as i64, PARAMS[addr].item_count()) as f32
}

unsafe impl Params for JovianParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.list
            .iter()
            .enumerate()
            .map(|(i, h)| (PARAMS[i].id.to_string(), h.ptr(), PARAMS[i].group.title().to_string()))
            .collect()
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert("patch".to_string(), self.current_patch().to_json());
        m.insert("ui".to_string(), serde_json::to_string(&*self.ui.read()).unwrap_or_default());
        m
    }

    fn deserialize_fields(&self, serialized: &BTreeMap<String, String>) {
        if let Some(p) = serialized.get("patch").and_then(|s| PatchState::from_json(s)) {
            *self.patch.write() = p;
        }
        if let Some(u) = serialized.get("ui").and_then(|s| serde_json::from_str::<UiState>(s).ok()) {
            *self.ui.write() = u;
        }
    }
}
