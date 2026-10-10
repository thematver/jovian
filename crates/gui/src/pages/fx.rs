//! Three effect slots form a compact chain. Selecting a slot opens its processor at full size,
//! keeping the sound-shaping controls beside a useful, readable visualization.

use crate::canvas::{Canvas, Fill, PathBuilder, Rgba, R};
use crate::host::{index_of, Host};
use crate::layout::{Kind, Layout, Page, BODY};
use crate::text::{Face, Style};
use crate::theme as t;
use crate::ui::{Mods, Ui};
use crate::widgets::{Icon, Item};
use jovian_core::params::{self, addr as a, PARAMS};

pub const VIZ_BASE: u32 = 300; // 300...302: editable EQ graphs
pub const SELECT_BASE: u32 = 330; // 330...332: effect chain selection
const P: Option<Page> = Some(Page::Fx);
const SUMMARY_H: f32 = 90.0;
const SLOT_GAP: f32 = 12.0;

fn slot_rect(s: usize) -> R {
    let width = (BODY.w - SLOT_GAP * 2.0) / 3.0;
    R::new(BODY.x + s as f32 * (width + SLOT_GAP), BODY.y, width, SUMMARY_H)
}

fn detail_rect() -> R {
    R::new(BODY.x, BODY.y + SUMMARY_H + 12.0, BODY.w, BODY.h - SUMMARY_H - 12.0)
}

pub fn viz_rect(_s: usize) -> R {
    let r = detail_rect();
    R::new(r.x + 18.0, r.y + 66.0, r.w * 0.52, r.h - 128.0)
}

fn controls_rect() -> R {
    let r = detail_rect();
    let v = viz_rect(0);
    R::new(v.right() + 22.0, r.y + 76.0, r.right() - v.right() - 40.0, r.h - 94.0)
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
    let detail = detail_rect();
    // The rack controls stay available while editing any processor.
    l.push(P, R::new(detail.right() - 424.0, detail.y + 14.0, 62.0, 24.0),
        Kind::Toggle { addr: a::FX_Enable, label: "RACK ON", color: t::ORANGE });
    l.push(P, R::new(detail.right() - 232.0, detail.y + 14.0, 214.0, 24.0),
        Kind::Switch { addr: a::FX_Routing, items: texts(&["SERIAL", "PARALLEL"]), order: vec![0, 1], color: t::WHITE_KEY, vertical: false });

    for s in 0..3 {
        let r = slot_rect(s);
        // Added first so type, bypass and mix controls take precedence in hit testing.
        l.push(P, r, Kind::Area(SELECT_BASE + s as u32));
        l.push(P, R::new(r.right() - 66.0, r.y + 12.0, 50.0, 22.0),
            Kind::Toggle { addr: pid(s, "Bypass"), label: "ON", color: t::ORANGE });
        let ty = pid(s, "ModuleType");
        l.push(P, R::new(r.x + 16.0, r.y + 44.0, r.w * 0.55, 30.0), Kind::Stepper { addr: ty });
        l.push(P, R::new(r.x + r.w * 0.64, r.y + 44.0, r.w * 0.32, 20.0),
            Kind::HSlider { addr: pid(s, "Dry_Wet"), color: t::ORANGE, bipolar: false });
        l.push_when(P, viz_rect(s), Kind::Area(VIZ_BASE + s as u32), ty, 10);
        l.controls.last_mut().unwrap().view = Some(s);
        for kind in 1..=11 {
            let ctls = type_controls(kind);
            let cols = if kind == 10 { 5 } else { 4 };
            let grid = controls_rect();
            let cw = grid.w / cols as f32;
            for (control, (index, span)) in ctls.iter().zip(cells(&ctls, cols)) {
                let cell = R::new(grid.x + (index % cols) as f32 * cw,
                    grid.y + (index / cols) as f32 * 112.0, cw * span as f32, 104.0);
                let radius = if kind == 10 { 22.0 } else { 26.0 };
                let kd = match *control {
                    C::Knob(sfx, label) => Kind::Knob { addr: pid(s, sfx), label, color: t::ORANGE, bipolar: false, radius },
                    C::Bipolar(sfx, label) => Kind::Knob { addr: pid(s, sfx), label, color: t::ORANGE, bipolar: true, radius },
                    C::Switch(sfx, _, items) => Kind::Switch { addr: pid(s, sfx), items: texts(items), order: (0..items.len()).collect(), color: t::WHITE_KEY, vertical: false },
                    C::Waves(sfx, _, icons) => Kind::Switch { addr: pid(s, sfx), items: icons.iter().map(|i| Item::Icon(*i)).collect(), order: (0..icons.len()).collect(), color: t::WHITE_KEY, vertical: false },
                    C::Toggle(sfx, label) => Kind::Toggle { addr: pid(s, sfx), label, color: t::WHITE_KEY },
                    C::Stepper(sfx, _) => Kind::Stepper { addr: pid(s, sfx) },
                };
                let ctl_r = match kd {
                    Kind::Knob { .. } => cell,
                    _ => R::new(cell.x + 8.0, cell.y + 30.0, cell.w - 16.0, 28.0),
                };
                l.push_when(P, ctl_r, kd, ty, kind);
                l.controls.last_mut().unwrap().view = Some(s);
            }
        }
    }
}

