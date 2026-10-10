//! The MOD page: LFO 2 / LFO 3, the modulation matrix, keyboard response curves and voice
//! dispersion (the Jup-8 V additions to the Jupiter-8 panel).

use crate::canvas::{vgrad, Canvas, Fill, PathBuilder, Rgba, R};
use crate::host::{index_of, Host};
use crate::layout::{Deco, Kind, Layout, Page, BODY};
use crate::text::{Face, Style};
use crate::theme as t;
use crate::ui::{Mods, Ui};
use crate::widgets::{Icon, Item};
use jovian_core::dsp::modulation::{HybridLfoAddresses, CURVE_ADDRESSES, LFO2_ADDR, LFO3_ADDR, SLOT_LAYOUT};
use jovian_core::params::{self, addr as a, PARAMS};

pub const CURVE_BASE: u32 = 200; // 200...203
pub const LFO_PREVIEW_BASE: u32 = 210; // 210, 211

const P: Option<Page> = Some(Page::Mod);
const COL_A_W: f32 = 372.0;
const COL_C_W: f32 = 324.0;

fn col_a() -> R {
    R::new(BODY.x, BODY.y, COL_A_W, BODY.h)
}
fn col_b() -> R {
    R::new(BODY.x + COL_A_W + 8.0, BODY.y, BODY.w - COL_A_W - COL_C_W - 16.0, BODY.h)
}
fn col_c() -> R {
    R::new(BODY.right() - COL_C_W, BODY.y, COL_C_W, BODY.h)
}

fn lfo_rect(k: usize) -> R {
    let c = col_a();
    let h = (c.h - 8.0) / 2.0;
    R::new(c.x, c.y + k as f32 * (h + 8.0), c.w, h)
}

fn preview_rect(k: usize) -> R {
    let r = lfo_rect(k);
    R::new(r.x + 14.0, r.y + 38.0, 150.0, 74.0)
}

fn curves_rect() -> R {
    let c = col_c();
    R::new(c.x, c.y, c.w, 286.0)
}

fn dispersion_rect() -> R {
    let c = col_c();
    let cr = curves_rect();
    R::new(c.x, cr.bottom() + 8.0, c.w, c.bottom() - cr.bottom() - 8.0)
}

pub fn curve_box(k: usize) -> R {
    let r = curves_rect();
    let (w, h) = ((r.w - 14.0 * 2.0 - 12.0) / 2.0, 92.0);
    let col = (k % 2) as f32;
    let row = (k / 2) as f32;
    R::new(r.x + 14.0 + col * (w + 12.0), r.y + 52.0 + row * (h + 30.0), w, h)
}

const CURVE_NAMES: [&str; 4] = ["VELOCITY", "AFTERTOUCH", "KEY TRACK", "MOD WHEEL"];
const SOURCE_NAMES: [&str; 9] = ["LFO 2", "LFO 3", "MOD MIXER", "SEQ MOD", "VELOCITY", "AFTERTOUCH", "KEY TRACK", "MOD WHEEL", "MPE SLIDE"];
const SOURCE_COLORS: [Rgba; 9] = [t::YELLOW, t::YELLOW, t::INK_DIM, t::INK_DIM, t::RED, t::RED, t::BLUE, t::ORANGE, t::ORANGE];

/// Matrix rows that are played by the engine (Mod Mixer and the sequencer are pass-through).
fn matrix_rows() -> Vec<(u8, u8, usize)> {
    let mut v = Vec::new();
    for (src, dests) in SLOT_LAYOUT {
        if src == 3 || src == 4 {
            continue;
        }
        for (k, d) in dests.iter().enumerate() {
            v.push((src, *d, k));
        }
    }
    v
}

const MROW_H: f32 = 21.0;

fn matrix_row_rect(i: usize, group_gaps: usize) -> R {
    let b = col_b();
    R::new(b.x + 14.0, b.y + 52.0 + i as f32 * MROW_H + group_gaps as f32 * 6.0, b.w - 28.0, MROW_H - 3.0)
}

