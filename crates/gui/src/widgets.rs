//! Drawing of the individual controls. Static parts (slots, scales, captions) go to the cached
//! background layer; the rest is drawn every frame.

use crate::canvas::{hgrad, vgrad, Canvas, Fill, PathBuilder, Rgba, R};
use crate::text::{Face, Style};
use crate::theme as t;

/// Icon shapes for waveform and other buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Icon {
    Sine,
    Saw,
    RevSaw,
    Square,
    Pulse,
    Triangle,
    Noise,
    SampleHold,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Text(&'static str),
    Icon(Icon),
}

pub const FADER_CAP_W: f32 = 24.0;
pub const FADER_CAP_H: f32 = 12.0;
/// Space under a fader for its label.
pub const FADER_LABEL_H: f32 = 22.0;

/// Geometry of a fader inside its column rectangle.
pub struct FaderGeom {
    pub cx: f32,
    pub top: f32,
    pub bottom: f32,
}

pub fn fader_geom(r: R) -> FaderGeom {
    FaderGeom { cx: r.cx(), top: r.y + FADER_CAP_H * 0.5 + 2.0, bottom: r.bottom() - FADER_LABEL_H - FADER_CAP_H * 0.5 }
}

pub fn fader_cap_y(r: R, v: f32) -> f32 {
    let g = fader_geom(r);
    g.bottom - v.clamp(0.0, 1.0) * (g.bottom - g.top)
}

/// Slot and scale of a fader (background layer).
pub fn fader_static(c: &mut Canvas, r: R, origin: f32) {
    let g = fader_geom(r);
    // scale ticks on both sides, longer at 0, 5 and 10
    for i in 0..=10 {
        let y = g.bottom - (g.bottom - g.top) * i as f32 / 10.0;
        let major = i % 5 == 0;
        let len = if major { 5.0 } else { 3.0 };
        let col = if major { Rgba::white(0.34) } else { Rgba::white(0.17) };
        c.hline(g.cx - 8.0 - len, g.cx - 8.0, y, 1.0, col);
        c.hline(g.cx + 8.0, g.cx + 8.0 + len, y, 1.0, col);
    }
    if origin > 0.0 {
        let y = g.bottom - (g.bottom - g.top) * origin;
        c.hline(g.cx - 16.0, g.cx - 8.0, y, 1.0, Rgba::white(0.55));
        c.hline(g.cx + 8.0, g.cx + 16.0, y, 1.0, Rgba::white(0.55));
    }
    // slot
    let slot = R::new(g.cx - 2.5, g.top - 4.0, 5.0, g.bottom - g.top + 8.0);
    c.rrect(slot.inset(-1.0), 3.5, Rgba::white(0.06));
    c.rrect(slot, 2.5, vgrad(slot.y, slot.bottom(), Rgba::hex(0x050506), Rgba::hex(0x0C0C0E)));
}

/// Moving parts of a fader: value trace, cap and the label / value line.
#[allow(clippy::too_many_arguments)]
pub fn fader(c: &mut Canvas, r: R, v: f32, color: Rgba, origin: f32, label: &str, value: Option<&str>, hover: bool, active: bool) {
    let g = fader_geom(r);
    let y = fader_cap_y(r, v);
    // value trace inside the slot
    let from = if origin > 0.0 { g.bottom - (g.bottom - g.top) * origin } else { g.bottom + 2.0 };
    let (a, b) = if y < from { (y, from) } else { (from, y) };
    if b - a > 0.5 {
        c.rrect(R::new(g.cx - 1.25, a, 2.5, b - a), 1.25, color.with_alpha(if hover || active { 1.0 } else { 0.85 }));
        c.rrect(R::new(g.cx - 3.5, a, 7.0, b - a), 3.5, color.with_alpha(0.14));
    }
    // cap
    let cap = R::new(g.cx - FADER_CAP_W * 0.5, y - FADER_CAP_H * 0.5, FADER_CAP_W, FADER_CAP_H);
    c.shadow(cap, 2.5, 3.0, 2.0, Rgba::black(0.75));
    let (top, bot) = if active {
        (Rgba::hex(0xFFFFFF), Rgba::hex(0xD6D5CF))
    } else if hover {
        (Rgba::hex(0xFAFAF7), Rgba::hex(0xCDCCC6))
    } else {
        (Rgba::hex(0xEDECE8), Rgba::hex(0xB9B8B2))
    };
    c.rrect(cap, 2.5, vgrad(cap.y, cap.bottom(), top, bot));
    c.rrect_stroke(cap, 2.5, 0.75, Rgba::black(0.55));
    c.hline(cap.x + 2.0, cap.right() - 2.0, cap.y + 1.0, 1.0, Rgba::white(0.8));
    c.rect(R::new(cap.x + 1.5, y - 0.9, cap.w - 3.0, 1.8), Rgba::hex(0x1A1A1A));
    // label / value
    let lr = R::new(r.x - 6.0, r.bottom() - FADER_LABEL_H + 6.0, r.w + 12.0, 12.0);
    match value {
        Some(vs) if hover || active => {
            c.text_in(vs, lr, &t::VALUE, t::AMBER, 0.5);
        }
        _ => {
            c.text_in(label, lr, &t::LABEL, if hover { t::INK } else { t::INK.with_alpha(0.86) }, 0.5);
        }
    }
}