/// Wide switches take two cells, never wrapping over a row boundary.
fn cells(ctls: &[C], cols: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    for control in ctls {
        let span = match control {
            C::Switch(_, _, items) if items.len() >= 3 => 2,
            C::Waves(_, _, icons) if icons.len() >= 3 => 2,
            _ => 1,
        };
        if span == 2 && i % cols == cols - 1 { i += 1; }
        out.push((i, span));
        i += span;
    }
    out
}

fn captions(kind: usize) -> Vec<(&'static str, f32, f32)> {
    let grid = controls_rect();
    let cols = if kind == 10 { 5 } else { 4 };
    let cw = grid.w / cols as f32;
    let ctls = type_controls(kind);
    ctls.iter().zip(cells(&ctls, cols)).filter_map(|(control, (index, _))| {
        let label = match control {
            C::Switch(_, label, _) | C::Waves(_, label, _) | C::Stepper(_, label) => *label,
            _ => return None,
        };
        Some((label, grid.x + (index % cols) as f32 * cw + 8.0,
            grid.y + (index / cols) as f32 * 112.0 + 17.0))
    }).collect()
}

fn panel(c: &mut Canvas, r: R) {
    c.rrect(r, 5.0, t::PANEL_TOP);
    c.rrect_stroke(r, 5.0, 1.0, t::LINE);
}

pub fn draw_background(ui: &Ui, c: &mut Canvas) {
    if ui.page != Page::Fx { return; }
    for s in 0..3 { panel(c, slot_rect(s)); }
    let detail = detail_rect();
    panel(c, detail);
    c.hline(detail.x + 18.0, detail.right() - 18.0, detail.y + 49.0, 1.0, t::LINE);
    c.text("ROUTING", detail.right() - 294.0, detail.y + 30.0, &t::SMALL, t::INK_DIM, 0.0);
    let v = viz_rect(ui.page_state.fx_selected.min(2));
    c.vline(v.right() + 10.0, detail.y + 66.0, detail.bottom() - 18.0, 1.0, t::LINE);
}

