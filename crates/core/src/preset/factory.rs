//! Built-in patches, written in physical units so they read like a Jupiter-8 patch sheet.

use super::state::PatchState;
use crate::dsp::voice_math;
use crate::params::{self, normalized_index};

struct Builder {
    state: PatchState,
}

impl Builder {
    fn set(&mut self, id: &str, normalized: f64) {
        self.state.values.insert(id.to_string(), normalized);
    }
    fn physical(&mut self, id: &str, v: f64) {
        let x = params::spec(id).normalized_from_physical(v);
        self.set(id, x);
    }
    fn item(&mut self, id: &str, label: &str) {
        let s = params::spec(id);
        let i = s.items().iter().position(|i| *i == label).unwrap_or_else(|| panic!("{id} has no item {label}"));
        self.set(id, normalized_index(i as i64, s.item_count()));
    }
    fn wave1(&mut self, w: &str) {
        self.item("Osc1Wave", w);
    }
    fn wave2(&mut self, w: &str) {
        self.item("Osc2Wave", w);
    }
    fn range2(&mut self, semis: i32) {
        self.item("Osc2Coarse", &semis.to_string());
    }
    fn fine2(&mut self, st: f64) {
        self.physical("Osc2Finetune", st);
    }
    fn cutoff_hz(&mut self, hz: f64) {
        self.set("LpfCut", voice_math::cutoff_normalized(hz));
    }
    fn env1(&mut self, a: f64, d: f64, s: f64, r: f64) {
        self.physical("Env1_Attack", a.max(0.8));
        self.physical("Env1_Decay", d.max(1.5));
        self.physical("Env1_Sustain", s * 100.0);
        self.physical("Env1_Release", r.max(1.5));
    }
    fn env2(&mut self, a: f64, d: f64, s: f64, r: f64) {
        self.physical("Env2_Attack", a.max(0.8));
        self.physical("Env2_Decay", d.max(1.5));
        self.physical("Env2_Sustain", s * 100.0);
        self.physical("Env2Release", r.max(1.5));
    }
    fn lfo(&mut self, hz: f64, wave: &str, delay: f64) {
        self.physical("LFO_RateUnSynced", hz);
        self.item("LFO_Waveform", wave);
        self.physical("LFO_FadeDelay", delay);
    }
    fn fx(&mut self, slot: usize, kind: &str, mix: f64) {
        self.item(&format!("FX{slot}_ModuleType"), kind);
        self.item(&format!("FX{slot}_Bypass"), "On");
        self.set(&format!("FX{slot}_Dry_Wet"), mix);
    }
}

fn make(name: &str, build: impl FnOnce(&mut Builder)) -> PatchState {
    let mut s = PatchState::new(name, "Jovian-8");
    s.author = "Jovian-8".into();
    let mut b = Builder { state: s };
    b.item("FX_Enable", "On");
    for slot in 1..=3 {
        b.item(&format!("FX{slot}_ModuleType"), "None");
    }
    build(&mut b);
    b.state
}

