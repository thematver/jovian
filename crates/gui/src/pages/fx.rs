//! The EFFECTS page: global switch and routing, then three slots. Each slot shows the controls
//! of its current effect type and a live picture of what the effect does.

use crate::canvas::{vgrad, Canvas, Fill, PathBuilder, Rgba, R};
use crate::host::{index_of, Host};
use crate::layout::{Deco, Kind, Layout, Page, BODY};
use crate::text::{Face, Style};
use crate::theme as t;
use crate::ui::{Mods, Ui};
use crate::widgets::{Icon, Item};
use jovian_core::params::{self, addr as a, PARAMS};

pub const VIZ_BASE: u32 = 300; // 300...302
const P: Option<Page> = Some(Page::Fx);
const BAR_H: f32 = 44.0;

fn slot_rect(s: usize) -> R {
    let wdt = (BODY.w - 16.0) / 3.0;
    R::new(BODY.x + s as f32 * (wdt + 8.0), BODY.y + BAR_H + 8.0, wdt, BODY.h - BAR_H - 8.0)
}

pub fn viz_rect(s: usize) -> R {
    let r = slot_rect(s);
    R::new(r.x + 14.0, r.y + 74.0, r.w - 28.0, 118.0)
}

fn pid(slot: usize, suffix: &str) -> usize {
    params::address(&format!("FX{}_{}", slot + 1, suffix)).unwrap_or_else(|| panic!("FX{} {suffix}", slot + 1))
}

#[derive(Clone, Copy)]
enum C {
    Knob(&'static str, &'static str),
    Bipolar(&'static str, &'static str),
    Switch(&'static str, &'static str, &'static [&'static str]),
    Waves(&'static str, &'static str, &'static [Icon]),
    Toggle(&'static str, &'static str),
    Stepper(&'static str, &'static str),
}

/// Controls of each effect type (index = FXn_ModuleType item), in grid order.
fn type_controls(kind: usize) -> Vec<C> {
    use C::*;
    match kind {
        1 => vec![
            Knob("Dattorro_Decay", "DECAY"),
            Knob("Dattorro_E_Size", "SIZE"),
            Knob("Dattorro_Predelay", "PREDELAY"),
            Knob("Dattorro_Damping", "DAMPING"),
            Knob("Dattorro_HPBandWith", "LOW CUT"),
            Knob("Dattorro_LPBandWith", "HIGH CUT"),
            Knob("Dattorro_MSMix", "WIDTH"),
        ],
        2 => vec![
            Knob("Delay_DelayTimeMs", "TIME"),
            Knob("Delay_Feedback", "FEEDBACK"),
            Knob("Delay_HPFreq", "LOW CUT"),
            Knob("Delay_LPFreq", "HIGH CUT"),
            Toggle("Delay_MidiSync", "SYNC"),
            Stepper("Delay_DelayTimeSynced", "DIVISION"),
            Switch("Delay_RateSyncedType", "FEEL", &["BIN", "TRI", "DOT"]),
            Switch("Delay_StereoMode", "MODE", &["STEREO", "PING"]),
            Knob("Delay_StereoWidth", "OFFSET"),
            Knob("Delay_StereoWidthPP", "SPREAD"),
        ],
        3 => vec![
            Knob("Chorus_Freq", "RATE"),
            Knob("Chorus_Depth", "DEPTH"),
            Knob("Chorus_Delay", "DELAY"),
            Knob("Chorus_Feedback", "FEEDBACK"),
            Switch("Chorus_Voices", "VOICES", &["1", "2", "3"]),
            Waves("Chorus_LFOShape", "SHAPE", &[Icon::Sine, Icon::Triangle]),
            Toggle("Chorus_StereoMode", "STEREO"),
        ],
        4 => vec![
            Knob("Flanger_Chorus_Freq", "RATE"),
            Knob("Flanger_DepthFlanger", "DEPTH"),
            Knob("Flanger_MinDelay", "DELAY"),
            Knob("Flanger_Chorus_Feedback", "FEEDBACK"),
            Switch("Flanger_Chorus_FBPolarity", "POLARITY", &["+", "−"]),
            Knob("Flanger_Chorus_HPFfc", "LOW CUT"),
            Knob("Flanger_Chorus_LPFfc", "HIGH CUT"),
            Waves("Flanger_Chorus_LFOShape", "SHAPE", &[Icon::Sine, Icon::Triangle]),
            Toggle("Flanger_Chorus_StereoMode", "STEREO"),
        ],
        5 => vec![
            Knob("Phaser_LFO_RateUnSynced", "RATE"),
            Knob("Phaser_Freq", "FREQ"),
            Knob("Phaser_LFOAmount", "DEPTH"),
            Knob("Phaser_Feedback", "FEEDBACK"),
            Toggle("Phaser_LFO_MidiSync", "SYNC"),
            Stepper("Phaser_LFO_RateSynced", "DIVISION"),
            Switch("Phaser_LFO_RateSyncedType", "FEEL", &["BIN", "TRI", "DOT"]),
            Stepper("Phaser_NPoles", "POLES"),
            Knob("Phaser_Stereo", "STEREO"),
            Waves("Phaser_LFOWaveform", "WAVE", &[Icon::Sine, Icon::Triangle, Icon::Saw, Icon::RevSaw, Icon::Square, Icon::SampleHold]),
        ],
        6 => vec![Knob("TS808_Drive", "DRIVE"), Knob("TS808_Tone", "TONE"), Knob("TS808_Level", "LEVEL")],
        7 => vec![
            Knob("Compressor_Threshold", "THRESHOLD"),
            Knob("Compressor_Ratio", "RATIO"),
            Knob("Compressor_Attack", "ATTACK"),
            Knob("Compressor_Release", "RELEASE"),
            Bipolar("Compressor_InputGain", "INPUT"),
            Bipolar("Compressor_OutputGain", "OUTPUT"),
            Toggle("Compressor_Makeup", "AUTO GAIN"),
        ],
        8 => vec![Knob("BitCrusher_Bitdepth", "BITS"), Knob("BitCrusher_Downsample", "DOWNSAMPLE")],
        9 => vec![
            Switch("MultiModeFilter_Mode", "MODE", &["LP", "HP", "BP", "C+", "C−"]),
            Knob("MultiModeFilter_Freq", "CUTOFF"),
            Knob("MultiModeFilter_Q", "RESONANCE"),
            Switch("MultiModeFilter_Slope", "SLOPE", &["12", "24", "36"]),
            Knob("MultiModeFilter_CombFreq", "COMB FREQ"),
        ],
        10 => vec![
            Knob("ParamEq_LFShelvCutoff", "LOW"),
            Knob("ParamEq_PeakCutoff1", "PEAK 1"),
            Knob("ParamEq_PeakCutoff2", "PEAK 2"),
            Knob("ParamEq_PeakCutoffLast", "PEAK 3"),
            Knob("ParamEq_HFShelvCutoff", "HIGH"),
            Bipolar("ParamEq_LFShelvGain", "GAIN"),
            Bipolar("ParamEq_PeakGain1", "GAIN"),
            Bipolar("ParamEq_PeakGain2", "GAIN"),
            Bipolar("ParamEq_PeakGainLast", "GAIN"),
            Bipolar("ParamEq_HFShelvGain", "GAIN"),
            Knob("ParamEq_LFShelvQ", "Q"),
            Knob("ParamEq_PeakBandwidth1", "Q"),
            Knob("ParamEq_PeakBandwidth2", "Q"),
            Knob("ParamEq_PeakBandwidthLast", "Q"),
            Knob("ParamEq_HFShelvQ", "Q"),
        ],
        11 => vec![
            Knob("StereoPan_Amount", "AMOUNT"),
            Knob("StereoPan_LFOSimple_RateUnSynced", "RATE"),
            Switch("StereoPan_LFOSimple_RateType", "CLOCK", &["HZ", "SYNC", "TRI", "DOT"]),
            Stepper("StereoPan_LFOSimple_RateSynced", "DIVISION"),
        ],
        _ => vec![],
    }
}

