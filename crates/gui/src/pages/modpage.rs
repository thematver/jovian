//! Source-focused modulation: select a source, shape it, then set its destinations.

use crate::canvas::{Canvas, Fill, PathBuilder, Rgba, R};
use crate::host::{index_of, Host};
use crate::layout::{Kind, Layout, Page, BODY};
use crate::text::{Face, Style};
use crate::theme as t;
use crate::ui::{Mods, Ui};
use crate::widgets::{Icon, Item};
use jovian_core::dsp::modulation::{HybridLfoAddresses, HybridLfoSettings, CURVE_ADDRESSES, LFO2_ADDR, LFO3_ADDR, SLOT_LAYOUT};
use jovian_core::params::{self, addr as a, PARAMS};

pub const CURVE_BASE: u32 = 200;
pub const SOURCE_BASE: u32 = 220;
pub const SOURCE_COUNT: usize = 10;
const P: Option<Page> = Some(Page::Mod);
const SOURCE_NAMES: [&str; SOURCE_COUNT] = ["LFO 1", "LFO 2", "LFO 3", "Velocity", "Aftertouch", "Key track", "Mod wheel", "MPE slide", "Engine", "Voice drift"];
const SOURCE_NOTES: [&str; SOURCE_COUNT] = ["Classic motion", "Shape & retrigger", "Shape & retrigger", "Touch response", "Pressure response", "Across the keys", "Performance control", "Per-note expression", "Routing & detail", "Analog variation"];
const SOURCE_COLORS: [Rgba; SOURCE_COUNT] = [t::YELLOW, t::YELLOW, t::YELLOW, t::RED, t::RED, t::BLUE, t::ORANGE, t::ORANGE, t::INK_DIM, t::BLUE];

fn palette() -> R { R::new(BODY.x, BODY.y, 202.0, BODY.h) }
fn editor() -> R { R::new(BODY.x + 218.0, BODY.y, 700.0, BODY.h) }
fn routes() -> R { R::new(BODY.x + 934.0, BODY.y, BODY.w - 934.0, BODY.h) }
fn source_row(k: usize) -> R {
    let p = palette();
    R::new(p.x + 10.0, p.y + 57.0 + k as f32 * 46.0, p.w - 20.0, 40.0)
}
fn preview_rect() -> R {
    let e = editor();
    R::new(e.x + 24.0, e.y + 92.0, e.w - 48.0, 214.0)
}
/// The selected response curve has the same large workspace for every performance source.
pub fn curve_box(_k: usize) -> R {
    let e = editor();
    R::new(e.x + 24.0, e.y + 112.0, e.w - 48.0, 286.0)
}
fn route_row(k: usize) -> R {
    let r = routes();
    R::new(r.x + 20.0, r.y + 111.0 + k as f32 * 128.0, r.w - 40.0, 108.0)
}

fn push(l: &mut Layout, view: usize, r: R, kind: Kind) {
    l.push(P, r, kind);
    l.controls.last_mut().unwrap().view = Some(view);
}
fn knob(l: &mut Layout, view: usize, r: R, addr: usize, label: &'static str, color: Rgba, bipolar: bool) {
    push(l, view, r, Kind::Knob { addr, label, color, bipolar, radius: 23.0 });
}
fn toggle(l: &mut Layout, view: usize, r: R, addr: usize, label: &'static str, color: Rgba) {
    push(l, view, r, Kind::Toggle { addr, label, color });
}
fn select(l: &mut Layout, view: usize, r: R, addr: usize) {
    let items = PARAMS[addr].items().iter().map(|s| Item::Text(*s)).collect();
    let order = (0..PARAMS[addr].item_count()).collect();
    push(l, view, r, Kind::Switch { addr, items, order, color: SOURCE_COLORS[view], vertical: false });
}
fn dest_slots(view: usize) -> Vec<(u8, u8)> {
    let src = match view { 1 => 1, 2 => 2, 3 => 5, 4 => 6, 5 => 7, 6 => 8, 7 => 9, _ => return Vec::new() };
    SLOT_LAYOUT.iter().find(|(s, _)| *s == src).map(|(_, dests)| dests.iter().map(|d| (src, *d)).collect()).unwrap_or_default()
}
fn slot_addresses(src: u8, dest: u8) -> (usize, usize) {
    (params::address(&format!("ModMatrixDest{dest}")).unwrap(), params::address(&format!("ModMatrixAmount_Src{src}_Dest{dest}")).unwrap())
}