pub fn draw(ui: &Ui, host: &dyn Host, c: &mut Canvas) {
    if ui.page != Page::Fx { return; }
    let selected = ui.page_state.fx_selected.min(2);
    for s in 0..3 {
        let r = slot_rect(s);
        let active = selected == s;
        c.rrect_stroke(r.inset(0.5), 5.0, if active { 2.0 } else { 1.0 }, if active { t::ORANGE } else { t::LINE });
        c.circle(r.x + 18.0, r.y + 22.0, 3.0, if active { t::ORANGE } else { t::INK_FAINT });
        let title = format!("{:02}   {}", s + 1, if active { "EDITING" } else { "SELECT TO EDIT" });
        c.text(&title, r.x + 30.0, r.y + 26.0, &Style::new(Face::Semi, 10.5, 0.65), if active { t::INK } else { t::INK_DIM }, 0.0);
        let kind = index_of(host, pid(s, "ModuleType"));
        let mix = PARAMS[pid(s, "Dry_Wet")].physical_f32(host.get(pid(s, "Dry_Wet")));
        c.text(if kind == 10 { "GAIN" } else { "MIX" }, r.x + r.w * 0.64 + 6.0, r.y + 39.0, &t::SMALL, t::INK_DIM, 0.0);
        c.text(&format!("{mix:.0}%"), r.right() - 16.0, r.y + 76.0, &t::VALUE, t::VALUE_INK, 1.0);
        if s < 2 {
            c.text(if index_of(host, a::FX_Routing) == 0 { "›" } else { "+" }, r.right() + SLOT_GAP * 0.5, r.cy() + 4.0,
                &Style::new(Face::Mono, 12.0, 0.0), t::INK_DIM, 0.5);
        }
    }
    let r = detail_rect();
    let kind = index_of(host, pid(selected, "ModuleType"));
    let name = PARAMS[pid(selected, "ModuleType")].display_string(host.get(pid(selected, "ModuleType")) as f64);
    c.text(&format!("{:02} / {name}", selected + 1), r.x + 18.0, r.y + 32.0,
        &Style::new(Face::Semi, 21.0, 0.2), t::INK, 0.0);
    let on = index_of(host, pid(selected, "Bypass")) == 1 && index_of(host, a::FX_Enable) == 1;
    viz(c, host, selected, kind, viz_rect(selected), on,
        ui.page_state.eq_drag.filter(|d| d.0 == selected).map(|d| d.1));
    for (label, x, y) in captions(kind) { c.text(label, x, y, &t::SMALL, t::INK_DIM, 0.0); }
    let v = viz_rect(selected);
    let hint = if kind == 10 {
        "Drag band: frequency + gain     Scroll: Q     Double-click: reset gain"
    } else if kind == 0 {
        "Choose a processor in the selected slot above."
    } else {
        "Select a slot above to edit its processor. All three slots keep playing."
    };
    c.text(hint, v.x, v.bottom() + 23.0, &Style::new(Face::Regular, 10.5, 0.0), t::INK_DIM, 0.0);
    routing(c, host, R::new(v.x, r.bottom() - 28.0, v.w, 20.0));
}

