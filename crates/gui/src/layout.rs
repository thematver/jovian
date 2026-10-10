//! Instrument workspace: source → tone → voice, with a compact audition dock.

use crate::canvas::{Rgba, R};
use crate::theme as t;
use crate::widgets::{Icon, Item};
use jovian_core::params::addr as a;

pub const W: f32 = 1320.0;
pub const H: f32 = 780.0;
pub const HEADER_H: f32 = 72.0;
pub const MARGIN: f32 = 16.0;
pub const LEFT_W: f32 = 156.0;
pub const GAP: f32 = 10.0;
pub const BODY_TOP: f32 = 84.0;
pub const BOTTOM_TOP: f32 = 662.0;
pub const BOTTOM_H: f32 = H - BOTTOM_TOP - MARGIN;
pub const RIGHT_X: f32 = 184.0;
pub const RIGHT_W: f32 = W - MARGIN - RIGHT_X;
/// The area pages other than SYNTH use.
pub const BODY: R = R::new(MARGIN, BODY_TOP, W - 2.0 * MARGIN, BOTTOM_TOP - GAP - BODY_TOP);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Synth,
    Mod,
    Fx,
    Library,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Synth, Page::Mod, Page::Fx, Page::Library];
    pub fn title(self) -> &'static str {
        match self {
            Page::Synth => "SYNTH",
            Page::Mod => "MOD",
            Page::Fx => "EFFECTS",
            Page::Library => "LIBRARY",
        }
    }
    pub fn index(self) -> u32 {
        self as u32
    }
    pub fn from_index(i: u32) -> Page {
        Page::ALL.get(i as usize).copied().unwrap_or(Page::Synth)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Fader { addr: usize, label: &'static str, color: Rgba, origin: f32 },
    /// Items are shown in `order` (indices into the parameter's items).
    Switch { addr: usize, items: Vec<Item>, order: Vec<usize>, color: Rgba, vertical: bool },
    Toggle { addr: usize, label: &'static str, color: Rgba },
    Knob { addr: usize, label: &'static str, color: Rgba, bipolar: bool, radius: f32 },
    Stepper { addr: usize },
    HSlider { addr: usize, color: Rgba, bipolar: bool },
    Tab(Page),
    PatchPrev,
    PatchNext,
    PatchName,
    Keyboard,
    Bender,
    ModWheel,
    Zoom,
    /// A clickable area handled by a page module (id is page specific).
    Area(u32),
}

impl Kind {
    pub fn addr(&self) -> Option<usize> {
        match self {
            Kind::Fader { addr, .. }
            | Kind::Switch { addr, .. }
            | Kind::Toggle { addr, .. }
            | Kind::Knob { addr, .. }
            | Kind::Stepper { addr }
            | Kind::HSlider { addr, .. } => Some(*addr),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Control {
    pub r: R,
    /// None = on every page.
    pub page: Option<Page>,
    pub kind: Kind,
    /// Only shown while the discrete parameter `.0` is at item `.1` (effect parameters).
    pub when: Option<(usize, usize)>,
    /// UI-only source/effect inspector selection. Never stored in an audio parameter.
    pub view: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum Deco {
    Section { r: R, title: &'static str, accent: Rgba },
    Caption { text: &'static str, cx: f32, y: f32 },
    Bracket { x0: f32, x1: f32, y: f32, text: &'static str },
    FaderSlot { r: R, origin: f32 },
    KnobRing { cx: f32, cy: f32, radius: f32 },
    Label { text: &'static str, x: f32, y: f32, align: f32 },
}

#[derive(Default)]
pub struct Layout {
    pub controls: Vec<Control>,
    pub decos: Vec<(Option<Page>, Deco)>,
}

impl Layout {
    pub fn push(&mut self, page: Option<Page>, r: R, kind: Kind) {
        match &kind {
            Kind::Fader { origin, .. } => self.decos.push((page, Deco::FaderSlot { r, origin: *origin })),
            _ => {}
        }
        self.controls.push(Control { r, page, kind, when: None, view: None });
    }

    /// Adds a control that is only visible while `addr` is at item `index`.
    pub fn push_when(&mut self, page: Option<Page>, r: R, kind: Kind, addr: usize, index: usize) {
        self.controls.push(Control { r, page, kind, when: Some((addr, index)), view: None });
    }

    pub fn deco(&mut self, page: Option<Page>, d: Deco) {
        self.decos.push((page, d));
    }
}


fn texts(s: &[&'static str]) -> Vec<Item> { s.iter().map(|x| Item::Text(x)).collect() }
fn icons(s: &[Icon]) -> Vec<Item> { s.iter().map(|x| Item::Icon(*x)).collect() }

fn knob(l: &mut Layout, page: Option<Page>, cx: f32, y: f32, width: f32, radius: f32, addr: usize, label: &'static str, color: Rgba, bipolar: bool) {
    l.push(page, R::new(cx - width * 0.5, y, width, 2.0 * radius + 42.0), Kind::Knob { addr, label, color, bipolar, radius });
}
fn label(l: &mut Layout, page: Option<Page>, text: &'static str, x: f32, y: f32, align: f32) {
    l.deco(page, Deco::Label { text, x, y, align });
}
fn switch(l: &mut Layout, page: Option<Page>, r: R, addr: usize, items: Vec<Item>, color: Rgba) {
    let order = (0..items.len()).collect();
    l.push(page, r, Kind::Switch { addr, items, order, color, vertical: false });
}

pub fn display_rect() -> R { R::new(232.0, 12.0, 552.0, 48.0) }

fn header(l: &mut Layout) {
    let d = display_rect();
    l.push(None, R::new(d.x + 4.0, d.y + 5.0, 26.0, d.h - 10.0), Kind::PatchPrev);
    l.push(None, R::new(d.x + 34.0, d.y + 4.0, 216.0, d.h - 8.0), Kind::PatchName);
    l.push(None, R::new(d.x + 250.0, d.y + 5.0, 26.0, d.h - 10.0), Kind::PatchNext);
    for (i, p) in Page::ALL.iter().enumerate() {
        l.push(None, R::new(826.0 + i as f32 * 108.0, 14.0, 102.0, 44.0), Kind::Tab(*p));
    }
    l.push(None, R::new(W - MARGIN - 40.0, 25.0, 40.0, 24.0), Kind::Zoom);
}

/// Every primary control follows a sound-design task; advanced routing lives on MOD.
fn instrument(l: &mut Layout) {
    let p = Some(Page::Synth);
    // The two sound sources are paired, with tuning adjacent to their waveforms.
    switch(l, p, R::new(180.0, 172.0, 148.0, 28.0), a::Osc1Wave, icons(&[Icon::Triangle, Icon::Saw, Icon::Pulse, Icon::Square]), t::ORANGE);
    label(l, p, "OCTAVE", 180.0, 211.0, 0.0);
    l.push(p, R::new(234.0, 204.0, 94.0, 26.0), Kind::Stepper { addr: a::Osc1Coarse });
    switch(l, p, R::new(180.0, 280.0, 148.0, 28.0), a::Osc2Wave, icons(&[Icon::Sine, Icon::Saw, Icon::Pulse, Icon::Noise]), t::ORANGE);
    label(l, p, "SEMITONES", 34.0, 357.0, 0.0);
    l.push(p, R::new(104.0, 342.0, 78.0, 26.0), Kind::Stepper { addr: a::Osc2Coarse });
    knob(l, p, 218.0, 314.0, 58.0, 14.0, a::Osc2Finetune, "FINE", t::ORANGE, true);
    l.push(p, R::new(254.0, 342.0, 74.0, 26.0), Kind::Toggle { addr: a::Osc2Mode, label: "LOW FREQ", color: t::ORANGE });
    // Blend sits after both oscillators, rather than beside unrelated filters.
    l.push(p, R::new(34.0, 400.0, 294.0, 24.0), Kind::HSlider { addr: a::OscMix, color: t::ORANGE, bipolar: true });
    knob(l, p, 66.0, 442.0, 64.0, 16.0, a::OscCrossMod, "CROSS MOD", t::ORANGE, false);
    knob(l, p, 146.0, 442.0, 64.0, 16.0, a::OscPwmAmount, "PULSE WIDTH", t::ORANGE, false);
    label(l, p, "PWM SOURCE", 202.0, 445.0, 0.0);
    switch(l, p, R::new(200.0, 452.0, 128.0, 24.0), a::OscPwmMode, texts(&["ENV", "MAN", "LFO"]), t::ORANGE);
    label(l, p, "SYNC", 200.0, 493.0, 0.0);
    switch(l, p, R::new(236.0, 482.0, 92.0, 24.0), a::OscSyncSelect, texts(&["2→1", "OFF", "1→2"]), t::ORANGE);

    // The tone stage gets the largest gesture and a direct-manipulation response graph.
    knob(l, p, 432.0, 284.0, 106.0, 31.0, a::LpfCut, "CUTOFF", t::BLUE, false);
    knob(l, p, 542.0, 296.0, 90.0, 21.0, a::LpfRes, "RESONANCE", t::BLUE, false);
    knob(l, p, 638.0, 296.0, 88.0, 21.0, a::HPF_Cutoff, "HIGH PASS", t::BLUE, false);
    knob(l, p, 738.0, 296.0, 100.0, 21.0, a::LpfEnvAmount, "ENV DEPTH", t::RED, false);
    label(l, p, "SLOPE", 802.0, 292.0, 0.0);
    switch(l, p, R::new(802.0, 300.0, 98.0, 24.0), a::LpfSlop, texts(&["12 dB", "24 dB"]), t::BLUE);
    label(l, p, "ENVELOPE", 802.0, 336.0, 0.0);
    switch(l, p, R::new(802.0, 344.0, 98.0, 24.0), a::LpfEnvSource, texts(&["ENV 1", "ENV 2"]), t::RED);
    label(l, p, "KEY TRACK", 652.0, 391.0, 0.0);
    l.push(p, R::new(718.0, 381.0, 182.0, 18.0), Kind::HSlider { addr: a::LPF_KeyFollowAmount, color: t::BLUE, bipolar: false });
    for (source, addrs) in [[a::Env1_Attack, a::Env1_Decay, a::Env1_Sustain, a::Env1_Release], [a::Env2_Attack, a::Env2_Decay, a::Env2_Sustain, a::Env2Release]].iter().enumerate() {
        for (i, (&addr, &name)) in addrs.iter().zip(["ATTACK", "DECAY", "SUSTAIN", "RELEASE"].iter()).enumerate() {
            let before = l.controls.len();
            knob(l, p, 656.0 + i as f32 * 68.0, 422.0, 64.0, 14.0, addr, name, t::RED, false);
            l.controls[before].when = Some((a::LpfEnvSource, source));
        }
    }
    // Amplitude always follows ENV 2. Voice behavior sits immediately below it.
    for (i, (&addr, &name)) in [a::Env2_Attack, a::Env2_Decay, a::Env2_Sustain, a::Env2Release].iter().zip(["ATTACK", "DECAY", "SUSTAIN", "RELEASE"].iter()).enumerate() {
        knob(l, p, 988.0 + i as f32 * 86.0, 266.0, 80.0, 16.0, addr, name, t::RED, false);
    }
    switch(l, p, R::new(948.0, 350.0, 338.0, 26.0), a::AssignMode, texts(&["MONO", "UNISON", "POLY 1", "POLY 2"]), t::INK);
    label(l, p, "VOICES", 948.0, 408.0, 0.0);
    for mode in [0, 2, 3] {
        l.push_when(p, R::new(1006.0, 392.0, 80.0, 26.0), Kind::Stepper { addr: a::Polyphon }, a::AssignMode, mode);
    }
    l.push_when(p, R::new(1006.0, 392.0, 80.0, 26.0), Kind::Stepper { addr: a::UniVPN }, a::AssignMode, 1);
    label(l, p, "PORTAMENTO", 1110.0, 408.0, 0.0);
    l.push(p, R::new(1228.0, 392.0, 58.0, 26.0), Kind::Toggle { addr: a::Portamento_On, label: "GLIDE", color: t::INK });
    for mode in [0, 2, 3] {
        let before = l.controls.len();
        knob(l, p, 990.0, 430.0, 84.0, 20.0, a::VCA_EnvAmount, "LEVEL", t::INK, false);
        l.controls[before].when = Some((a::AssignMode, mode));
    }
    let before = l.controls.len();
    knob(l, p, 990.0, 430.0, 84.0, 20.0, a::VoiceDetuneAmount, "DETUNE", t::INK, false);
    l.controls[before].when = Some((a::AssignMode, 1));
    knob(l, p, 1110.0, 430.0, 100.0, 20.0, a::PanSpread, "WIDTH", t::INK, false);
    knob(l, p, 1230.0, 430.0, 100.0, 20.0, a::Portamento_Time, "GLIDE TIME", t::INK, false);

    // Musical motion is a horizontal strip with explicit clock semantics.
    label(l, p, "SHAPE", 34.0, 575.0, 0.0);
    switch(l, p, R::new(34.0, 584.0, 160.0, 28.0), a::LFO_Waveform, icons(&[Icon::Sine, Icon::RevSaw, Icon::Square, Icon::SampleHold]), t::YELLOW);
    label(l, p, "CLOCK", 210.0, 575.0, 0.0);
    switch(l, p, R::new(210.0, 584.0, 100.0, 28.0), a::LFO_MidiSync, texts(&["HZ", "SYNC"]), t::YELLOW);
    let before = l.controls.len();
    knob(l, p, 362.0, 562.0, 88.0, 17.0, a::LFO_RateUnSynced, "RATE", t::YELLOW, false);
    l.controls[before].when = Some((a::LFO_MidiSync, 0));
    l.push_when(p, R::new(322.0, 584.0, 80.0, 28.0), Kind::Stepper { addr: a::LFO_RateSynced }, a::LFO_MidiSync, 1);
    knob(l, p, 452.0, 562.0, 88.0, 17.0, a::OscLfoAmount, "PITCH", t::YELLOW, false);
    knob(l, p, 542.0, 562.0, 88.0, 17.0, a::LfoFiltAmount, "FILTER", t::YELLOW, false);
    knob(l, p, 626.0, 562.0, 70.0, 17.0, a::VCA_LfoAmount, "AMP", t::YELLOW, false);
    // Arpeggio is a playable rhythm section, not a remote sidebar.
    l.push(p, R::new(1246.0, 542.0, 40.0, 24.0), Kind::Toggle { addr: a::ArpSeq_ArpSeqMode, label: "ON", color: t::ORANGE });
    label(l, p, "PATTERN", 694.0, 575.0, 0.0);
    l.push(p, R::new(694.0, 584.0, 174.0, 28.0), Kind::Stepper { addr: a::ArpSeq_PatternMode });
    label(l, p, "OCTAVES", 882.0, 575.0, 0.0);
    switch(l, p, R::new(882.0, 584.0, 116.0, 28.0), a::ArpSeq_OctaveRepeat, texts(&["1", "2", "3", "4"]), t::ORANGE);
    label(l, p, "CLOCK", 1012.0, 575.0, 0.0);
    switch(l, p, R::new(1012.0, 584.0, 106.0, 28.0), a::ArpSeq_MidiSync, texts(&["HZ", "SYNC"]), t::ORANGE);
    let before = l.controls.len();
    knob(l, p, 1202.0, 562.0, 130.0, 17.0, a::ArpSeq_RateUnSynced, "RATE", t::ORANGE, false);
    l.controls[before].when = Some((a::ArpSeq_MidiSync, 0));
    l.push_when(p, R::new(1132.0, 584.0, 154.0, 28.0), Kind::Stepper { addr: a::ArpSeq_RateSynced }, a::ArpSeq_MidiSync, 1);
}

fn bottom(l: &mut Layout) {
    knob(l, None, 55.0, BOTTOM_TOP + 17.0, 72.0, 20.0, a::Volume, "OUTPUT", t::INK, false);
    l.push(None, R::new(110.0, BOTTOM_TOP + 14.0, 20.0, 56.0), Kind::Bender);
    l.push(None, R::new(144.0, BOTTOM_TOP + 14.0, 20.0, 56.0), Kind::ModWheel);
    l.push(None, R::new(RIGHT_X, BOTTOM_TOP, RIGHT_W, BOTTOM_H), Kind::Keyboard);
}

pub const KEY_LO: u8 = 36;
pub const KEY_HI: u8 = 96;

pub fn build() -> Layout {
    let mut l = Layout::default();
    header(&mut l);
    instrument(&mut l);
    bottom(&mut l);
    l
}
pub fn build_all() -> Layout {
    let mut l = build();
    crate::pages::layout(&mut l);
    l
}