pub fn layout(l: &mut Layout) {
    for k in 0..SOURCE_COUNT {
        l.push(P, source_row(k), Kind::Area(SOURCE_BASE + k as u32));
    }
    classic_lfo(l);
    hybrid_lfo(l, 1, &LFO2_ADDR);
    hybrid_lfo(l, 2, &LFO3_ADDR);
    for k in 0..4 { push(l, 3 + k, curve_box(k), Kind::Area(CURVE_BASE + k as u32)); }
    for view in 1..=7 {
        for (k, (src, dest)) in dest_slots(view).into_iter().enumerate() {
            let r = route_row(k);
            let (da, amt) = slot_addresses(src, dest);
            push(l, view, R::new(r.x, r.y + 23.0, r.w, 30.0), Kind::Stepper { addr: da });
            push(l, view, R::new(r.x - 6.0, r.y + 69.0, r.w - 55.0, 24.0), Kind::HSlider { addr: amt, color: SOURCE_COLORS[view], bipolar: true });
        }
    }
    engine_details(l);
    voice_drift(l);
}

fn classic_lfo(l: &mut Layout) {
    let e = editor();
    let pv = preview_rect();
    push(l, 0, R::new(pv.x, e.y + 326.0, 348.0, 30.0), Kind::Switch {
        addr: a::LFO_Waveform,
        items: vec![Item::Icon(Icon::Sine), Item::Icon(Icon::RevSaw), Item::Icon(Icon::Square), Item::Icon(Icon::SampleHold)],
        order: vec![0, 1, 2, 3], color: t::YELLOW, vertical: false,
    });
    select(l, 0, R::new(e.x + 400.0, e.y + 326.0, 276.0, 30.0), a::LFO_MidiSync);
    knob(l, 0, R::new(e.x + 38.0, e.y + 385.0, 118.0, 100.0), a::LFO_RateUnSynced, "RATE", t::YELLOW, false);
    l.controls.last_mut().unwrap().when = Some((a::LFO_MidiSync, 0));
    knob(l, 0, R::new(e.x + 188.0, e.y + 385.0, 118.0, 100.0), a::LFO_FadeDelay, "DELAY", t::YELLOW, false);
    push(l, 0, R::new(e.x + 24.0, e.y + 420.0, 150.0, 30.0), Kind::Stepper { addr: a::LFO_RateSynced });
    l.controls.last_mut().unwrap().when = Some((a::LFO_MidiSync, 1));
    toggle(l, 0, R::new(e.x + 24.0, e.y + 521.0, 182.0, 28.0), a::LFO_Retrig, "KEY RETRIGGER", t::YELLOW);
    for (k, (addr, label)) in [(a::OscLfoAmount, "PITCH DEPTH"), (a::LfoFiltAmount, "FILTER DEPTH"), (a::VCA_LfoAmount, "AMPLITUDE")].into_iter().enumerate() {
        let r = route_row(k);
        knob(l, 0, R::new(r.x - 2.0, r.y + 24.0, 116.0, 90.0), addr, label, t::YELLOW, false);
    }
    let r = route_row(0);
    push(l, 0, R::new(r.x + 130.0, r.y + 52.0, r.w - 130.0, 30.0), Kind::Stepper { addr: a::OscDestSelect });
}

