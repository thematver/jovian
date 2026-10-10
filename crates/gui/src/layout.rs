//! Where every control sits. Sections of a row are laid out from column specs and stretched so
//! that all rows share the same edges.

use crate::canvas::{Rgba, R};
use crate::theme as t;
use crate::widgets::{Icon, Item};
use jovian_core::params::addr as a;

pub const W: f32 = 1240.0;
pub const H: f32 = 720.0;
pub const HEADER_H: f32 = 52.0;
pub const MARGIN: f32 = 12.0;
pub const LEFT_W: f32 = 256.0;
pub const GAP: f32 = 8.0;
pub const BODY_TOP: f32 = HEADER_H + 8.0;
pub const ROW_H: f32 = 248.0;
pub const BOTTOM_TOP: f32 = BODY_TOP + 2.0 * ROW_H + GAP + GAP;
pub const BOTTOM_H: f32 = H - BOTTOM_TOP - MARGIN;
pub const RIGHT_X: f32 = MARGIN + LEFT_W + GAP;
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
        self.controls.push(Control { r, page, kind, when: None });
    }

    /// Adds a control that is only visible while `addr` is at item `index`.
    pub fn push_when(&mut self, page: Option<Page>, r: R, kind: Kind, addr: usize, index: usize) {
        self.controls.push(Control { r, page, kind, when: Some((addr, index)) });
    }

    pub fn deco(&mut self, page: Option<Page>, d: Deco) {
        self.decos.push((page, d));
    }
}

// MARK: - section builder

pub const FADER_W: f32 = 36.0;
pub const SWITCH_W: f32 = 44.0;
const PAD: f32 = 12.0;
const COL_GAP: f32 = 6.0;
const TITLE_H: f32 = 34.0;
const BOTTOM_PAD: f32 = 8.0;
const ITEM_H: f32 = 20.0;
const ITEM_GAP: f32 = 4.0;
const CAPTION_H: f32 = 14.0;