pub fn layout(l: &mut Layout) {
    lfo(l, 0, &LFO2_ADDR, "LFO 2");
    lfo(l, 1, &LFO3_ADDR, "LFO 3");

    // matrix
    l.deco(P, Deco::Section { r: col_b(), title: "MOD MATRIX", accent: t::ORANGE });
    let mut gaps = 0;
    let mut last = 0u8;
    for (i, (src, d, _k)) in matrix_rows().into_iter().enumerate() {
        if last != 0 && src != last {
            gaps += 1;
        }
        last = src;
        let r = matrix_row_rect(i, gaps);
        let dest = params::address(&format!("ModMatrixDest{d}")).unwrap();
        let amt = params::address(&format!("ModMatrixAmount_Src{src}_Dest{d}")).unwrap();
        l.push(P, R::new(r.x + 104.0, r.y, 150.0, r.h), Kind::Stepper { addr: dest });
        l.push(P, R::new(r.x + 262.0, r.y, r.w - 262.0 - 46.0, r.h), Kind::HSlider { addr: amt, color: SOURCE_COLORS[src as usize - 1], bipolar: true });
    }

    // keyboard curves
    l.deco(P, Deco::Section { r: curves_rect(), title: "KEYBOARD CURVES", accent: t::RED });
    for k in 0..4 {
        l.push(P, curve_box(k), Kind::Area(CURVE_BASE + k as u32));
    }

    // dispersion
    let d = dispersion_rect();
    l.deco(P, Deco::Section { r: d, title: "DISPERSION", accent: t::WHITE_KEY });
    l.push(
        P,
        R::new(d.x + 14.0, d.y + 38.0, d.w - 28.0, 20.0),
        Kind::Switch { addr: a::Condition, items: vec![Item::Text("1"), Item::Text("2"), Item::Text("3"), Item::Text("CUSTOM")], order: vec![0, 1, 2, 3], color: t::WHITE_KEY, vertical: false },
    );
    let rows = [
        (a::PitchDispersion, "PITCH"),
        (a::PWDispersion, "PULSE W."),
        (a::EnvDispersion, "ENVELOPES"),
        (a::CutoffDispersion, "CUTOFF"),
        (a::ResoDispersion, "RESONANCE"),
        (a::ModDispersion, "MOD DEPTH"),
    ];
    for (i, (addr, label)) in rows.iter().enumerate() {
        let col = (i % 2) as f32;
        let row = (i / 2) as f32;
        let cw = (d.w - 28.0 - 16.0) / 2.0;
        let x = d.x + 14.0 + col * (cw + 16.0);
        let y = d.y + 76.0 + row * 38.0;
        l.deco(P, Deco::Label { text: label, x, y: y + 4.0, align: 0.0 });
        l.push(P, R::new(x - 4.0, y + 8.0, cw + 8.0, 18.0), Kind::HSlider { addr: *addr, color: t::WHITE_KEY, bipolar: false });
    }
}