fn hybrid_lfo(l: &mut Layout, view: usize, ad: &HybridLfoAddresses) {
    let e = editor();
    push(l, view, R::new(e.x + 24.0, e.y + 326.0, 348.0, 30.0), Kind::Switch {
        addr: ad.wave,
        items: vec![Item::Icon(Icon::Sine), Item::Icon(Icon::Triangle), Item::Icon(Icon::Saw), Item::Icon(Icon::RevSaw), Item::Icon(Icon::Square), Item::Icon(Icon::SampleHold)],
        order: vec![0, 1, 2, 3, 4, 5], color: t::YELLOW, vertical: false,
    });
    select(l, view, R::new(e.x + 400.0, e.y + 326.0, 276.0, 30.0), ad.rate_mode);
    for (k, (addr, label, bipolar)) in [(ad.rate, "RATE", false), (ad.fade, "FADE IN", false), (ad.warp, "WARP", true), (ad.phase, "PHASE", false)].into_iter().enumerate() {
        knob(l, view, R::new(e.x + 24.0 + k as f32 * 166.0, e.y + 379.0, 150.0, 100.0), addr, label, t::YELLOW, bipolar);
        if k == 0 { l.controls.last_mut().unwrap().when = Some((ad.rate_mode, 0)); }
    }
    for mode in 1..=3 {
        push(l, view, R::new(e.x + 24.0, e.y + 414.0, 150.0, 30.0), Kind::Stepper { addr: ad.rate_sync });
        l.controls.last_mut().unwrap().when = Some((ad.rate_mode, mode));
    }
    select(l, view, R::new(e.x + 202.0, e.y + 483.0, 210.0, 28.0), ad.looped);
    for (k, (addr, label)) in [(ad.retrig, "KEY RETRIGGER"), (ad.unipolar, "UNIPOLAR"), (ad.poly, "PER VOICE")].into_iter().enumerate() {
        toggle(l, view, R::new(e.x + 24.0 + k as f32 * 222.0, e.y + 521.0, 208.0, 28.0), addr, label, t::YELLOW);
    }
}

fn engine_details(l: &mut Layout) {
    let e = editor();
    let view = 8;
    knob(l, view, R::new(e.x + 24.0, e.y + 91.0, 135.0, 100.0), a::MasterTune, "MASTER TUNE", t::WHITE_KEY, true);
    push(l, view, R::new(e.x + 195.0, e.y + 129.0, 170.0, 30.0), Kind::Stepper { addr: a::UniVPN });
    select(l, view, R::new(e.x + 412.0, e.y + 129.0, 264.0, 30.0), a::PlPrio);
    for (k, (addr, label)) in [(a::OscLfoAmount, "PITCH LFO"), (a::OscEnvAmount, "PITCH ENV 1"), (a::LfoFiltAmount, "FILTER LFO"), (a::VCA_LfoAmount, "AMP LFO")].into_iter().enumerate() {
        knob(l, view, R::new(e.x + 24.0 + k as f32 * 166.0, e.y + 230.0, 150.0, 100.0), addr, label, t::YELLOW, false);
    }
    push(l, view, R::new(e.x + 24.0, e.y + 367.0, 300.0, 30.0), Kind::Stepper { addr: a::OscDestSelect });
    push(l, view, R::new(e.x + 376.0, e.y + 367.0, 300.0, 30.0), Kind::Stepper { addr: a::OscPwmMode });
    knob(l, view, R::new(e.x + 24.0, e.y + 433.0, 142.0, 100.0), a::VCA_EnvAmount, "AMP ENV DEPTH", t::BLUE, false);
    knob(l, view, R::new(e.x + 184.0, e.y + 433.0, 142.0, 100.0), a::LFO_FadeDelay, "LFO DELAY", t::YELLOW, false);
    toggle(l, view, R::new(e.x + 376.0, e.y + 439.0, 142.0, 28.0), a::Env1_KeyFollow, "ENV 1 KEY FOLLOW", t::RED);
    toggle(l, view, R::new(e.x + 534.0, e.y + 439.0, 142.0, 28.0), a::Env1_Polarity, "ENV 1 INVERT", t::RED);
    toggle(l, view, R::new(e.x + 376.0, e.y + 478.0, 142.0, 28.0), a::Env2_KeyFollow, "ENV 2 KEY FOLLOW", t::RED);
    toggle(l, view, R::new(e.x + 534.0, e.y + 478.0, 142.0, 28.0), a::LFO_Retrig, "LFO KEY RETRIG", t::YELLOW);

    let r = routes();
    for (k, (addr, label)) in [(a::BendVCO1, "OSC 1"), (a::BendVCO2, "OSC 2"), (a::BendVCF, "FILTER")].into_iter().enumerate() {
        toggle(l, view, R::new(r.x + 20.0 + k as f32 * 106.0, r.y + 119.0, 102.0, 28.0), addr, label, t::WHITE_KEY);
    }
    push(l, view, R::new(r.x + 20.0, r.y + 184.0, 148.0, 30.0), Kind::Stepper { addr: a::BendVCOAmount });
    push(l, view, R::new(r.x + 186.0, r.y + 184.0, r.w - 206.0, 30.0), Kind::HSlider { addr: a::BendVCFAmount, color: t::BLUE, bipolar: false });
    toggle(l, view, R::new(r.x + 20.0, r.y + 277.0, 142.0, 28.0), a::LfoModVCO, "TO PITCH", t::YELLOW);
    toggle(l, view, R::new(r.x + 186.0, r.y + 277.0, 142.0, 28.0), a::LfoModVCF, "TO FILTER", t::BLUE);
    knob(l, view, R::new(r.x + 20.0, r.y + 328.0, 142.0, 100.0), a::LfoModVCOAmount, "PITCH DEPTH", t::YELLOW, false);
    knob(l, view, R::new(r.x + 186.0, r.y + 328.0, 142.0, 100.0), a::LfoModVcfAmount, "FILTER DEPTH", t::BLUE, false);
}