/// Item rectangles of a switch (vertical stack or horizontal row) inside `r`.
pub fn switch_items(r: R, n: usize, vertical: bool, gap: f32) -> Vec<R> {
    let mut out = Vec::with_capacity(n);
    if n == 0 {
        return out;
    }
    if vertical {
        let h = (r.h - gap * (n as f32 - 1.0)) / n as f32;
        for i in 0..n {
            out.push(R::new(r.x, r.y + i as f32 * (h + gap), r.w, h));
        }
    } else {
        let w = (r.w - gap * (n as f32 - 1.0)) / n as f32;
        for i in 0..n {
            out.push(R::new(r.x + i as f32 * (w + gap), r.y, w, r.h));
        }
    }
    out
}

/// One lit button of a switch / toggle.
pub fn button(c: &mut Canvas, r: R, item: &Item, color: Rgba, on: bool, hover: bool, small: bool) {
    let rad = 3.0;
    c.shadow(r, rad, 2.0, 1.0, Rgba::black(0.55));
    if on {
        let top = color.lighten(0.12);
        let bot = color.darken(0.08);
        c.rrect(r, rad, vgrad(r.y, r.bottom(), top, bot));
        c.rrect(r.inset(-0.5), rad + 0.5, Fill::Solid(color.with_alpha(0.0)));
        c.hline(r.x + 2.0, r.right() - 2.0, r.y + 0.75, 1.0, Rgba::white(0.45));
        c.rrect_stroke(r, rad, 0.75, Rgba::black(0.35));
    } else {
        let base = color.mix(t::PANEL_BOTTOM, 0.8);
        let top = base.lighten(if hover { 0.1 } else { 0.05 });
        c.rrect(r, rad, vgrad(r.y, r.bottom(), top, base.darken(0.15)));
        c.hline(r.x + 2.0, r.right() - 2.0, r.y + 0.75, 1.0, Rgba::white(0.08));
        c.rrect_stroke(r, rad, 0.75, Rgba::black(0.6));
    }
    let ink = if on { t::INK_DARK } else { color.lighten(0.35).with_alpha(if hover { 0.95 } else { 0.72 }) };
    match item {
        Item::Text(s) => {
            let st = if small { Style::new(Face::CondBold, 8.5, 0.5) } else { t::BUTTON };
            c.text_in(s, r.offset(0.0, 0.25), &st, ink, 0.5);
        }
        Item::Icon(i) => icon(c, *i, R::new(r.cx() - 9.0, r.cy() - 4.5, 18.0, 9.0), ink, 1.4),
    }
}

/// Small round LED.
pub fn led(c: &mut Canvas, cx: f32, cy: f32, on: f32) {
    if on > 0.01 {
        c.glow(cx, cy, 7.0, t::LED.with_alpha(0.55 * on));
    }
    c.circle(cx, cy, 2.6, Rgba::black(0.7));
    let col = Rgba::hex(0x3A1512).mix(t::LED, on);
    c.circle(cx, cy, 2.0, Fill::Radial(cx - 0.6, cy - 0.6, 2.6, vec![(0.0, col.lighten(0.4 * on)), (1.0, col)]));
}