fn texts(s: &[&'static str]) -> Vec<Item> {
    s.iter().map(|x| Item::Text(x)).collect()
}

pub fn layout(l: &mut Layout) {
    // global bar
    let bar = R::new(BODY.x, BODY.y, BODY.w, BAR_H);
    l.deco(P, Deco::Section { r: bar, title: "", accent: t::ORANGE });
    l.push(P, R::new(bar.x + 112.0, bar.y + 12.0, 44.0, 20.0), Kind::Toggle { addr: a::FX_Enable, label: "ON", color: t::ORANGE });
    l.deco(P, Deco::Label { text: "ROUTING", x: bar.x + 186.0, y: bar.y + 26.0, align: 0.0 });
    l.push(
        P,
        R::new(bar.x + 238.0, bar.y + 12.0, 150.0, 20.0),
        Kind::Switch { addr: a::FX_Routing, items: texts(&["SERIAL", "PARALLEL"]), order: vec![0, 1], color: t::WHITE_KEY, vertical: false },
    );

    for s in 0..3 {
        let r = slot_rect(s);
        let title: &'static str = ["FX 1", "FX 2", "FX 3"][s];
        l.deco(P, Deco::Section { r, title, accent: t::ORANGE });
        l.push(P, R::new(r.right() - 14.0 - 44.0, r.y + 8.0, 44.0, 18.0), Kind::Toggle { addr: pid(s, "Bypass"), label: "ON", color: t::ORANGE });
        let ty = pid(s, "ModuleType");
        l.push(P, R::new(r.x + 14.0, r.y + 40.0, 170.0, 22.0), Kind::Stepper { addr: ty });
        l.push(P, R::new(r.x + 228.0, r.y + 42.0, r.right() - 14.0 - r.x - 228.0, 18.0), Kind::HSlider { addr: pid(s, "Dry_Wet"), color: t::ORANGE, bipolar: false });
        l.push(P, viz_rect(s), Kind::Area(VIZ_BASE + s as u32));
        for kind in 1..=11 {
            let ctls = type_controls(kind);
            let eq = kind == 10;
            let cols = if eq { 5 } else { 4 };
            let cw = (r.w - 28.0) / cols as f32;
            let top = r.y + 204.0;
            let ch = if eq { 76.0 } else { 80.0 };
            for (c, (ci, span)) in ctls.iter().zip(cells(&ctls, cols)) {
                let col = (ci % cols) as f32;
                let row = (ci / cols) as f32;
                let cell = R::new(r.x + 14.0 + col * cw, top + row * ch, cw * span as f32, ch);
                let radius = if eq { 12.0 } else { 15.0 };
                let (kd, cap): (Kind, Option<&'static str>) = match *c {
                    C::Knob(sfx, label) => (Kind::Knob { addr: pid(s, sfx), label, color: t::ORANGE, bipolar: false, radius }, None),
                    C::Bipolar(sfx, label) => (Kind::Knob { addr: pid(s, sfx), label, color: t::ORANGE, bipolar: true, radius }, None),
                    C::Switch(sfx, label, items) => (
                        Kind::Switch { addr: pid(s, sfx), items: texts(items), order: (0..items.len()).collect(), color: t::WHITE_KEY, vertical: false },
                        Some(label),
                    ),
                    C::Waves(sfx, label, icons) => (
                        Kind::Switch { addr: pid(s, sfx), items: icons.iter().map(|i| Item::Icon(*i)).collect(), order: (0..icons.len()).collect(), color: t::WHITE_KEY, vertical: false },
                        Some(label),
                    ),
                    C::Toggle(sfx, label) => (Kind::Toggle { addr: pid(s, sfx), label, color: t::WHITE_KEY }, None),
                    C::Stepper(sfx, label) => (Kind::Stepper { addr: pid(s, sfx) }, Some(label)),
                };
                let ctl_r = match kd {
                    Kind::Knob { .. } => R::new(cell.x, cell.y, cell.w, cell.h - 4.0),
                    Kind::Switch { .. } => R::new(cell.x + 6.0, cell.y + 26.0, cell.w - 12.0, 20.0),
                    Kind::Toggle { .. } => R::new(cell.x + 8.0, cell.y + 26.0, cell.w - 16.0, 20.0),
                    _ => R::new(cell.x + 6.0, cell.y + 26.0, cell.w - 12.0, 20.0),
                };
                l.push_when(P, ctl_r, kd, ty, kind);
                if let Some(cap) = cap {
                    // captions of switches are drawn by the page (they depend on the type)
                    let _ = cap;
                }
            }
        }
    }
}

/// Grid cell index and span of every control: switches with three or more items take two cells.
fn cells(ctls: &[C], cols: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    for c in ctls {
        let span = match c {
            C::Switch(_, _, items) if items.len() >= 3 => 2,
            C::Waves(_, _, icons) if icons.len() >= 3 => 2,
            _ => 1,
        };
        if span == 2 && i % cols == cols - 1 {
            i += 1; // do not wrap a wide cell
        }
        out.push((i, span));
        i += span;
    }
    out
}

/// Captions for switch/stepper cells (drawn dynamically, depend on the effect type).
fn captions(kind: usize, r: R) -> Vec<(&'static str, f32, f32)> {
    let eq = kind == 10;
    let cols = if eq { 5 } else { 4 };
    let cw = (r.w - 28.0) / cols as f32;
    let top = r.y + 204.0;
    let ch = if eq { 76.0 } else { 80.0 };
    let ctls = type_controls(kind);
    ctls.iter()
        .zip(cells(&ctls, cols))
        .filter_map(|(c, (i, _))| {
            let label = match c {
                C::Switch(_, l, _) | C::Waves(_, l, _) | C::Stepper(_, l) => *l,
                _ => return None,
            };
            let col = (i % cols) as f32;
            let row = (i / cols) as f32;
            Some((label, r.x + 14.0 + col * cw + 8.0, top + row * ch + 18.0))
        })
        .collect()
}

pub fn draw_background(ui: &Ui, c: &mut Canvas) {
    if ui.page != Page::Fx {
        return;
    }
    let bar = R::new(BODY.x, BODY.y, BODY.w, BAR_H);
    c.text("EFFECTS", bar.x + 26.0, bar.y + 26.5, &t::TITLE, t::INK, 0.0);
    c.rrect(R::new(bar.x + 14.0, bar.y + 17.5, 3.0, 9.5), 1.0, t::ORANGE);
    for s in 0..3 {
        let r = slot_rect(s);
        c.text("MIX", r.x + 196.0, r.y + 55.0, &t::SMALL, t::INK_DIM, 0.0);
    }
}

pub fn draw(ui: &Ui, host: &dyn Host, c: &mut Canvas) {
    if ui.page != Page::Fx {
        return;
    }
    // routing diagram
    let bar = R::new(BODY.x, BODY.y, BODY.w, BAR_H);
    routing(c, host, R::new(bar.x + 420.0, bar.y + 8.0, 360.0, 28.0));
    for s in 0..3 {
        let r = slot_rect(s);
        let ty = pid(s, "ModuleType");
        let kind = index_of(host, ty);
        let on = index_of(host, pid(s, "Bypass")) == 1 && index_of(host, a::FX_Enable) == 1;
        let mix = PARAMS[pid(s, "Dry_Wet")].physical_f32(host.get(pid(s, "Dry_Wet")));
        let mix_label = if kind == 10 { format!("SCALE {:.0}%", mix) } else { format!("{:.0}%", mix) };
        c.text(&mix_label, r.right() - 14.0, r.y + 76.0 - 12.0, &Style::new(Face::Mono, 9.0, 0.0), t::INK_DIM, 1.0);
        for (label, x, y) in captions(kind, r) {
            c.text(label, x, y, &t::SMALL, t::INK_DIM, 0.0);
        }
        let v = viz_rect(s);
        viz(c, host, s, kind, v, on, ui.page_state.eq_drag.filter(|d| d.0 == s).map(|d| d.1));
    }
}

fn routing(c: &mut Canvas, host: &dyn Host, r: R) {
    let parallel = index_of(host, a::FX_Routing) == 1;
    let enabled = index_of(host, a::FX_Enable) == 1;
    let st = Style::new(Face::CondBold, 9.0, 0.8);
    let bw = 46.0;
    let ink = |on: bool| if on && enabled { t::INK } else { t::INK_FAINT };
    let line = Rgba::white(if enabled { 0.35 } else { 0.12 });
    let box_at = |c: &mut Canvas, x: f32, y: f32, s: usize| {
        let on = index_of(host, pid(s, "Bypass")) == 1 && index_of(host, pid(s, "ModuleType")) > 0;
        let br = R::new(x, y, bw, 16.0);
        c.rrect(br, 3.0, if on && enabled { t::ORANGE.with_alpha(0.25) } else { Rgba::white(0.05) });
        c.rrect_stroke(br, 3.0, 0.75, if on && enabled { t::ORANGE } else { Rgba::white(0.15) });
        c.text_in(["FX 1", "FX 2", "FX 3"][s], br, &st, ink(on), 0.5);
    };
    if parallel {
        let x0 = r.x + 30.0;
        let xs = [r.x + 90.0, r.x + 150.0, r.x + 210.0];
        c.text_in("IN", R::new(r.x, r.y, 24.0, r.h), &st, t::INK_DIM, 0.5);
        c.hline(r.x + 22.0, x0, r.cy(), 1.0, line);
        c.hline(x0, xs[2] + bw + 14.0, r.cy(), 1.0, line.mul_alpha(0.4));
        for (s, x) in xs.iter().enumerate() {
            box_at(c, *x, r.cy() - 8.0, s);
        }
        c.hline(xs[2] + bw, xs[2] + bw + 18.0, r.cy(), 1.0, line);
        c.text_in("OUT", R::new(xs[2] + bw + 18.0, r.y, 30.0, r.h), &st, t::INK_DIM, 0.5);
        c.text_in("(averaged)", R::new(xs[2] + bw + 50.0, r.y, 80.0, r.h), &Style::new(Face::Regular, 10.0, 0.0), t::INK_FAINT, 0.0);
    } else {
        c.text_in("IN", R::new(r.x, r.y, 24.0, r.h), &st, t::INK_DIM, 0.5);
        let mut x = r.x + 34.0;
        for s in 0..3 {
            c.hline(x - 10.0, x, r.cy(), 1.0, line);
            box_at(c, x, r.cy() - 8.0, s);
            x += bw + 14.0;
        }
        c.hline(x - 14.0, x - 4.0, r.cy(), 1.0, line);
        c.text_in("OUT", R::new(x - 2.0, r.y, 30.0, r.h), &st, t::INK_DIM, 0.5);
    }
}

// MARK: - visualisations

fn frame(c: &mut Canvas, r: R) {
    c.rrect(r, 5.0, vgrad(r.y, r.bottom(), Rgba::hex(0x060607), t::DISPLAY_BG));
    c.rrect_stroke(r, 5.0, 1.0, Rgba::black(0.9));
}

fn grid_log(c: &mut Canvas, g: R) {
    for f in [100.0f32, 1000.0, 10000.0] {
        let x = g.x + g.w * (f / 20.0).log10() / 3.0;
        c.vline(x, g.y, g.bottom(), 1.0, Rgba::white(0.06));
        let label = if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") };
        c.text(&label, x + 3.0, g.bottom() - 3.0, &Style::new(Face::Mono, 8.0, 0.0), Rgba::white(0.22), 0.0);
    }
}

fn curve(c: &mut Canvas, pts: &[(f32, f32)], base: f32, col: Rgba, fill: bool) {
    if pts.len() < 2 {
        return;
    }
    if fill {
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].0, base);
        for p in pts {
            pb.line_to(p.0, p.1);
        }
        pb.line_to(pts[pts.len() - 1].0, base);
        pb.close();
        if let Some(p) = pb.finish() {
            c.fill_path(&p, Fill::Solid(col.with_alpha(0.12)));
        }
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        pb.line_to(p.0, p.1);
    }
    if let Some(p) = pb.finish() {
        c.stroke_path(&p, 1.6, col);
    }
}