fn voice_drift(l: &mut Layout) {
    let e = editor();
    select(l, 9, R::new(e.x + 24.0, e.y + 105.0, e.w - 48.0, 32.0), a::Condition);
    for (k, (addr, label)) in [(a::PitchDispersion, "PITCH"), (a::PWDispersion, "PULSE WIDTH"), (a::EnvDispersion, "ENVELOPES"), (a::CutoffDispersion, "CUTOFF"), (a::ResoDispersion, "RESONANCE"), (a::ModDispersion, "MOD DEPTH")].into_iter().enumerate() {
        knob(l, 9, R::new(e.x + 24.0 + (k % 3) as f32 * 222.0, e.y + 194.0 + (k / 3) as f32 * 174.0, 208.0, 116.0), addr, label, t::BLUE, false);
    }
}

pub fn draw_background(ui: &Ui, c: &mut Canvas) {
    if ui.page != Page::Mod { return; }
    for r in [palette(), editor(), routes()] {
        c.rrect(r, 5.0, t::PANEL_TOP);
        c.rrect_stroke(r, 5.0, 1.0, t::LINE);
    }
    let p = palette();
    c.text("SOURCES", p.x + 18.0, p.y + 31.0, &Style::new(Face::Semi, 11.0, 1.0), t::INK_DIM, 0.0);
    c.hline(p.x + 18.0, p.right() - 18.0, p.y + 43.0, 1.0, t::LINE);
    c.text("SELECT → SHAPE → ROUTE", p.x + 18.0, p.bottom() - 18.0, &Style::new(Face::CondSemi, 9.0, 0.4), t::INK_FAINT, 0.0);
}

fn title(c: &mut Canvas, r: R, title: &str, subtitle: &str, color: Rgba) {
    c.circle(r.x + 25.0, r.y + 28.0, 3.5, color);
    c.text(title, r.x + 38.0, r.y + 39.0, &Style::new(Face::Semi, 28.0, -0.3), t::INK, 0.0);
    c.text(subtitle, r.x + 24.0, r.y + 66.0, &Style::new(Face::Regular, 12.0, 0.0), t::INK_DIM, 0.0);
}

fn label(c: &mut Canvas, text: &str, x: f32, y: f32) {
    c.text(text, x, y, &Style::new(Face::Semi, 10.0, 0.65), t::INK_DIM, 0.0);
}

fn display_box(c: &mut Canvas, r: R) {
    c.rrect(r, 4.0, t::DISPLAY_BG);
    c.rrect_stroke(r, 4.0, 1.0, t::LINE_STRONG);
    for k in 1..4 {
        c.hline(r.x + 3.0, r.right() - 3.0, r.y + r.h * k as f32 / 4.0, 1.0, t::DISPLAY_LINE);
    }
    for k in 1..4 {
        c.vline(r.x + r.w * k as f32 / 4.0, r.y + 3.0, r.bottom() - 3.0, 1.0, t::DISPLAY_LINE);
    }
}