fn routing(c: &mut Canvas, host: &dyn Host, r: R) {
    let parallel = index_of(host, a::FX_Routing) == 1;
    let enabled = index_of(host, a::FX_Enable) == 1;
    let st = Style::new(Face::CondBold, 9.0, 0.8);
    let bw = 46.0;
    let ink = |on: bool| if on && enabled { t::INK } else { t::INK_FAINT };
    let line = t::INK.with_alpha(if enabled { 0.35 } else { 0.12 });
    let box_at = |c: &mut Canvas, x: f32, y: f32, s: usize| {
        let on = index_of(host, pid(s, "Bypass")) == 1 && index_of(host, pid(s, "ModuleType")) > 0;
        let br = R::new(x, y, bw, 16.0);
        c.rrect(br, 3.0, if on && enabled { t::ORANGE.with_alpha(0.15) } else { t::INK.with_alpha(0.035) });
        c.rrect_stroke(br, 3.0, 0.75, if on && enabled { t::ORANGE } else { t::LINE });
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
    c.rrect(r, 4.0, t::DISPLAY_BG);
    c.rrect_stroke(r, 4.0, 1.0, t::LINE_STRONG);
}

fn grid_log(c: &mut Canvas, g: R) {
    for f in [100.0f32, 1000.0, 10000.0] {
        let x = g.x + g.w * (f / 20.0).log10() / 3.0;
        c.vline(x, g.y, g.bottom(), 1.0, t::DISPLAY_LINE);
        let label = if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") };
        c.text(&label, x + 5.0, g.bottom() - 6.0, &Style::new(Face::Mono, 10.0, 0.0), t::DISPLAY_DIM, 0.0);
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

fn synced_hz(host: &dyn Host, slot: usize, suffix: &str, feel: usize) -> f64 {
    let bars = jovian_core::fx::dsp::bars(&format!("FX{}_{suffix}", slot + 1));
    let index = index_of(host, pid(slot, suffix)).min(bars.len() - 1);
    1.0 / jovian_core::fx::dsp::sync_seconds(bars[index], feel, host.tempo_bpm())
}

fn viz(c: &mut Canvas, host: &dyn Host, s: usize, kind: usize, r: R, on: bool, eq_hot: Option<usize>) {
    frame(c, r);
    let g = r.inset_xy(10.0, 10.0);
    let col = if on { t::AMBER } else { t::AMBER_DIM };
    let small = Style::new(Face::Mono, 11.0, 0.0);
    match kind {
        0 => {
            c.text_in("Empty slot — choose an effect", r, &t::UI, t::DISPLAY_DIM, 0.5);
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
            c.text(&format!("RT60 ≈ {:.1} s", rt60), g.right(), g.y + 9.0, &small, t::DISPLAY_DIM, 1.0);
        }
        2 => {
            // delay: echo taps for both channels
            let synced = index_of(host, pid(s, "Delay_MidiSync")) == 1;
            let time = if synced {
                let bars = jovian_core::fx::dsp::bars(&format!("FX{}_Delay_DelayTimeSynced", s + 1));
                let i = index_of(host, pid(s, "Delay_DelayTimeSynced")).min(bars.len() - 1);
                jovian_core::fx::dsp::sync_seconds(bars[i], index_of(host, pid(s, "Delay_RateSyncedType")), host.tempo_bpm())
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
            c.text(&format!("{:.0} ms", time * 1000.0), g.right(), g.y + 9.0, &small, t::DISPLAY_DIM, 1.0);
        }
        3 | 4 | 5 | 11 => {
            // modulation effects: the LFO driving them
            let (rate, wave) = match kind {
                3 => (ph(host, s, "Chorus_Freq"), index_of(host, pid(s, "Chorus_LFOShape"))),
                4 => (ph(host, s, "Flanger_Chorus_Freq"), index_of(host, pid(s, "Flanger_Chorus_LFOShape"))),
                5 => {
                    let rate = if index_of(host, pid(s, "Phaser_LFO_MidiSync")) == 1 {
                        synced_hz(host, s, "Phaser_LFO_RateSynced", index_of(host, pid(s, "Phaser_LFO_RateSyncedType")))
                    } else { ph(host, s, "Phaser_LFO_RateUnSynced") };
                    (rate, index_of(host, pid(s, "Phaser_LFOWaveform")))
                }
                _ => {
                    let clock = index_of(host, pid(s, "StereoPan_LFOSimple_RateType"));
                    let rate = if clock == 0 { ph(host, s, "StereoPan_LFOSimple_RateUnSynced") }
                        else { synced_hz(host, s, "StereoPan_LFOSimple_RateSynced", clock - 1) };
                    (rate, 0)
                }
            };
            let depth = match kind {
                3 => ph(host, s, "Chorus_Depth") / 10.0,
                4 => ph(host, s, "Flanger_DepthFlanger") / 10.0,
                5 => ph(host, s, "Phaser_LFOAmount"),
                _ => ph(host, s, "StereoPan_Amount"),
            }
            .clamp(0.0, 1.0);
            let cycles = (rate * 2.0).clamp(1.0, 8.0);
            let n = 120;
            for (k, off) in [(0, 0.0), (1, 0.25)] {
                let mut pts = Vec::new();
                for i in 0..=n {
                    let u = i as f64 / n as f64;
                    let p = (u * cycles + off).fract();
                    let v = match wave {
                        0 => (p * std::f64::consts::TAU).sin(),
                        1 => jovian_core::fx::dsp::triangle(p as f32) as f64,
                        2 => 1.0 - 2.0 * p,
                        3 => 2.0 * p - 1.0,
                        4 => if p < 0.5 { 1.0 } else { -1.0 },
                        _ => [0.4, -0.7, 0.8, -0.2, 0.5, -0.9, 0.1, 0.6][((u * cycles + off).floor() as usize) % 8],
                    };
                    pts.push((g.x + g.w * u as f32, g.cy() - (v * depth) as f32 * g.h * 0.45));
                }
                curve(c, &pts, g.cy(), if k == 0 { col } else { col.mul_alpha(0.4) }, false);
            }
            c.text(&format!("{rate:.2} Hz"), g.right(), g.y + 9.0, &small, t::DISPLAY_DIM, 1.0);
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
            c.hline(g.x, g.right(), g.cy(), 1.0, t::DISPLAY_LINE);
            c.vline(g.cx(), g.y, g.bottom(), 1.0, t::DISPLAY_LINE);
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
                c.stroke_path(&p, 1.0, t::DISPLAY_LINE);
            }
            curve(c, &pts, gg.bottom(), col, false);
            let gr = host.compressor_reduction(s).clamp(0.0, 30.0);
            c.rrect(gr_box, 2.0, t::DISPLAY_LINE);
            let h = gr_box.h * gr / 30.0;
            c.rrect(R::new(gr_box.x, gr_box.y, gr_box.w, h), 2.0, t::RED);
            c.text("GR", gr_box.cx(), gr_box.bottom() + 0.0 - 2.0, &Style::new(Face::CondBold, 7.0, 0.3), t::DISPLAY_DIM, 0.5);
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
            c.text(&format!("{bits:.1} bit  ÷{down:.0}"), g.right(), g.y + 9.0, &small, t::DISPLAY_DIM, 1.0);
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
        c.text("BYPASSED", r.x + 10.0, r.y + 14.0, &Style::new(Face::CondBold, 9.0, 1.0), t::DISPLAY_DIM, 0.0);
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
    for db in [-12.0f32, 0.0, 12.0] {
        let y = g.cy() - db / EQ_RANGE_DB as f32 * g.h * 0.5;
        c.hline(g.x, g.right(), y, 1.0, t::DISPLAY_LINE);
        c.text(&format!("{db:+.0} dB"), g.x + 5.0, y - 5.0,
            &Style::new(Face::Mono, 10.0, 0.0), t::DISPLAY_DIM, 0.0);
    }
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
        c.circle(x, y, if on { 9.0 } else { 7.0 }, band_cols[b]);
        c.circle_stroke(x, y, 7.5, 1.0, t::DISPLAY_BG);
        c.text(&(b + 1).to_string(), x + 11.0, y - 9.0,
            &Style::new(Face::Mono, 10.5, 0.0), t::DISPLAY_INK, 0.0);
    }
}

/// Starts dragging the nearest EQ node.
pub fn area_down(ui: &mut Ui, host: &dyn Host, id: u32, r: R, x: f32, y: f32, double: bool) {
    if (SELECT_BASE..SELECT_BASE + 3).contains(&id) {
        let slot = (id - SELECT_BASE) as usize;
        if ui.page_state.fx_selected != slot {
            ui.cancel_interaction(host);
            ui.page_state.fx_selected = slot;
            ui.invalidate_background();
            ui.invalidate_all();
        }
        return;
    }
    if !(VIZ_BASE..VIZ_BASE + 3).contains(&id) { return; }
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
    if !(VIZ_BASE..VIZ_BASE + 3).contains(&id) { return; }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::MockHost;
    use crate::ui::{Button, Event};
    use jovian_core::preset::PatchState;

    fn select(ui: &mut Ui, host: &dyn Host, slot: usize) {
        let r = slot_rect(slot);
        ui.event(host, Event::Down { x: r.x + 30.0, y: r.y + 18.0, button: Button::Left, mods: Mods::default() });
        ui.event(host, Event::Up { x: r.x + 30.0, y: r.y + 18.0, button: Button::Left, mods: Mods::default() });
    }

    #[test]
    fn selecting_a_processor_preserves_the_sound_of_all_slots() {
        let host = MockHost::new(PatchState::default());
        let before = *host.values.borrow();
        let mut ui = Ui::new(Page::Fx, 100);
        ui.tick(&host);
        select(&mut ui, &host, 2);
        assert_eq!(ui.page_state.fx_selected, 2);
        assert_eq!(*host.values.borrow(), before);
    }

    #[test]
    fn graph_input_reaches_only_the_selected_equalizer() {
        let host = MockHost::new(PatchState::default());
        for slot in 0..3 {
            let addr = pid(slot, "ModuleType");
            host.set_once(addr, params::normalized_index(10, PARAMS[addr].item_count()) as f32);
        }
        let mut ui = Ui::new(Page::Fx, 100);
        ui.tick(&host);
        select(&mut ui, &host, 1);
        let graph = viz_rect(1);
        ui.event(&host, Event::Move { x: graph.cx(), y: graph.cy(), mods: Mods::default() });
        assert!(matches!(ui.hovered().map(|c| &c.kind), Some(Kind::Area(id)) if *id == VIZ_BASE + 1));
    }
}