fn ph(host: &dyn Host, s: usize, sfx: &str) -> f64 {
    let a = pid(s, sfx);
    PARAMS[a].physical_f32(host.get(a))
}

fn viz(c: &mut Canvas, host: &dyn Host, s: usize, kind: usize, r: R, on: bool, eq_hot: Option<usize>) {
    frame(c, r);
    let g = r.inset_xy(10.0, 10.0);
    let col = if on { t::AMBER } else { t::AMBER_DIM };
    let small = Style::new(Face::Mono, 9.0, 0.0);
    match kind {
        0 => {
            c.text_in("Empty slot — choose an effect", r, &t::UI, t::INK_FAINT, 0.5);
        }
        1 => {
            // reverb: predelay, then the decay of the tank (RT60 from loop gain and size)
            let decay = ph(host, s, "Dattorro_Decay").max(1e-3);
            let size = ph(host, s, "Dattorro_E_Size");
            let pre = ph(host, s, "Dattorro_Predelay");
            let loop_s = 0.72 * size;
            let rt60 = (3.0 * loop_s / -(decay * decay).log10()).clamp(0.05, 60.0);
            let span = (pre + rt60 * 0.8).max(0.5);
            let n = 90;
            let mut pts = Vec::new();
            let mut seed = 12345u32;
            for i in 0..=n {
                let tt = span * i as f64 / n as f64;
                let env = if tt < pre { 0.0 } else { 10f64.powf(-3.0 * (tt - pre) / rt60) };
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let jitter = 0.75 + 0.25 * (seed % 1000) as f64 / 1000.0;
                pts.push((g.x + g.w * i as f32 / n as f32, g.bottom() - (env * jitter) as f32 * g.h));
            }
            curve(c, &pts, g.bottom(), col, true);
            c.text(&format!("RT60 ≈ {:.1} s", rt60), g.right(), g.y + 9.0, &small, t::INK_DIM, 1.0);
        }
        2 => {
            // delay: echo taps for both channels
            let synced = index_of(host, pid(s, "Delay_MidiSync")) == 1;
            let time = if synced {
                let bars = jovian_core::fx::dsp::bars(&format!("FX{}_Delay_DelayTimeSynced", s + 1));
                let i = index_of(host, pid(s, "Delay_DelayTimeSynced")).min(bars.len() - 1);
                jovian_core::fx::dsp::sync_seconds(bars[i], index_of(host, pid(s, "Delay_RateSyncedType")), 120.0)
            } else {
                ph(host, s, "Delay_DelayTimeMs") / 1000.0
            };
            let fb = ph(host, s, "Delay_Feedback");
            let ping = index_of(host, pid(s, "Delay_StereoMode")) == 1;
            let width = ph(host, s, "Delay_StereoWidth");
            let span = (time * 8.0).max(0.4);
            for k in 1..=24 {
                let a = fb.powi(k as i32 - 1);
                if a < 0.01 {
                    break;
                }
                for (ch, colr) in [(0, col), (1, t::ORANGE.mul_alpha(if on { 1.0 } else { 0.5 }))] {
                    let (tt, amp) = if ping {
                        if (k % 2 == 1) != (ch == 0) {
                            continue;
                        }
                        (time * k as f64, a)
                    } else {
                        (time * k as f64 * if ch == 1 { 1.0 + width } else { 1.0 }, a)
                    };
                    if tt > span {
                        continue;
                    }
                    let x = g.x + (tt / span) as f32 * g.w + if ch == 1 { 2.5 } else { 0.0 };
                    let h = amp as f32 * g.h;
                    c.rect(R::new(x - 1.0, g.bottom() - h, 2.0, h), colr);
                }
            }
            c.text(&format!("{:.0} ms", time * 1000.0), g.right(), g.y + 9.0, &small, t::INK_DIM, 1.0);
        }
        3 | 4 | 5 | 11 => {
            // modulation effects: the LFO driving them
            let (rate, tri) = match kind {
                3 => (ph(host, s, "Chorus_Freq"), index_of(host, pid(s, "Chorus_LFOShape")) == 1),
                4 => (ph(host, s, "Flanger_Chorus_Freq"), index_of(host, pid(s, "Flanger_Chorus_LFOShape")) == 1),
                5 => (ph(host, s, "Phaser_LFO_RateUnSynced"), index_of(host, pid(s, "Phaser_LFOWaveform")) == 1),
                _ => (ph(host, s, "StereoPan_LFOSimple_RateUnSynced"), false),
            };
            let depth = match kind {
                3 => ph(host, s, "Chorus_Depth") / 10.0,
                4 => ph(host, s, "Flanger_DepthFlanger") / 10.0,
                5 => ph(host, s, "Phaser_LFOAmount"),
                _ => ph(host, s, "StereoPan_Amount"),
            }
            .clamp(0.05, 1.0);
            let cycles = (rate * 2.0).clamp(1.0, 8.0);
            let n = 120;
            for (k, off) in [(0, 0.0), (1, 0.25)] {
                let mut pts = Vec::new();
                for i in 0..=n {
                    let u = i as f64 / n as f64;
                    let p = (u * cycles + off).fract();
                    let v = if tri { jovian_core::fx::dsp::triangle(p as f32) as f64 } else { (p * std::f64::consts::TAU).sin() };
                    pts.push((g.x + g.w * u as f32, g.cy() - (v * depth) as f32 * g.h * 0.45));
                }
                curve(c, &pts, g.cy(), if k == 0 { col } else { col.mul_alpha(0.4) }, false);
            }
            c.text(&format!("{rate:.2} Hz"), g.right(), g.y + 9.0, &small, t::INK_DIM, 1.0);
        }
        6 => {
            // overdrive: transfer curve of the clipping stage
            let drive = host.get(pid(s, "TS808_Drive")) as f64;
            let gain = 2.0 * 30f64.powf(drive);
            let n = 80;
            let mut pts = Vec::new();
            for i in 0..=n {
                let x = -1.0 + 2.0 * i as f64 / n as f64;
                let y = (x + ((gain * x * 0.3 + 0.15).tanh() - 0.15f64.tanh())) / 2.0;
                pts.push((g.cx() + (x as f32) * g.w * 0.5, g.cy() - (y.clamp(-1.0, 1.0) as f32) * g.h * 0.5));
            }
            c.hline(g.x, g.right(), g.cy(), 1.0, Rgba::white(0.08));
            c.vline(g.cx(), g.y, g.bottom(), 1.0, Rgba::white(0.08));
            curve(c, &pts, g.cy(), col, false);
        }
        7 => {
            // compressor: static curve + gain reduction meter
            let thr = ph(host, s, "Compressor_Threshold");
            let ratio = ph(host, s, "Compressor_Ratio").max(1.0);
            let gr_box = R::new(g.right() - 10.0, g.y, 10.0, g.h);
            let gg = R::new(g.x, g.y, g.w - 22.0, g.h);
            let n = 80;
            let mut pts = Vec::new();
            for i in 0..=n {
                let x = -60.0 + 60.0 * i as f64 / n as f64;
                let over = x - thr;
                let y = if over > 3.0 {
                    thr + over / ratio
                } else if over > -3.0 {
                    let k = over + 3.0;
                    x + (1.0 / ratio - 1.0) * k * k / 12.0
                } else {
                    x
                };
                pts.push((gg.x + gg.w * ((x + 60.0) / 60.0) as f32, gg.bottom() - gg.h * (((y + 60.0) / 60.0).clamp(0.0, 1.0)) as f32));
            }
            let mut pb = PathBuilder::new();
            pb.move_to(gg.x, gg.bottom());
            pb.line_to(gg.right(), gg.y);
            if let Some(p) = pb.finish() {
                c.stroke_path(&p, 1.0, Rgba::white(0.1));
            }
            curve(c, &pts, gg.bottom(), col, false);
            let gr = host.compressor_reduction(s).clamp(0.0, 30.0);
            c.rrect(gr_box, 2.0, Rgba::white(0.06));
            let h = gr_box.h * gr / 30.0;
            c.rrect(R::new(gr_box.x, gr_box.y, gr_box.w, h), 2.0, t::RED);
            c.text("GR", gr_box.cx(), gr_box.bottom() + 0.0 - 2.0, &Style::new(Face::CondBold, 7.0, 0.3), t::INK_DIM, 0.5);
        }
        8 => {
            // bit crusher: a quantised and held sine
            let bits = ph(host, s, "BitCrusher_Bitdepth");
            let down = ph(host, s, "BitCrusher_Downsample").max(1.0);
            let q = 2f64.powf(bits - 1.0);
            let n = 200;
            let hold = (down / 3.0).max(1.0);
            let mut pts = Vec::new();
            let mut held = 0.0;
            let mut cnt = 0.0;
            for i in 0..=n {
                let u = i as f64 / n as f64;
                cnt += 1.0;
                if cnt >= hold || i == 0 {
                    cnt -= hold;
                    held = ((u * std::f64::consts::TAU * 2.0).sin() * q).round() / q;
                }
                pts.push((g.x + g.w * u as f32, g.cy() - held as f32 * g.h * 0.45));
            }
            curve(c, &pts, g.cy(), col, false);
            c.text(&format!("{bits:.1} bit  ÷{down:.0}"), g.right(), g.y + 9.0, &small, t::INK_DIM, 1.0);
        }
        9 => {
            grid_log(c, g);
            let mode = index_of(host, pid(s, "MultiModeFilter_Mode"));
            let fc = ph(host, s, "MultiModeFilter_Freq");
            let q = ph(host, s, "MultiModeFilter_Q");
            let stages = index_of(host, pid(s, "MultiModeFilter_Slope")) + 1;
            let comb = ph(host, s, "MultiModeFilter_CombFreq");
            let n = 120;
            let mut pts = Vec::new();
            for i in 0..=n {
                let u = i as f64 / n as f64;
                let f = 20.0 * 1000f64.powf(u);
                let db = filter_db(mode, f, fc, q, stages, comb);
                pts.push((g.x + g.w * u as f32, g.bottom() - g.h * (((db + 36.0) / 54.0).clamp(0.0, 1.0)) as f32));
            }
            curve(c, &pts, g.bottom(), col, true);
        }
        10 => eq_viz(c, host, s, g, col, eq_hot),
        _ => {}
    }
    if !on && kind != 0 {
        c.text("BYPASSED", r.x + 10.0, r.y + 14.0, &Style::new(Face::CondBold, 9.0, 1.0), t::INK_FAINT, 0.0);
    }
}