/// Waveform icons drawn as strokes inside `r`.
pub fn icon(c: &mut Canvas, i: Icon, r: R, col: Rgba, w: f32) {
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    let ym = r.cy();
    let mut pb = PathBuilder::new();
    match i {
        Icon::Sine => {
            let n = 24;
            for k in 0..=n {
                let u = k as f32 / n as f32;
                let x = x0 + u * r.w;
                let y = ym - (u * std::f32::consts::TAU).sin() * r.h * 0.5;
                if k == 0 {
                    pb.move_to(x, y)
                } else {
                    pb.line_to(x, y)
                }
            }
        }
        Icon::Saw => {
            pb.move_to(x0, y1);
            pb.line_to(r.cx(), y0);
            pb.line_to(r.cx(), y1);
            pb.line_to(x1, y0);
            pb.line_to(x1, ym);
        }
        Icon::RevSaw => {
            pb.move_to(x0, y0);
            pb.line_to(r.cx(), y1);
            pb.line_to(r.cx(), y0);
            pb.line_to(x1, y1);
        }
        Icon::Square => {
            pb.move_to(x0, ym);
            pb.line_to(x0, y0);
            pb.line_to(r.cx(), y0);
            pb.line_to(r.cx(), y1);
            pb.line_to(x1, y1);
            pb.line_to(x1, ym);
        }
        Icon::Pulse => {
            let a = x0 + r.w * 0.28;
            pb.move_to(x0, y1);
            pb.line_to(x0, y0);
            pb.line_to(a, y0);
            pb.line_to(a, y1);
            pb.line_to(x1 - 1.0, y1);
            pb.line_to(x1 - 1.0, y0);
        }
        Icon::Triangle => {
            pb.move_to(x0, ym);
            pb.line_to(x0 + r.w * 0.25, y0);
            pb.line_to(x0 + r.w * 0.75, y1);
            pb.line_to(x1, ym);
        }
        Icon::Noise => {
            let ys = [0.5, 0.1, 0.8, 0.3, 0.95, 0.05, 0.65, 0.2, 0.9, 0.4, 0.6, 0.15, 0.7];
            for (k, f) in ys.iter().enumerate() {
                let x = x0 + r.w * k as f32 / (ys.len() - 1) as f32;
                let y = y0 + r.h * f;
                if k == 0 {
                    pb.move_to(x, y)
                } else {
                    pb.line_to(x, y)
                }
            }
        }
        Icon::SampleHold => {
            let ys = [0.6, 0.15, 0.85, 0.35, 0.7];
            let step = r.w / ys.len() as f32;
            pb.move_to(x0, y0 + r.h * ys[0]);
            for (k, f) in ys.iter().enumerate() {
                let y = y0 + r.h * f;
                if k > 0 {
                    pb.line_to(x0 + k as f32 * step, y);
                }
                pb.line_to(x0 + (k + 1) as f32 * step, y);
            }
        }
    }
    if let Some(p) = pb.finish() {
        c.stroke_path(&p, w, col);
    }
}

pub const KNOB_START: f32 = 0.75 * std::f32::consts::PI; // 135°
pub const KNOB_SWEEP: f32 = 1.5 * std::f32::consts::PI; // 270°

fn arc(pb: &mut PathBuilder, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, start: bool) {
    let n = (((a1 - a0).abs() / 0.12).ceil() as usize).max(1);
    for k in 0..=n {
        let a = a0 + (a1 - a0) * k as f32 / n as f32;
        let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
        if k == 0 && start {
            pb.move_to(x, y)
        } else {
            pb.line_to(x, y)
        }
    }
}

/// Static ring of a knob.
pub fn knob_static(c: &mut Canvas, cx: f32, cy: f32, rad: f32) {
    let mut pb = PathBuilder::new();
    arc(&mut pb, cx, cy, rad + 4.0, KNOB_START, KNOB_START + KNOB_SWEEP, true);
    if let Some(p) = pb.finish() {
        c.stroke_path(&p, 2.5, Rgba::black(0.55));
    }
    // ticks at both ends and centre
    for k in [0.0f32, 0.5, 1.0] {
        let a = KNOB_START + KNOB_SWEEP * k;
        c.line(cx + (rad + 7.0) * a.cos(), cy + (rad + 7.0) * a.sin(), cx + (rad + 9.5) * a.cos(), cy + (rad + 9.5) * a.sin(), 1.0, Rgba::white(0.3));
    }
}