/// Value of the hybrid LFO shape at phase `ph` (same as the engine).
fn lfo_shape(wave: usize, ph: f64, warp: f64, held: &[f32]) -> f64 {
    let w = warp.clamp(-1.0, 1.0);
    let mut p = ph.min(0.999_999);
    if w > 0.0 {
        p = p.powf(1.0 + 3.0 * w);
    } else if w < 0.0 {
        p = 1.0 - (1.0 - p).powf(1.0 - 3.0 * w);
    }
    match wave {
        0 => (2.0 * std::f64::consts::PI * p).sin(),
        1 => {
            if p < 0.25 {
                4.0 * p
            } else if p < 0.75 {
                2.0 - 4.0 * p
            } else {
                4.0 * p - 4.0
            }
        }
        2 => 2.0 * p - 1.0,
        3 => 1.0 - 2.0 * p,
        4 => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        _ => held[((ph * held.len() as f64) as usize).min(held.len() - 1)] as f64,
    }
}

fn draw_lfo_preview(c: &mut Canvas, host: &dyn Host, r: R, ad: &HybridLfoAddresses) {
    display_box(c, r);
    // Use the engine's conversion so clock division, triplets, dots and fade time agree.
    let values: params::Values = std::array::from_fn(|addr| host.get(addr));
    let mut settings = HybridLfoSettings::default();
    settings.update(&values, ad, host.tempo_bpm());
    let (wave, warp, phase0) = (settings.wave, settings.warp, settings.phase0);
    let (uni, single, fade) = (settings.unipolar, settings.single, settings.fade_sec);
    let held = [0.4f32, -0.8, 0.7, -0.2, 0.9, -0.5, 0.1, -0.9];
    let g = r.inset_xy(8.0, 8.0);
    let cycles = 2.0;
    let n = 120;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let u = i as f64 / n as f64;
        let t_cyc = u * cycles;
        let (ph, done) = if single && t_cyc + phase0 >= 1.0 { (1.0, true) } else { (((t_cyc + phase0) % 1.0), false) };
        let mut v = lfo_shape(wave, ph, warp, &held);
        if done && wave >= 2 {
            v = lfo_shape(wave, 0.999_999, warp, &held);
        }
        if uni {
            v = 0.5 * v + 0.5;
        }
        // Two cycles have a different duration at every rate and transport tempo.
        let elapsed = t_cyc / settings.hz.max(1e-6);
        let fade_k = if fade > 0.0 { (elapsed / fade).min(1.0) } else { 1.0 };
        v *= fade_k;
        let y = if uni { g.bottom() - v as f32 * g.h } else { g.cy() - v as f32 * g.h * 0.5 };
        pts.push((g.x + g.w * u as f32, y));
    }
    let base = if uni { g.bottom() } else { g.cy() };
    c.hline(g.x, g.right(), base, 1.0, t::DISPLAY_LINE);
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, base);
    for p in &pts {
        pb.line_to(p.0, p.1);
    }
    pb.line_to(pts[n].0, base);
    pb.close();
    if let Some(p) = pb.finish() {
        c.fill_path(&p, Fill::Solid(t::AMBER.with_alpha(0.12)));
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        pb.line_to(p.0, p.1);
    }
    if let Some(p) = pb.finish() {
        c.stroke_path(&p, 1.5, t::AMBER);
    }
    // rate readout
    let mode = index_of(host, ad.rate_mode);
    let rate = if mode == 0 {
        PARAMS[ad.rate].display_string(host.get(ad.rate) as f64)
    } else {
        let div = PARAMS[ad.rate_sync].display_string(host.get(ad.rate_sync) as f64);
        format!("{div}{}", ["", "", "T", "."][mode])
    };
    c.text(&rate, r.right() - 6.0, r.y + 13.0, &Style::new(Face::Mono, 9.0, 0.0), t::DISPLAY_INK, 1.0);
}

/// Points of a keyboard curve in box coordinates: (0, y0), (x1, y1), (1, y2).
fn curve_points(host: &dyn Host, k: usize, r: R) -> [(f32, f32); 3] {
    let ad = CURVE_ADDRESSES[k];
    let g = r.inset(6.0);
    let px = |x: f32| g.x + x * g.w;
    let py = |y: f32| g.bottom() - y * g.h;
    [(px(0.0), py(host.get(ad[0]))), (px(host.get(ad[1])), py(host.get(ad[2]))), (px(1.0), py(host.get(ad[3])))]
}