fn filter_db(mode: usize, f: f64, fc: f64, q: f64, stages: usize, comb: f64) -> f64 {
    let w = f / fc;
    let mag2 = |q: f64, lp: bool| {
        // |H|² of a 2-pole section
        let den = (1.0 - w * w).powi(2) + (w / q).powi(2);
        if lp {
            1.0 / den
        } else {
            w.powi(4) / den
        }
    };
    let db = match mode {
        0 | 1 => {
            let lp = mode == 0;
            let qs: Vec<f64> = match stages {
                1 => vec![q],
                2 => vec![0.541_196, (q * 1.847_759).min(40.0)],
                _ => vec![0.517_638, 0.707_107, (q * 2.732_051).min(40.0)],
            };
            qs.iter().map(|qq| 10.0 * mag2(*qq, lp).log10()).sum::<f64>()
        }
        2 => {
            let k = 1.0 / q;
            let den = (1.0 - w * w).powi(2) + (w * k).powi(2);
            let m = (w * k).powi(2) / den;
            10.0 * m.log10() * stages as f64
        }
        _ => {
            // comb: gain from Q, notches / peaks at multiples of the comb frequency
            let g = 1.0 - 0.5 / q.max(0.5);
            let ph = 2.0 * std::f64::consts::PI * f / comb.max(1.0);
            let (re, im) = if mode == 3 {
                let d = (1.0 - g * ph.cos(), g * ph.sin());
                (d.0 / (d.0 * d.0 + d.1 * d.1), -d.1 / (d.0 * d.0 + d.1 * d.1))
            } else {
                (1.0 + g * ph.cos(), -g * ph.sin())
            };
            let norm = if mode == 3 { (1.0 - g * g).sqrt() } else { 1.0 / (1.0 + g * g).sqrt() };
            20.0 * ((re * re + im * im).sqrt() * norm).max(1e-6).log10()
        }
    };
    db.max(-60.0)
}