/// Knob with value arc.
#[allow(clippy::too_many_arguments)]
pub fn knob(c: &mut Canvas, cx: f32, cy: f32, rad: f32, v: f32, color: Rgba, bipolar: bool, hover: bool, active: bool) {
    let v = v.clamp(0.0, 1.0);
    let a0 = if bipolar { KNOB_START + KNOB_SWEEP * 0.5 } else { KNOB_START };
    let a1 = KNOB_START + KNOB_SWEEP * v;
    if (a1 - a0).abs() > 0.01 {
        let mut pb = PathBuilder::new();
        let (s, e) = if a1 > a0 { (a0, a1) } else { (a1, a0) };
        arc(&mut pb, cx, cy, rad + 4.0, s, e, true);
        if let Some(p) = pb.finish() {
            c.stroke_path(&p, 5.0, color.with_alpha(0.18));
            c.stroke_path(&p, 2.2, color);
        }
    }
    // body
    c.shadow(R::new(cx - rad, cy - rad, rad * 2.0, rad * 2.0), rad, 3.0, 2.0, Rgba::black(0.7));
    let lit = if active { 0.1 } else if hover { 0.05 } else { 0.0 };
    c.circle(cx, cy, rad, Fill::Linear(cx, cy - rad, cx, cy + rad, vec![(0.0, Rgba::hex(0x48484E).lighten(lit)), (1.0, Rgba::hex(0x1E1E21).lighten(lit))]));
    c.circle(cx, cy, rad - 2.5, Fill::Linear(cx, cy - rad, cx, cy + rad, vec![(0.0, Rgba::hex(0x2C2C30).lighten(lit)), (1.0, Rgba::hex(0x37373C).lighten(lit))]));
    c.circle_stroke(cx, cy, rad, 0.75, Rgba::black(0.8));
    // pointer
    let a = a1;
    let (ca, sa) = (a.cos(), a.sin());
    c.line(cx + ca * rad * 0.25, cy + sa * rad * 0.25, cx + ca * (rad - 3.0), cy + sa * (rad - 3.0), 2.0, Rgba::hex(0xF2F0EA));
}

/// A recessed value box (steppers, menus).
pub fn value_box(c: &mut Canvas, r: R, text: &str, hover: bool, arrows: bool) {
    c.rrect(r, 3.0, vgrad(r.y, r.bottom(), Rgba::hex(0x0A0A0B), Rgba::hex(0x121214)));
    c.rrect_stroke(r, 3.0, 0.75, if hover { Rgba::white(0.28) } else { Rgba::white(0.12) });
    let inner = if arrows { r.inset_xy(14.0, 0.0) } else { r.inset_xy(6.0, 0.0) };
    c.text_in(text, inner, &Style::new(Face::Mono, 10.0, 0.0), t::AMBER, 0.5);
    if arrows {
        let col = if hover { t::INK } else { t::INK_DIM };
        chevron(c, r.x + 7.0, r.cy(), -1.0, col);
        chevron(c, r.right() - 7.0, r.cy(), 1.0, col);
    }
}

pub fn chevron(c: &mut Canvas, x: f32, y: f32, dir: f32, col: Rgba) {
    let mut pb = PathBuilder::new();
    pb.move_to(x - 1.5 * dir, y - 3.0);
    pb.line_to(x + 1.5 * dir, y);
    pb.line_to(x - 1.5 * dir, y + 3.0);
    if let Some(p) = pb.finish() {
        c.stroke_path(&p, 1.4, col);
    }
}

pub fn caret_down(c: &mut Canvas, x: f32, y: f32, col: Rgba) {
    let mut pb = PathBuilder::new();
    pb.move_to(x - 3.0, y - 1.5);
    pb.line_to(x, y + 1.5);
    pb.line_to(x + 3.0, y - 1.5);
    if let Some(p) = pb.finish() {
        c.stroke_path(&p, 1.4, col);
    }
}

/// Section frame with title (background layer).
pub fn section(c: &mut Canvas, r: R, title: &str, accent: Rgba) {
    c.shadow(r, 6.0, 6.0, 3.0, Rgba::black(0.45));
    c.rrect(r, 6.0, vgrad(r.y, r.bottom(), Rgba::hex(0x252529), Rgba::hex(0x1D1D20)));
    c.rrect_stroke(r, 6.0, 1.0, Rgba::black(0.65));
    c.rrect_stroke(r.inset(1.0), 5.0, 1.0, Rgba::white(0.045));
    // title strip
    let tx = r.x + 12.0;
    let ty = r.y + 17.0;
    c.rrect(R::new(tx, ty - 8.5, 3.0, 9.5), 1.0, accent);
    c.text(title, tx + 8.0, ty, &t::TITLE, t::INK, 0.0);
    let w = c.fonts.measure(title, &t::TITLE);
    c.hline(tx + 8.0 + w + 8.0, r.right() - 12.0, ty - 4.0, 1.0, Rgba::white(0.1));
}

/// Small caption (background layer), centred above a control.
pub fn caption(c: &mut Canvas, s: &str, cx: f32, y: f32) {
    c.text(s, cx, y, &t::SMALL, t::INK_DIM, 0.5);
}