pub fn draw(ui: &Ui, host: &dyn Host, c: &mut Canvas) {
    if ui.page != Page::Mod { return; }
    let view = ui.page_state.mod_source.min(SOURCE_COUNT - 1);
    for k in 0..SOURCE_COUNT {
        let r = source_row(k);
        let selected = view == k;
        let hovered = r.contains(ui.mouse.0, ui.mouse.1);
        if selected || hovered { c.rrect(r, 3.0, if selected { t::INK } else { t::WELL }); }
        c.circle(r.x + 12.0, r.y + 14.0, 2.8, SOURCE_COLORS[k]);
        c.text(SOURCE_NAMES[k], r.x + 24.0, r.y + 18.0, &Style::new(Face::Semi, 13.0, 0.0), if selected { t::PANEL_TOP } else { t::INK }, 0.0);
        c.text(SOURCE_NOTES[k], r.x + 24.0, r.y + 33.0, &Style::new(Face::Regular, 9.5, 0.0), if selected { t::DISPLAY_DIM } else { t::INK_DIM }, 0.0);
    }
    let e = editor();
    let rr = routes();
    title(c, e, SOURCE_NAMES[view], match view {
        0 => "Classic modulation for pitch, filter and amplitude.",
        1 | 2 => "Design the movement, then choose where it goes.",
        3 => "Shape how your touch becomes modulation.",
        4 => "Shape the pressure response after a note is held.",
        5 => "Shape modulation across the keyboard.",
        6 => "Shape the wheel response for expressive performance.",
        7 => "Per-note slide from an MPE controller.",
        8 => "Voice behavior, classic routing and performance detail.",
        _ => "Small differences that give each voice its own character.",
    }, SOURCE_COLORS[view]);
    if view <= 7 {
        title(c, rr, "Destinations", if view == 0 { "Classic signal paths" } else { "One source, several connections" }, SOURCE_COLORS[view]);
    }
    match view {
        0 => {
            let r = preview_rect();
            display_box(c, r);
            crate::display::lfo(c, r.inset_xy(18.0, 18.0), index_of(host, a::LFO_Waveform));
            let rate_addr = if index_of(host, a::LFO_MidiSync) == 0 { a::LFO_RateUnSynced } else { a::LFO_RateSynced };
            c.text(&PARAMS[rate_addr].display_string(host.get(rate_addr) as f64), r.right() - 14.0, r.y + 23.0, &Style::new(Face::Mono, 12.0, 0.0), t::DISPLAY_INK, 1.0);
            label(c, "WAVEFORM", e.x + 24.0, e.y + 318.0);
            label(c, "CLOCK", e.x + 400.0, e.y + 318.0);
            if index_of(host, a::LFO_MidiSync) == 1 { label(c, "HOST DIVISION", e.x + 24.0, e.y + 408.0); }
            label(c, "NOTE-ON BEHAVIOR", e.x + 24.0, e.y + 510.0);
            for (k, name) in ["OSCILLATORS", "FILTER", "AMPLIFIER"].iter().enumerate() {
                let r = route_row(k);
                label(c, &format!("0{}  {name}", k + 1), r.x, r.y + 12.0);
                if k < 2 { c.hline(r.x, r.right(), r.bottom() + 9.0, 1.0, t::LINE); }
            }
        }
        1 | 2 => {
            let ad = if view == 1 { &LFO2_ADDR } else { &LFO3_ADDR };
            draw_lfo_preview(c, host, preview_rect(), ad);
            label(c, "WAVEFORM", e.x + 24.0, e.y + 318.0);
            label(c, "CLOCK", e.x + 400.0, e.y + 318.0);
            if index_of(host, ad.rate_mode) != 0 { label(c, "HOST DIVISION", e.x + 24.0, e.y + 400.0); }
        }
        3..=6 => {
            let k = view - 3;
            draw_response_curve(ui, host, c, k);
            label(c, "DRAG A POINT TO SHAPE THE RESPONSE", e.x + 24.0, e.y + 100.0);
            label(c, "INPUT", e.x + 24.0, e.y + 424.0);
            c.text(match view { 3 => "Soft → hard touch", 4 => "Light → firm pressure", 5 => "Low → high notes", _ => "Wheel down → wheel up" }, e.x + 24.0, e.y + 449.0, &Style::new(Face::Regular, 14.0, 0.0), t::INK, 0.0);
            label(c, "OUTPUT", e.x + 384.0, e.y + 424.0);
            c.text("Modulation depth", e.x + 384.0, e.y + 449.0, &Style::new(Face::Regular, 14.0, 0.0), t::INK, 0.0);
            c.text("Double-click the curve to restore a linear response.", e.x + 24.0, e.y + 507.0, &Style::new(Face::Regular, 12.0, 0.0), t::INK_DIM, 0.0);
        }
        7 => {
            let r = curve_box(0);
            display_box(c, r);
            let g = r.inset(26.0);
            c.hline(g.x, g.right(), g.cy(), 1.0, t::DISPLAY_LINE);
            c.line(g.x, g.bottom(), g.right(), g.y, 2.0, t::AMBER);
            c.circle(g.x, g.bottom(), 5.0, t::AMBER);
            c.circle(g.right(), g.y, 5.0, t::AMBER);
            label(c, "INPUT RANGE  0 — 127", e.x + 24.0, e.y + 424.0);
            c.text("Each held note carries its own slide expression.", e.x + 24.0, e.y + 458.0, &Style::new(Face::Regular, 15.0, 0.0), t::INK, 0.0);
            c.text("Choose a destination to hear it respond.", e.x + 24.0, e.y + 484.0, &Style::new(Face::Regular, 12.0, 0.0), t::INK_DIM, 0.0);
        }
        8 => draw_engine_labels(c),
        _ => draw_drift_labels(c),
    }
    if (1..=7).contains(&view) {
        for (k, (src, dest)) in dest_slots(view).into_iter().enumerate() {
            let r = route_row(k);
            let (da, amt) = slot_addresses(src, dest);
            label(c, &format!("CONNECTION 0{}", k + 1), r.x, r.y + 12.0);
            let active = index_of(host, da) > 0;
            c.text(&format!("{:+.0}%", PARAMS[amt].physical_f32(host.get(amt))), r.right(), r.y + 86.0, &Style::new(Face::Mono, 12.0, 0.0), if active { SOURCE_COLORS[view] } else { t::INK_FAINT }, 1.0);
            c.hline(r.x, r.right(), r.bottom() + 9.0, 1.0, t::LINE);
        }
        c.text("Select a destination, then set its depth.", rr.x + 20.0, rr.bottom() - 25.0, &Style::new(Face::Regular, 11.0, 0.0), t::INK_DIM, 0.0);
    }
}