fn lfo(l: &mut Layout, k: usize, ad: &HybridLfoAddresses, title: &'static str) {
    let r = lfo_rect(k);
    l.deco(P, Deco::Section { r, title, accent: t::YELLOW });
    let pv = preview_rect(k);
    l.push(P, pv, Kind::Area(LFO_PREVIEW_BASE + k as u32));
    l.push(
        P,
        R::new(pv.x, pv.bottom() + 8.0, pv.w, 20.0),
        Kind::Switch {
            addr: ad.wave,
            items: vec![
                Item::Icon(Icon::Sine),
                Item::Icon(Icon::Triangle),
                Item::Icon(Icon::Saw),
                Item::Icon(Icon::RevSaw),
                Item::Icon(Icon::Square),
                Item::Icon(Icon::SampleHold),
            ],
            order: vec![0, 1, 2, 3, 4, 5],
            color: t::YELLOW,
            vertical: false,
        },
    );
    // knobs
    let kx = pv.right() + 14.0;
    let kw = (r.right() - 10.0 - kx) / 3.0;
    let knob = |l: &mut Layout, i: usize, row: usize, addr: usize, label: &'static str, bipolar: bool| {
        let cx = kx + kw * (i as f32 + 0.5);
        let y = r.y + 34.0 + row as f32 * 66.0;
        l.push(P, R::new(cx - kw * 0.5, y, kw, 60.0), Kind::Knob { addr, label, color: t::YELLOW, bipolar, radius: 14.0 });
    };
    knob(l, 0, 0, ad.rate, "RATE", false);
    knob(l, 1, 0, ad.fade, "FADE IN", false);
    knob(l, 2, 0, ad.warp, "WARP", true);
    knob(l, 0, 1, ad.phase, "PHASE", false);
    // clock / division
    let cy = r.y + 112.0;
    l.deco(P, Deco::Label { text: "DIVISION", x: kx + kw + 6.0, y: cy - 4.0, align: 0.0 });
    l.push(P, R::new(kx + kw + 4.0, cy + 2.0, kw * 2.0 - 8.0, 18.0), Kind::Stepper { addr: ad.rate_sync });
    // bottom rows
    let by = r.bottom() - 62.0;
    l.deco(P, Deco::Label { text: "CLOCK", x: r.x + 14.0, y: by - 4.0, align: 0.0 });
    l.push(
        P,
        R::new(r.x + 14.0, by + 2.0, 196.0, 18.0),
        Kind::Switch { addr: ad.rate_mode, items: vec![Item::Text("HZ"), Item::Text("SYNC"), Item::Text("TRIP"), Item::Text("DOT")], order: vec![0, 1, 2, 3], color: t::WHITE_KEY, vertical: false },
    );
    l.deco(P, Deco::Label { text: "MODE", x: r.x + 222.0, y: by - 4.0, align: 0.0 });
    l.push(
        P,
        R::new(r.x + 222.0, by + 2.0, r.right() - 14.0 - r.x - 222.0, 18.0),
        Kind::Switch { addr: ad.looped, items: vec![Item::Text("ONE SHOT"), Item::Text("LOOP")], order: vec![0, 1], color: t::WHITE_KEY, vertical: false },
    );
    let ty = r.bottom() - 30.0;
    let tw = (r.w - 28.0 - 12.0) / 3.0;
    l.push(P, R::new(r.x + 14.0, ty, tw, 18.0), Kind::Toggle { addr: ad.retrig, label: "KEY RETRIG", color: t::YELLOW });
    l.push(P, R::new(r.x + 14.0 + tw + 6.0, ty, tw, 18.0), Kind::Toggle { addr: ad.unipolar, label: "UNIPOLAR", color: t::YELLOW });
    l.push(P, R::new(r.x + 14.0 + 2.0 * (tw + 6.0), ty, tw, 18.0), Kind::Toggle { addr: ad.poly, label: "PER VOICE", color: t::YELLOW });
}

pub fn draw_background(ui: &Ui, c: &mut Canvas) {
    if ui.page != Page::Mod {
        return;
    }
    // matrix source labels and column captions
    let b = col_b();
    let st = Style::new(Face::CondSemi, 9.0, 1.1);
    c.text("SOURCE", b.x + 14.0, b.y + 44.0, &st, t::INK_DIM, 0.0);
    c.text("DESTINATION", b.x + 118.0, b.y + 44.0, &st, t::INK_DIM, 0.0);
    c.text("AMOUNT", b.x + 276.0, b.y + 44.0, &st, t::INK_DIM, 0.0);
    let mut gaps = 0;
    let mut last = 0u8;
    for (i, (src, _d, k)) in matrix_rows().into_iter().enumerate() {
        if last != 0 && src != last {
            gaps += 1;
            let r = matrix_row_rect(i, gaps);
            c.hline(r.x, r.right(), r.y - 4.5, 1.0, Rgba::white(0.06));
        }
        last = src;
        if k == 0 {
            let r = matrix_row_rect(i, gaps);
            let col = SOURCE_COLORS[src as usize - 1];
            c.circle(r.x + 4.0, r.cy(), 3.0, col);
            c.text_in(SOURCE_NAMES[src as usize - 1], R::new(r.x + 13.0, r.y, 90.0, r.h), &Style::new(Face::CondBold, 10.0, 0.8), t::INK, 0.0);
        }
    }
    c.text(
        "Mod Mixer and sequencer slots of Jup-8 V presets are kept but not played.",
        b.x + 14.0,
        b.bottom() - 14.0,
        &Style::new(Face::Regular, 10.0, 0.0),
        t::INK_FAINT,
        0.0,
    );
    // curve captions
    for (k, name) in CURVE_NAMES.iter().enumerate() {
        let r = curve_box(k);
        c.text(name, r.x, r.y - 7.0, &t::SMALL, t::INK_DIM, 0.0);
    }
}