pub fn all() -> Vec<PatchState> {
    vec![
        make("Init", |_| {}),
        make("Poly Brass", |p| {
            p.wave1("Saw");
            p.wave2("Saw");
            p.range2(0);
            p.fine2(0.08);
            p.set("OscMix", 0.5);
            p.cutoff_hz(260.0);
            p.set("LpfRes", 0.18);
            p.set("LpfEnvAmount", 0.62);
            p.set("LPF_KeyFollowAmount", 0.5);
            p.env1(70.0, 900.0, 0.45, 260.0);
            p.env2(15.0, 1200.0, 0.85, 280.0);
            p.fx(1, "Chorus", 0.35);
            p.fx(2, "Reverb", 0.18);
        }),
        make("String Machine", |p| {
            p.wave1("Saw");
            p.wave2("Saw");
            p.range2(0);
            p.fine2(0.12);
            p.set("OscMix", 0.5);
            p.cutoff_hz(2400.0);
            p.set("LpfEnvAmount", 0.1);
            p.item("LpfSlop", "12db");
            p.env2(420.0, 1500.0, 0.9, 900.0);
            p.env1(400.0, 2000.0, 0.6, 900.0);
            p.lfo(5.2, "Sine", 0.6);
            p.physical("OscLfoAmount", 0.12);
            p.fx(1, "Chorus", 0.6);
            p.fx(2, "Reverb", 0.3);
        }),
        make("Solo Bass", |p| {
            p.item("AssignMode", "Solo");
            p.item("PlPrio", "Last");
            p.item("Osc1Coarse", "16'");
            p.wave1("Square");
            p.wave2("Saw");
            p.range2(-12);
            p.set("OscMix", 0.5);
            p.cutoff_hz(120.0);
            p.set("LpfRes", 0.35);
            p.set("LpfEnvAmount", 0.55);
            p.env1(1.0, 260.0, 0.1, 80.0);
            p.env2(1.0, 600.0, 0.7, 60.0);
        }),
        make("Sync Lead", |p| {
            p.item("AssignMode", "Solo");
            p.item("OscSyncSelect", "VCO 1");
            p.item("OscDestSelect", "VCO 1");
            p.wave1("Saw");
            p.wave2("Rectangle");
            p.range2(0);
            p.set("OscMix", 0.25);
            p.physical("OscEnvAmount", 19.0);
            p.env1(1.0, 700.0, 0.2, 200.0);
            p.cutoff_hz(3000.0);
            p.set("LpfRes", 0.15);
            p.env2(3.0, 800.0, 0.8, 250.0);
            p.item("Portamento_On", "On");
            p.physical("Portamento_Time", 60.0);
            p.lfo(5.5, "Sine", 0.8);
            p.physical("OscLfoAmount", 0.15);
            p.fx(1, "Delay", 0.22);
        }),
        make("PWM Pad", |p| {
            p.wave1("Rectangle");
            p.wave2("Rectangle");
            p.range2(-12);
            p.fine2(0.06);
            p.set("OscMix", 0.45);
            p.item("OscPwmMode", "LFO Mode");
            p.physical("OscPwmAmount", 70.0);
            p.lfo(0.6, "Sine", 0.0);
            p.cutoff_hz(1500.0);
            p.set("LpfRes", 0.1);
            p.env2(700.0, 2000.0, 0.85, 1600.0);
            p.fx(1, "Chorus", 0.4);
            p.fx(2, "Reverb", 0.35);
        }),
        make("Arp Pluck", |p| {
            p.item("ArpSeq_ArpSeqMode", "On");
            p.item("ArpSeq_PatternMode", "Up");
            p.item("ArpSeq_OctaveRepeat", "2");
            p.item("ArpSeq_MidiSync", "Sync");
            p.item("ArpSeq_RateSynced", "1/16 Bar");
            p.wave1("Saw");
            p.wave2("Rectangle");
            p.set("OscMix", 0.4);
            p.cutoff_hz(400.0);
            p.set("LpfRes", 0.3);
            p.set("LpfEnvAmount", 0.5);
            p.env1(1.0, 180.0, 0.0, 150.0);
            p.env2(1.0, 400.0, 0.3, 200.0);
            p.fx(1, "Delay", 0.3);
        }),
        make("Unison Lead", |p| {
            p.item("AssignMode", "Unison");
            p.physical("VoiceDetuneAmount", 0.18);
            p.set("PanSpread", 0.6);
            p.item("Polyphon", "4");
            p.wave1("Saw");
            p.wave2("Saw");
            p.range2(-12);
            p.set("OscMix", 0.5);
            p.cutoff_hz(1800.0);
            p.set("LpfRes", 0.2);
            p.set("LpfEnvAmount", 0.3);
            p.env1(5.0, 500.0, 0.4, 300.0);
            p.env2(5.0, 600.0, 0.9, 300.0);
            p.physical("Volume", -14.0);
        }),
        make("Cross Mod Bell", |p| {
            p.wave1("Triangle");
            p.wave2("Sine");
            p.range2(19);
            p.set("OscCrossMod", 0.35);
            p.set("OscMix", 0.15);
            p.cutoff_hz(5000.0);
            p.env2(1.0, 2500.0, 0.0, 2000.0);
            p.env1(1.0, 1500.0, 0.0, 1500.0);
            p.item("Env2_KeyFollow", "On");
            p.fx(1, "Reverb", 0.35);
        }),
        make("Filter Sweep", |p| {
            p.wave1("Saw");
            p.wave2("Saw");
            p.range2(7);
            p.set("OscMix", 0.4);
            p.cutoff_hz(300.0);
            p.set("LpfRes", 0.75);
            p.set("LfoFiltAmount", 0.55);
            p.lfo(0.25, "Sine", 0.0);
            p.env2(200.0, 2000.0, 0.9, 1500.0);
            p.fx(1, "Phaser", 0.3);
            p.fx(2, "Delay", 0.25);
        }),
    ]
}