fn draw_engine_labels(c: &mut Canvas) {
    let e = editor();
    let r = routes();
    title(c, r, "Performance", "Bend and modulation lever routing", t::INK_DIM);
    label(c, "UNISON VOICES", e.x + 195.0, e.y + 116.0);
    label(c, "NOTE PRIORITY", e.x + 412.0, e.y + 116.0);
    label(c, "CLASSIC MODULATION DEPTH", e.x + 24.0, e.y + 211.0);
    c.hline(e.x + 24.0, e.right() - 24.0, e.y + 192.0, 1.0, t::LINE);
    label(c, "PITCH MOD DESTINATION", e.x + 24.0, e.y + 354.0);
    label(c, "PULSE WIDTH SOURCE", e.x + 376.0, e.y + 354.0);
    label(c, "BEND DESTINATIONS", r.x + 20.0, r.y + 106.0);
    label(c, "PITCH RANGE", r.x + 20.0, r.y + 171.0);
    label(c, "FILTER DEPTH", r.x + 186.0, r.y + 171.0);
    c.hline(r.x + 20.0, r.right() - 20.0, r.y + 240.0, 1.0, t::LINE);
    label(c, "MOD LEVER / LFO", r.x + 20.0, r.y + 264.0);
}

fn draw_drift_labels(c: &mut Canvas) {
    let e = editor();
    let r = routes();
    label(c, "CONDITION", e.x + 24.0, e.y + 93.0);
    title(c, r, "Voice character", "Variation without changing your patch", t::BLUE);
    let lines = ["Pitch & pulse width", "Subtle oscillator differences.", "Envelopes", "Slightly different timing per voice.", "Filter & resonance", "A less uniform tone across chords.", "Modulation depth", "Small differences in movement."];
    for (k, line) in lines.iter().enumerate() {
        c.text(line, r.x + 24.0, r.y + 123.0 + k as f32 * 34.0, &Style::new(if k % 2 == 0 { Face::Semi } else { Face::Regular }, if k % 2 == 0 { 14.0 } else { 11.0 }, 0.0), if k % 2 == 0 { t::INK } else { t::INK_DIM }, 0.0);
    }
}