fn display_box(c: &mut Canvas, r: R) {
    c.rrect(r, 5.0, vgrad(r.y, r.bottom(), Rgba::hex(0x060607), t::DISPLAY_BG));
    c.rrect_stroke(r, 5.0, 1.0, Rgba::black(0.9));
    for k in 1..4 {
        c.hline(r.x + 3.0, r.right() - 3.0, r.y + r.h * k as f32 / 4.0, 1.0, Rgba::white(0.035));
    }
    for k in 1..4 {
        c.vline(r.x + r.w * k as f32 / 4.0, r.y + 3.0, r.bottom() - 3.0, 1.0, Rgba::white(0.035));
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
    let wave = index_of(host, ad.wave);
    let warp = PARAMS[ad.warp].physical_f32(host.get(ad.warp));
    let phase0 = PARAMS[ad.phase].physical_f32(host.get(ad.phase)) / 360.0;
    let uni = host.get(ad.unipolar) >= 0.5;
    let single = index_of(host, ad.looped) == 0;
    let fade = PARAMS[ad.fade].physical_f32(host.get(ad.fade));
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
        // fade-in over the first part of the preview (2 cycles shown ~ 1 s)
        let fade_k = if fade > 0.0 { (u * 1.0 / fade.max(0.05)).min(1.0) } else { 1.0 };
        v *= fade_k;
        let y = if uni { g.bottom() - v as f32 * g.h } else { g.cy() - v as f32 * g.h * 0.5 };
        pts.push((g.x + g.w * u as f32, y));
    }
    let base = if uni { g.bottom() } else { g.cy() };
    c.hline(g.x, g.right(), base, 1.0, Rgba::white(0.12));
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
    c.text(&rate, r.right() - 6.0, r.y + 13.0, &Style::new(Face::Mono, 9.0, 0.0), t::AMBER_DIM.lighten(0.3), 1.0);
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
    if ui.page != Page::Mod {
        return;
    }
    draw_lfo_preview(c, host, preview_rect(0), &LFO2_ADDR);
    draw_lfo_preview(c, host, preview_rect(1), &LFO3_ADDR);
    // matrix amounts
    let mut gaps = 0;
    let mut last = 0u8;
    for (i, (src, d, _)) in matrix_rows().into_iter().enumerate() {
        if last != 0 && src != last {
            gaps += 1;
        }
        last = src;
        let r = matrix_row_rect(i, gaps);
        let amt = params::address(&format!("ModMatrixAmount_Src{src}_Dest{d}")).unwrap();
        let dest = params::address(&format!("ModMatrixDest{d}")).unwrap();
        let v = PARAMS[amt].physical_f32(host.get(amt));
        let active = index_of(host, dest) > 0 && v.abs() > 1e-4;
        let s = format!("{:+.0}%", v * 100.0);
        c.text_in(&s, R::new(r.right() - 42.0, r.y, 42.0, r.h), &Style::new(Face::Mono, 9.5, 0.0), if active { t::AMBER } else { t::INK_FAINT }, 1.0);
    }
    // curves
    for k in 0..4 {
        let r = curve_box(k);
        display_box(c, r);
        let p = curve_points(host, k, r);
        let g = r.inset(6.0);
        let mut pb = PathBuilder::new();
        pb.move_to(p[0].0, g.bottom());
        for q in p {
            pb.line_to(q.0, q.1);
        }
        pb.line_to(p[2].0, g.bottom());
        pb.close();
        if let Some(path) = pb.finish() {
            c.fill_path(&path, Fill::Solid(t::AMBER.with_alpha(0.1)));
        }
        let mut pb = PathBuilder::new();
        pb.move_to(p[0].0, p[0].1);
        pb.line_to(p[1].0, p[1].1);
        pb.line_to(p[2].0, p[2].1);
        if let Some(path) = pb.finish() {
            c.stroke_path(&path, 1.5, t::AMBER);
        }
        let hot = ui.page_state.curve_drag.filter(|(kk, _)| *kk == k).map(|(_, i)| i);
        let hover_k = r.contains(ui.mouse.0, ui.mouse.1);
        for (i, q) in p.iter().enumerate() {
            let on = hot == Some(i);
            c.circle(q.0, q.1, if on { 5.0 } else { 4.0 }, if on { t::INK } else { t::AMBER });
            c.circle_stroke(q.0, q.1, 4.5, 1.0, Rgba::black(0.6));
            let _ = hover_k;
        }
    }
}

pub fn area_down(ui: &mut Ui, host: &dyn Host, id: u32, r: R, x: f32, y: f32, double: bool) {
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