/// EQ bands: (frequency id, gain id, q id, kind 0 low shelf / 1 bell / 2 high shelf).
const EQ_BANDS: [(&str, &str, &str, u8); 5] = [
    ("ParamEq_LFShelvCutoff", "ParamEq_LFShelvGain", "ParamEq_LFShelvQ", 0),
    ("ParamEq_PeakCutoff1", "ParamEq_PeakGain1", "ParamEq_PeakBandwidth1", 1),
    ("ParamEq_PeakCutoff2", "ParamEq_PeakGain2", "ParamEq_PeakBandwidth2", 1),
    ("ParamEq_PeakCutoffLast", "ParamEq_PeakGainLast", "ParamEq_PeakBandwidthLast", 1),
    ("ParamEq_HFShelvCutoff", "ParamEq_HFShelvGain", "ParamEq_HFShelvQ", 2),
];
const EQ_RANGE_DB: f64 = 18.0;

fn band_db(kind: u8, f: f64, f0: f64, db: f64, q: f64) -> f64 {
    let a = 10f64.powf(db / 40.0);
    let w = f / f0;
    // analog prototypes of the RBJ filters, s = jw
    let (nr, ni, dr, di) = match kind {
        1 => (1.0 - w * w, w * a / q, 1.0 - w * w, w / (a * q)),
        0 => {
            let k = a.sqrt() / q;
            (a * (a - w * w), a * k * w, 1.0 - a * w * w, k * w)
        }
        _ => {
            let k = a.sqrt() / q;
            (a * (1.0 - a * w * w), a * k * w, a - w * w, k * w)
        }
    };
    10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di)).max(1e-12).log10()
}