pub enum Stacked {
    Switch { addr: usize, items: Vec<Item>, order: Vec<usize>, caption: Option<&'static str> },
    Toggle { addr: usize, label: &'static str, caption: Option<&'static str> },
    Space(f32),
}

pub enum Col {
    Fader { addr: usize, label: &'static str, origin: f32 },
    Stack { w: f32, items: Vec<Stacked> },
    /// Extra space between columns.
    Gap(f32),
}

impl Col {
    fn width(&self) -> f32 {
        match self {
            Col::Fader { .. } => FADER_W,
            Col::Stack { w, .. } => *w,
            Col::Gap(g) => *g,
        }
    }
}

pub struct SectionSpec {
    pub title: &'static str,
    pub accent: Rgba,
    pub cols: Vec<Col>,
}

impl SectionSpec {
    fn natural(&self) -> f32 {
        let n = self.cols.iter().filter(|c| !matches!(c, Col::Gap(_))).count();
        2.0 * PAD + self.cols.iter().map(|c| c.width()).sum::<f32>() + COL_GAP * (n.max(1) - 1) as f32
    }
}

/// Lays out a row of sections from `x` over `width`, stretching inner gaps to fill it.
pub fn row(l: &mut Layout, page: Page, x: f32, y: f32, width: f32, h: f32, sections: Vec<SectionSpec>) {
    let natural: f32 = sections.iter().map(|s| s.natural()).sum::<f32>() + GAP * (sections.len() - 1) as f32;
    let extra = (width - natural).max(0.0);
    // extra space goes into the gaps between columns (and a little into the padding)
    let slots: usize = sections.iter().map(|s| s.cols.iter().filter(|c| !matches!(c, Col::Gap(_))).count() + 1).sum();
    let per_slot = extra / slots as f32;
    let mut sx = x;
    for s in sections {
        let ncols = s.cols.iter().filter(|c| !matches!(c, Col::Gap(_))).count();
        let sw = s.natural() + per_slot * (ncols + 1) as f32;
        let sr = R::new(sx, y, sw, h);
        l.deco(Some(page), Deco::Section { r: sr, title: s.title, accent: s.accent });
        let mut cx = sx + PAD + per_slot * 0.5;
        let top = y + TITLE_H;
        let bottom = y + h - BOTTOM_PAD;
        let mut first = true;
        for c in s.cols {
            if let Col::Gap(g) = c {
                cx += g;
                continue;
            }
            if !first {
                cx += COL_GAP + per_slot;
            }
            first = false;
            let w = c.width();
            match c {
                Col::Fader { addr, label, origin } => {
                    l.push(Some(page), R::new(cx, top, w, bottom - top), Kind::Fader { addr, label, color: s.accent, origin });
                }
                Col::Stack { items, .. } => {
                    // stacks are centred in the column, above the fader label line
                    let content: f32 = items
                        .iter()
                        .map(|it| match it {
                            Stacked::Space(h) => *h,
                            Stacked::Switch { items, caption, .. } => {
                                let n = items.len() as f32;
                                caption.map_or(0.0, |_| CAPTION_H) + n * ITEM_H + (n - 1.0) * ITEM_GAP + 10.0
                            }
                            Stacked::Toggle { caption, .. } => caption.map_or(0.0, |_| CAPTION_H) + ITEM_H + ITEM_GAP,
                        })
                        .sum();
                    let avail = bottom - top - crate::widgets::FADER_LABEL_H;
                    let mut yy = top + ((avail - content) * 0.42).max(2.0);
                    for it in items {
                        match it {
                            Stacked::Space(h) => yy += h,
                            Stacked::Switch { addr, items, order, caption } => {
                                if let Some(cap) = caption {
                                    l.deco(Some(page), Deco::Caption { text: cap, cx: cx + w * 0.5, y: yy + 8.0 });
                                    yy += CAPTION_H;
                                }
                                let n = items.len() as f32;
                                let hh = n * ITEM_H + (n - 1.0) * ITEM_GAP;
                                l.push(Some(page), R::new(cx, yy, w, hh), Kind::Switch { addr, items, order, color: s.accent, vertical: true });
                                yy += hh + 10.0;
                            }
                            Stacked::Toggle { addr, label, caption } => {
                                if let Some(cap) = caption {
                                    l.deco(Some(page), Deco::Caption { text: cap, cx: cx + w * 0.5, y: yy + 8.0 });
                                    yy += CAPTION_H;
                                }
                                l.push(Some(page), R::new(cx, yy, w, ITEM_H), Kind::Toggle { addr, label, color: t::WHITE_KEY });
                                yy += ITEM_H + ITEM_GAP;
                            }
                        }
                    }
                }
                Col::Gap(_) => {}
            }
            cx += w;
        }
        sx += sw + GAP;
    }
}

fn texts(s: &[&'static str]) -> Vec<Item> {
    s.iter().map(|x| Item::Text(x)).collect()
}

fn icons(s: &[Icon]) -> Vec<Item> {
    s.iter().map(|x| Item::Icon(*x)).collect()
}

fn fader(addr: usize, label: &'static str) -> Col {
    Col::Fader { addr, label, origin: 0.0 }
}

fn bipolar(addr: usize, label: &'static str) -> Col {
    Col::Fader { addr, label, origin: 0.5 }
}

fn stack(w: f32, items: Vec<Stacked>) -> Col {
    Col::Stack { w, items }
}

fn sw(addr: usize, items: Vec<Item>, order: &[usize], caption: Option<&'static str>) -> Stacked {
    Stacked::Switch { addr, items, order: order.to_vec(), caption }
}

fn toggle(addr: usize, label: &'static str, caption: Option<&'static str>) -> Stacked {
    Stacked::Toggle { addr, label, caption }
}

/// The SYNTH page, header and keyboard strip.
pub fn build() -> Layout {
    let mut l = Layout::default();
    header(&mut l);
    synth_rows(&mut l);
    left_column(&mut l);
    bottom(&mut l);
    l
}

fn header(l: &mut Layout) {
    // display: [<] name [>]
    let d = display_rect();
    l.push(None, R::new(d.x + 6.0, d.y + 6.0, 18.0, d.h - 12.0), Kind::PatchPrev);
    l.push(None, R::new(d.x + 28.0, d.y + 4.0, 196.0, d.h - 8.0), Kind::PatchName);
    l.push(None, R::new(d.x + 228.0, d.y + 6.0, 18.0, d.h - 12.0), Kind::PatchNext);
    let tw = 76.0;
    let mut x = W - MARGIN - 4.0 * tw - 34.0;
    for p in Page::ALL {
        l.push(None, R::new(x, 12.0, tw - 4.0, 28.0), Kind::Tab(p));
        x += tw;
    }
    l.push(None, R::new(W - MARGIN - 28.0, 14.0, 28.0, 24.0), Kind::Zoom);
}

pub fn display_rect() -> R {
    R::new(318.0, 7.0, 470.0, 38.0)
}

fn synth_rows(l: &mut Layout) {
    let p = Page::Synth;
    let row_a = vec![
        SectionSpec {
            title: "LFO",
            accent: t::YELLOW,
            cols: vec![
                fader(a::LFO_RateUnSynced, "RATE"),
                fader(a::LFO_FadeDelay, "DELAY"),
                stack(
                    SWITCH_W,
                    vec![
                        sw(a::LFO_Waveform, icons(&[Icon::Sine, Icon::RevSaw, Icon::Square, Icon::SampleHold]), &[0, 1, 2, 3], Some("WAVE")),
                        Stacked::Space(4.0),
                        toggle(a::LFO_MidiSync, "SYNC", None),
                        toggle(a::LFO_Retrig, "KEY", None),
                    ],
                ),
            ],
        },
        SectionSpec {
            title: "VCO MOD",
            accent: t::YELLOW,
            cols: vec![
                fader(a::OscLfoAmount, "LFO"),
                fader(a::OscEnvAmount, "ENV-1"),
                stack(SWITCH_W + 4.0, vec![sw(a::OscDestSelect, texts(&["VCO-2", "1 + 2", "VCO-1"]), &[2, 1, 0], Some("DEST"))]),
            ],
        },
        SectionSpec {
            title: "PWM",
            accent: t::YELLOW,
            cols: vec![
                fader(a::OscPwmAmount, "PW"),
                stack(SWITCH_W + 4.0, vec![sw(a::OscPwmMode, texts(&["ENV-1", "MAN", "LFO"]), &[0, 1, 2], Some("SOURCE"))]),
            ],
        },
        SectionSpec {
            title: "VCO-1",
            accent: t::ORANGE,
            cols: vec![
                fader(a::OscCrossMod, "X-MOD"),
                stack(SWITCH_W, vec![sw(a::Osc1Coarse, texts(&["16'", "8'", "4'", "2'"]), &[0, 1, 2, 3], Some("RANGE"))]),
                stack(
                    SWITCH_W,
                    vec![sw(a::Osc1Wave, icons(&[Icon::Triangle, Icon::Saw, Icon::Pulse, Icon::Square]), &[0, 1, 2, 3], Some("WAVE"))],
                ),
            ],
        },
        SectionSpec {
            title: "VCO-2",
            accent: t::ORANGE,
            cols: vec![
                stack(
                    SWITCH_W + 4.0,
                    vec![
                        sw(a::OscSyncSelect, texts(&["2 > 1", "OFF", "1 > 2"]), &[0, 1, 2], Some("SYNC")),
                        Stacked::Space(4.0),
                        toggle(a::Osc2Mode, "LOW FREQ", None),
                    ],
                ),
                Col::Fader { addr: a::Osc2Coarse, label: "RANGE", origin: 12.0 / 36.0 },
                bipolar(a::Osc2Finetune, "FINE"),
                stack(
                    SWITCH_W,
                    vec![sw(a::Osc2Wave, icons(&[Icon::Sine, Icon::Saw, Icon::Pulse, Icon::Noise]), &[0, 1, 2, 3], Some("WAVE"))],
                ),
            ],
        },
        SectionSpec {
            title: "MIXER",
            accent: t::ORANGE,
            cols: vec![bipolar(a::OscMix, "MIX"), fader(a::HPF_Cutoff, "HPF")],
        },
    ];
    row(l, p, RIGHT_X, BODY_TOP, RIGHT_W, ROW_H, row_a);

    let row_b = vec![
        SectionSpec {
            title: "VCF",
            accent: t::BLUE,
            cols: vec![
                fader(a::LpfCut, "FREQ"),
                fader(a::LpfRes, "RES"),
                stack(
                    SWITCH_W,
                    vec![
                        sw(a::LpfSlop, texts(&["-12", "-24"]), &[1, 0], Some("SLOPE")),
                        Stacked::Space(6.0),
                        sw(a::LpfEnvSource, texts(&["ENV-1", "ENV-2"]), &[0, 1], Some("ENV")),
                    ],
                ),
                fader(a::LpfEnvAmount, "ENV"),
                fader(a::LfoFiltAmount, "LFO"),
                fader(a::LPF_KeyFollowAmount, "KYBD"),
            ],
        },
        SectionSpec { title: "VCA", accent: t::BLUE, cols: vec![fader(a::VCA_EnvAmount, "LEVEL"), fader(a::VCA_LfoAmount, "LFO")] },
        SectionSpec {
            title: "ENV-1",
            accent: t::RED,
            cols: vec![
                fader(a::Env1_Attack, "A"),
                fader(a::Env1_Decay, "D"),
                fader(a::Env1_Sustain, "S"),
                fader(a::Env1_Release, "R"),
                stack(SWITCH_W + 6.0, vec![toggle(a::Env1_KeyFollow, "KEY FLW", None), toggle(a::Env1_Polarity, "INVERT", None)]),
            ],
        },
        SectionSpec {
            title: "ENV-2",
            accent: t::RED,
            cols: vec![
                fader(a::Env2_Attack, "A"),
                fader(a::Env2_Decay, "D"),
                fader(a::Env2_Sustain, "S"),
                fader(a::Env2Release, "R"),
                stack(SWITCH_W + 6.0, vec![toggle(a::Env2_KeyFollow, "KEY FLW", None)]),
            ],
        },
    ];
    row(l, p, RIGHT_X, BODY_TOP + ROW_H + GAP, RIGHT_W, ROW_H, row_b);
}

fn left_column(l: &mut Layout) {
    let p = Page::Synth;
    let x = MARGIN;
    let w = LEFT_W;
    let total = 2.0 * ROW_H + GAP;
    let (h1, h2) = (116.0, 184.0);
    let h3 = total - h1 - h2 - 2.0 * GAP;
    let knob = |l: &mut Layout, cx: f32, y: f32, addr: usize, label: &'static str, color: Rgba, bipolar: bool, radius: f32| {
        l.push(Some(p), R::new(cx - 30.0, y, 60.0, 2.0 * radius + 34.0), Kind::Knob { addr, label, color, bipolar, radius });
    };
    // MASTER: volume, tune, glide (+ on)
    let y = BODY_TOP;
    l.deco(Some(p), Deco::Section { r: R::new(x, y, w, h1), title: "MASTER", accent: t::WHITE_KEY });
    let ky = y + 36.0;
    knob(l, x + 46.0, ky, a::Volume, "VOLUME", t::WHITE_KEY, false, 18.0);
    knob(l, x + 116.0, ky, a::MasterTune, "TUNE", t::WHITE_KEY, true, 18.0);
    knob(l, x + 186.0, ky, a::Portamento_Time, "GLIDE", t::WHITE_KEY, false, 18.0);
    l.push(Some(p), R::new(x + w - 12.0 - 24.0, ky + 13.0, 24.0, 18.0), Kind::Toggle { addr: a::Portamento_On, label: "ON", color: t::WHITE_KEY });

    // ASSIGN
    let y = BODY_TOP + h1 + GAP;
    l.deco(Some(p), Deco::Section { r: R::new(x, y, w, h2), title: "ASSIGN", accent: t::RED });
    l.push(
        Some(p),
        R::new(x + 12.0, y + 36.0, w - 24.0, ITEM_H),
        Kind::Switch { addr: a::AssignMode, items: texts(&["SOLO", "UNISON", "POLY 1", "POLY 2"]), order: vec![0, 1, 2, 3], color: t::RED, vertical: false },
    );
    let ry = y + 72.0;
    let lx = x + 12.0;
    let bx = x + 72.0;
    l.deco(Some(p), Deco::Label { text: "VOICES", x: lx, y: ry + 12.5, align: 0.0 });
    l.push(Some(p), R::new(bx, ry, 74.0, 18.0), Kind::Stepper { addr: a::Polyphon });
    l.deco(Some(p), Deco::Label { text: "UNISON", x: lx, y: ry + 37.5, align: 0.0 });
    l.push(Some(p), R::new(bx, ry + 25.0, 74.0, 18.0), Kind::Stepper { addr: a::UniVPN });
    l.deco(Some(p), Deco::Label { text: "PRIORITY", x: lx, y: ry + 62.5, align: 0.0 });
    l.push(
        Some(p),
        R::new(bx, ry + 50.0, 74.0, 18.0),
        Kind::Switch { addr: a::PlPrio, items: texts(&["LO", "HI", "LAST"]), order: vec![0, 1, 2], color: t::RED, vertical: false },
    );
    knob(l, x + 182.0, ry - 2.0, a::VoiceDetuneAmount, "DETUNE", t::RED, false, 15.0);
    l.deco(Some(p), Deco::Label { text: "SPREAD", x: lx, y: ry + 87.5, align: 0.0 });
    l.push(Some(p), R::new(bx - 6.0, ry + 75.0, 86.0, 18.0), Kind::HSlider { addr: a::PanSpread, color: t::RED, bipolar: false });

    // ARPEGGIO
    let y = BODY_TOP + h1 + h2 + 2.0 * GAP;
    l.deco(Some(p), Deco::Section { r: R::new(x, y, w, h3), title: "ARPEGGIO", accent: t::ORANGE });
    l.push(Some(p), R::new(x + w - 12.0 - 38.0, y + 8.0, 38.0, 18.0), Kind::Toggle { addr: a::ArpSeq_ArpSeqMode, label: "ON", color: t::ORANGE });
    let gy = y + 38.0;
    l.push(
        Some(p),
        R::new(x + 12.0, gy, w - 24.0, ITEM_H),
        Kind::Switch { addr: a::ArpSeq_PatternMode, items: texts(&["UP", "DOWN", "ORD", "REV", "U/D", "U/D2", "RND"]), order: vec![0, 1, 2, 3, 4, 5, 6], color: t::ORANGE, vertical: false },
    );
    let ry = gy + 36.0;
    l.deco(Some(p), Deco::Label { text: "RANGE", x: lx, y: ry + 12.5, align: 0.0 });
    l.push(
        Some(p),
        R::new(bx, ry, 100.0, 18.0),
        Kind::Switch { addr: a::ArpSeq_OctaveRepeat, items: texts(&["1", "2", "3", "4"]), order: vec![0, 1, 2, 3], color: t::ORANGE, vertical: false },
    );
    l.deco(Some(p), Deco::Label { text: "CLOCK", x: lx, y: ry + 37.5, align: 0.0 });
    l.push(Some(p), R::new(bx, ry + 25.0, 100.0, 18.0), Kind::Switch { addr: a::ArpSeq_MidiSync, items: texts(&["FREE", "HOST"]), order: vec![0, 1], color: t::WHITE_KEY, vertical: false });
    l.deco(Some(p), Deco::Label { text: "DIVISION", x: lx, y: ry + 62.5, align: 0.0 });
    l.push(Some(p), R::new(bx, ry + 50.0, 100.0, 18.0), Kind::Stepper { addr: a::ArpSeq_RateSynced });
    knob(l, x + 214.0, ry + 4.0, a::ArpSeq_RateUnSynced, "RATE", t::ORANGE, false, 17.0);
}

fn bottom(l: &mut Layout) {
    let y = BOTTOM_TOP;
    let h = BOTTOM_H;
    let x = MARGIN;
    let w = LEFT_W;
    l.deco(None, Deco::Section { r: R::new(x, y, w, h), title: "BENDER", accent: t::WHITE_KEY });
    // wheels
    let wy = y + 32.0;
    let wh = h - 58.0;
    l.push(None, R::new(x + 16.0, wy, 20.0, wh), Kind::Bender);
    l.push(None, R::new(x + 46.0, wy, 20.0, wh), Kind::ModWheel);
    // routing grid: label | VCO-1 VCO-2 | VCF | amount
    let gx = x + 80.0;
    let col = [gx + 34.0, gx + 72.0, gx + 110.0];
    l.deco(None, Deco::Label { text: "BEND", x: gx, y: wy + 12.5, align: 0.0 });
    l.push(None, R::new(col[0], wy, 34.0, 18.0), Kind::Toggle { addr: a::BendVCO1, label: "VCO1", color: t::WHITE_KEY });
    l.push(None, R::new(col[1], wy, 34.0, 18.0), Kind::Toggle { addr: a::BendVCO2, label: "VCO2", color: t::WHITE_KEY });
    l.push(None, R::new(col[2], wy, 34.0, 18.0), Kind::Toggle { addr: a::BendVCF, label: "VCF", color: t::WHITE_KEY });
    l.deco(None, Deco::Label { text: "RANGE", x: gx, y: wy + 37.5, align: 0.0 });
    l.push(None, R::new(col[0], wy + 25.0, 72.0, 18.0), Kind::Stepper { addr: a::BendVCOAmount });
    l.push(None, R::new(col[2], wy + 25.0, 34.0, 18.0), Kind::HSlider { addr: a::BendVCFAmount, color: t::WHITE_KEY, bipolar: false });
    l.deco(None, Deco::Label { text: "LEVER", x: gx, y: wy + 62.5, align: 0.0 });
    l.push(None, R::new(col[0], wy + 50.0, 34.0, 18.0), Kind::Toggle { addr: a::LfoModVCO, label: "VCO", color: t::WHITE_KEY });
    l.push(None, R::new(col[1], wy + 50.0, 72.0, 18.0), Kind::HSlider { addr: a::LfoModVCOAmount, color: t::WHITE_KEY, bipolar: false });
    l.push(None, R::new(col[0], wy + 75.0, 34.0, 18.0), Kind::Toggle { addr: a::LfoModVCF, label: "VCF", color: t::WHITE_KEY });
    l.push(None, R::new(col[1], wy + 75.0, 72.0, 18.0), Kind::HSlider { addr: a::LfoModVcfAmount, color: t::WHITE_KEY, bipolar: false });
    // keyboard
    l.push(None, R::new(RIGHT_X, y, RIGHT_W, h), Kind::Keyboard);
}

/// First and last MIDI note of the on-screen keyboard.
pub const KEY_LO: u8 = 36;
pub const KEY_HI: u8 = 96;

/// Every page.
pub fn build_all() -> Layout {
    let mut l = build();
    crate::pages::layout(&mut l);
    l
}