fn draw_response_curve(ui: &Ui, host: &dyn Host, c: &mut Canvas, k: usize) {
    let r = curve_box(k);
    display_box(c, r);
    let p = curve_points(host, k, r);
    let g = r.inset(6.0);
    let mut pb = PathBuilder::new();
    pb.move_to(p[0].0, g.bottom());
    for q in p { pb.line_to(q.0, q.1); }
    pb.line_to(p[2].0, g.bottom());
    pb.close();
    if let Some(path) = pb.finish() { c.fill_path(&path, Fill::Solid(t::AMBER.with_alpha(0.1))); }
    let mut pb = PathBuilder::new();
    pb.move_to(p[0].0, p[0].1);
    pb.line_to(p[1].0, p[1].1);
    pb.line_to(p[2].0, p[2].1);
    if let Some(path) = pb.finish() { c.stroke_path(&path, 2.0, t::AMBER); }
    let hot = ui.page_state.curve_drag.filter(|(kk, _)| *kk == k).map(|(_, i)| i);
    for (i, q) in p.iter().enumerate() {
        c.circle(q.0, q.1, if hot == Some(i) { 7.0 } else { 6.0 }, if hot == Some(i) { t::DISPLAY_INK } else { t::AMBER });
        c.circle_stroke(q.0, q.1, 6.5, 1.2, t::DISPLAY_BG);
    }
}

pub fn area_down(ui: &mut Ui, host: &dyn Host, id: u32, r: R, x: f32, y: f32, double: bool) {
    if (SOURCE_BASE..SOURCE_BASE + SOURCE_COUNT as u32).contains(&id) {
        ui.cancel_interaction(host);
        ui.page_state.mod_source = (id - SOURCE_BASE) as usize;
        ui.invalidate_background();
        return;
    }
    if !(CURVE_BASE..CURVE_BASE + 4).contains(&id) {
        return;
    }
    let k = (id - CURVE_BASE) as usize;
    let ad = CURVE_ADDRESSES[k];
    if double {
        for addr in ad {
            host.set_once(addr, PARAMS[addr].default as f32);
        }
        return;
    }
    let p = curve_points(host, k, r);
    let (i, _) = p.iter().enumerate().map(|(i, q)| (i, (q.0 - x).hypot(q.1 - y))).fold((0, f32::MAX), |b, c| if c.1 < b.1 { c } else { b });
    ui.page_state.curve_drag = Some((k, i));
    match i {
        0 => host.begin(ad[0]),
        1 => {
            host.begin(ad[1]);
            host.begin(ad[2]);
        }
        _ => host.begin(ad[3]),
    }
    ui.focus = Some((if i == 0 { ad[0] } else if i == 1 { ad[2] } else { ad[3] }, std::time::Instant::now()));
    area_drag(ui, host, id, x, y, Mods::default());
}

pub fn area_drag(ui: &mut Ui, host: &dyn Host, id: u32, x: f32, y: f32, _mods: Mods) {
    let Some((k, i)) = ui.page_state.curve_drag else { return };
    if id != CURVE_BASE + k as u32 {
        return;
    }
    let g = curve_box(k).inset(6.0);
    let ad = CURVE_ADDRESSES[k];
    let nx = ((x - g.x) / g.w).clamp(0.0, 1.0);
    let ny = ((g.bottom() - y) / g.h).clamp(0.0, 1.0);
    match i {
        0 => host.set(ad[0], ny),
        1 => {
            host.set(ad[1], nx);
            host.set(ad[2], ny);
        }
        _ => host.set(ad[3], ny),
    }
}

pub fn area_up(ui: &mut Ui, host: &dyn Host) {
    if let Some((k, i)) = ui.page_state.curve_drag.take() {
        let ad = CURVE_ADDRESSES[k];
        match i {
            0 => host.end(ad[0]),
            1 => {
                host.end(ad[1]);
                host.end(ad[2]);
            }
            _ => host.end(ad[3]),
        }
    }
}