fn eq_node(host: &dyn Host, s: usize, b: usize, g: R) -> (f32, f32) {
    let (fi, gi, _, _) = EQ_BANDS[b];
    let scale = ph(host, s, "ParamEq_Scale") * PARAMS[pid(s, "Dry_Wet")].physical_f32(host.get(pid(s, "Dry_Wet"))) / 100.0;
    let f = ph(host, s, fi);
    let db = ph(host, s, gi) * scale;
    (g.x + g.w * ((f / 20.0).log10() / 3.0) as f32, g.cy() - (db / EQ_RANGE_DB) as f32 * g.h * 0.5)
}

fn eq_viz(c: &mut Canvas, host: &dyn Host, s: usize, g: R, col: Rgba, hot: Option<usize>) {
    grid_log(c, g);
    c.hline(g.x, g.right(), g.cy(), 1.0, Rgba::white(0.1));
    let scale = ph(host, s, "ParamEq_Scale") * PARAMS[pid(s, "Dry_Wet")].physical_f32(host.get(pid(s, "Dry_Wet"))) / 100.0;
    let n = 140;
    let mut pts = Vec::new();
    for i in 0..=n {
        let u = i as f64 / n as f64;
        let f = 20.0 * 1000f64.powf(u);
        let mut db = 0.0;
        for (fi, gi, qi, k) in EQ_BANDS {
            db += band_db(k, f, ph(host, s, fi), ph(host, s, gi) * scale, ph(host, s, qi).max(0.05));
        }
        pts.push((g.x + g.w * u as f32, g.cy() - (db / EQ_RANGE_DB).clamp(-1.0, 1.0) as f32 * g.h * 0.5));
    }
    curve(c, &pts, g.cy(), col, true);
    let band_cols = [t::RED, t::ORANGE, t::YELLOW, t::BLUE, t::WHITE_KEY];
    for b in 0..5 {
        let (x, y) = eq_node(host, s, b, g);
        let on = hot == Some(b);
        c.circle(x, y, if on { 6.5 } else { 5.0 }, band_cols[b]);
        c.circle_stroke(x, y, 6.0, 1.0, Rgba::black(0.6));
    }
}