/// Thin bracket line above a group of controls with its caption.
pub fn bracket(c: &mut Canvas, x0: f32, x1: f32, y: f32, s: &str) {
    let w = c.fonts.measure(s, &t::SMALL) + 10.0;
    let cx = (x0 + x1) * 0.5;
    c.hline(x0, cx - w * 0.5, y, 1.0, Rgba::white(0.18));
    c.hline(cx + w * 0.5, x1, y, 1.0, Rgba::white(0.18));
    c.vline(x0, y, y + 4.0, 1.0, Rgba::white(0.18));
    c.vline(x1 - 1.0, y, y + 4.0, 1.0, Rgba::white(0.18));
    c.text(s, cx, y + 3.5, &t::SMALL, t::INK_DIM, 0.5);
}

/// Tab of the page bar.
pub fn tab(c: &mut Canvas, r: R, label: &str, on: bool, hover: bool) {
    if on {
        c.rrect(r, 4.0, vgrad(r.y, r.bottom(), Rgba::hex(0x3A3A40), Rgba::hex(0x2C2C31)));
        c.hline(r.x + 3.0, r.right() - 3.0, r.y + 0.75, 1.0, Rgba::white(0.12));
        c.rrect(R::new(r.cx() - 9.0, r.bottom() - 3.0, 18.0, 2.0), 1.0, t::ORANGE);
    } else if hover {
        c.rrect(r, 4.0, Rgba::white(0.05));
    }
    let st = Style::new(Face::CondBold, 11.0, 1.4);
    c.text_in(label, r.offset(0.0, -0.5), &st, if on { t::INK } else if hover { t::INK.with_alpha(0.8) } else { t::INK_DIM }, 0.5);
}

/// Generic flat text button.
pub fn text_button(c: &mut Canvas, r: R, label: &str, hover: bool, primary: bool) {
    let (a, b) = if primary { (t::ORANGE.lighten(0.08), t::ORANGE.darken(0.08)) } else { (Rgba::hex(0x34343A), Rgba::hex(0x2A2A2F)) };
    c.shadow(r, 4.0, 2.0, 1.0, Rgba::black(0.5));
    c.rrect(r, 4.0, vgrad(r.y, r.bottom(), if hover { a.lighten(0.08) } else { a }, b));
    c.rrect_stroke(r, 4.0, 0.75, Rgba::black(0.5));
    c.hline(r.x + 3.0, r.right() - 3.0, r.y + 0.75, 1.0, Rgba::white(if primary { 0.35 } else { 0.1 }));
    let st = Style::new(Face::Semi, 11.0, 0.3);
    c.text_in(label, r, &st, if primary { t::INK_DARK } else { t::INK }, 0.5);
}

/// Horizontal bipolar/unipolar slider (matrix amounts, browser).
#[allow(clippy::too_many_arguments)]
pub fn hslider(c: &mut Canvas, r: R, v: f32, color: Rgba, bipolar: bool, hover: bool, active: bool) {
    let track = R::new(r.x, r.cy() - 2.0, r.w, 4.0);
    c.rrect(track, 2.0, Rgba::hex(0x0B0B0C));
    let x = r.x + r.w * v.clamp(0.0, 1.0);
    let from = if bipolar { r.cx() } else { r.x };
    let (a, b) = if x < from { (x, from) } else { (from, x) };
    if b - a > 0.5 {
        c.rrect(R::new(a, track.y + 0.5, b - a, 3.0), 1.5, color);
    }
    if bipolar {
        c.vline(r.cx(), r.cy() - 4.0, r.cy() + 4.0, 1.0, Rgba::white(0.3));
    }
    let k = R::new(x - 5.0, r.cy() - 5.0, 10.0, 10.0);
    c.shadow(k, 5.0, 2.0, 1.0, Rgba::black(0.6));
    c.circle(x, r.cy(), 5.0, vgrad(k.y, k.bottom(), Rgba::hex(0xF4F3EF), Rgba::hex(0xBDBCB6)));
    if hover || active {
        c.circle_stroke(x, r.cy(), 6.5, 1.0, color.with_alpha(0.7));
    }
}

/// Horizontal gradient strip of the Jupiter colours (logo accent).
pub fn rainbow(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
    let cols = [t::RED, t::ORANGE, t::YELLOW, t::BLUE];
    let bw = (w - 3.0 * 2.0) / 4.0;
    for (i, col) in cols.iter().enumerate() {
        let r = R::new(x + i as f32 * (bw + 2.0), y, bw, h);
        c.rrect(r, 1.0, vgrad(r.y, r.bottom(), col.lighten(0.1), col.darken(0.1)));
    }
}

pub fn hgrad_fill(x0: f32, x1: f32, a: Rgba, b: Rgba) -> Fill {
    hgrad(x0, x1, a, b)
}