/// Starts dragging the nearest EQ node.
pub fn area_down(ui: &mut Ui, host: &dyn Host, id: u32, r: R, x: f32, y: f32, double: bool) {
    let s = (id - VIZ_BASE) as usize;
    if index_of(host, pid(s, "ModuleType")) != 10 {
        return;
    }
    let g = r.inset_xy(10.0, 10.0);
    let (b, d) = (0..5)
        .map(|b| {
            let (nx, ny) = eq_node(host, s, b, g);
            (b, (nx - x).hypot(ny - y))
        })
        .fold((0, f32::MAX), |acc, v| if v.1 < acc.1 { v } else { acc });
    if d > 18.0 {
        return;
    }
    let (fi, gi, _, _) = EQ_BANDS[b];
    if double {
        host.set_once(pid(s, gi), PARAMS[pid(s, gi)].default as f32);
        return;
    }
    host.begin(pid(s, fi));
    host.begin(pid(s, gi));
    ui.page_state.eq_drag = Some((s, b));
    ui.focus = Some((pid(s, fi), std::time::Instant::now()));
}

pub fn area_drag(ui: &mut Ui, host: &dyn Host, id: u32, x: f32, y: f32, _mods: Mods) {
    let Some((s, b)) = ui.page_state.eq_drag else { return };
    if id != VIZ_BASE + s as u32 {
        return;
    }
    let g = viz_rect(s).inset_xy(10.0, 10.0);
    let (fi, gi, _, _) = EQ_BANDS[b];
    let f = 20.0 * 1000f64.powf(((x - g.x) / g.w).clamp(0.0, 1.0) as f64);
    let scale = (ph(host, s, "ParamEq_Scale") * PARAMS[pid(s, "Dry_Wet")].physical_f32(host.get(pid(s, "Dry_Wet"))) / 100.0).max(0.05);
    let db = ((g.cy() - y) / (g.h * 0.5)) as f64 * EQ_RANGE_DB / scale;
    let fa = pid(s, fi);
    let ga = pid(s, gi);
    host.set(fa, PARAMS[fa].normalized_from_physical(f).clamp(0.0, 1.0) as f32);
    host.set(ga, PARAMS[ga].normalized_from_physical(db.clamp(-15.0, 15.0)).clamp(0.0, 1.0) as f32);
    ui.focus = Some((ga, std::time::Instant::now()));
}

pub fn area_up(ui: &mut Ui, host: &dyn Host) {
    if let Some((s, b)) = ui.page_state.eq_drag.take() {
        let (fi, gi, _, _) = EQ_BANDS[b];
        host.end(pid(s, fi));
        host.end(pid(s, gi));
    }
}

/// Mouse wheel over an EQ node changes its Q.
pub fn area_wheel(ui: &mut Ui, host: &dyn Host, id: u32, dy: f32) {
    let s = (id - VIZ_BASE) as usize;
    if index_of(host, pid(s, "ModuleType")) != 10 {
        return;
    }
    let g = viz_rect(s).inset_xy(10.0, 10.0);
    let (x, y) = ui.mouse;
    if let Some(b) = (0..5).find(|b| {
        let (nx, ny) = eq_node(host, s, *b, g);
        (nx - x).hypot(ny - y) < 18.0
    }) {
        let qa = pid(s, EQ_BANDS[b].2);
        let v = (host.get(qa) + dy.signum() * 0.03).clamp(0.0, 1.0);
        host.set_once(qa, v);
        ui.focus = Some((qa, std::time::Instant::now()));
    }
}
