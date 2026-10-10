//! The main instrument view: the sound path, parameter previews, and graph gestures.
//! These are parameter/model previews; no waveform or spectrum is presented as live audio.

use crate::canvas::{Canvas, PathBuilder, Rgba, R};
use crate::display;
use crate::host::{index_of, Host};
use crate::layout::{Kind, Layout, Page};
use crate::text::{Face, Style};
use crate::theme as t;
use crate::ui::{Mods, Ui};
use jovian_core::dsp::voice_math as vm;
use jovian_core::params::{addr as a, PARAMS};
use std::time::Instant;

pub const FILTER: u32 = 400;
pub const TONE_ENVELOPE: u32 = 401;
pub const AMP_ENVELOPE: u32 = 402;
pub const FILTER_RECT: R = R::new(376.0, 164.0, 524.0, 106.0);
pub const TONE_ENV_RECT: R = R::new(376.0, 420.0, 244.0, 72.0);
pub const AMP_ENV_RECT: R = R::new(948.0, 164.0, 338.0, 86.0);
const OSC1_RECT: R = R::new(34.0, 176.0, 132.0, 56.0);
const OSC2_RECT: R = R::new(34.0, 284.0, 132.0, 56.0);
const ENV1: [usize; 4] = [a::Env1_Attack, a::Env1_Decay, a::Env1_Sustain, a::Env1_Release];
const ENV2: [usize; 4] = [a::Env2_Attack, a::Env2_Decay, a::Env2_Sustain, a::Env2Release];
const GRAPH: Rgba = Rgba::hex(0x21302F);
const GRAPH_LINE: Rgba = Rgba::rgba(0.75, 0.86, 0.82, 0.13);
const ENV_COLOR: Rgba = Rgba::hex(0xBBD4B4);

/// Addresses and geometry are captured at mouse-down. Host automation can change the envelope
/// selector during a drag without redirecting the gesture or leaving the original one open.
#[derive(Debug, Clone, Copy)]
pub enum Drag {
    Filter { start: (f32, f32), cutoff: f32, resonance: f32 },
    Envelope {
        id: u32,
        addresses: [usize; 4],
        node: usize,
        start: (f32, f32),
        physical: [f64; 4],
        pixels_per_weight: f32,
        plot_height: f32,
    },
}

pub fn layout(l: &mut Layout) {
    for (id, r) in [(FILTER, FILTER_RECT), (TONE_ENVELOPE, TONE_ENV_RECT), (AMP_ENVELOPE, AMP_ENV_RECT)] {
        l.push(Some(Page::Synth), r, Kind::Area(id));
    }
}

fn stage(c: &mut Canvas, number: &str, title: &str, x: f32, right: f32) {
    c.rrect(R::new(x, 95.0, 24.0, 18.0), 3.0, t::INK);
    c.text_in(number, R::new(x, 95.0, 24.0, 18.0), &Style::new(Face::MonoSemi, 9.0, 0.0), t::PANEL_TOP, 0.5);
    c.text(title, x + 34.0, 108.0, &Style::new(Face::Bold, 12.0, 1.0), t::INK, 0.0);
    if right < 1304.0 {
        c.hline(right - 21.0, right - 2.0, 104.0, 1.0, t::INK_DIM);
        c.line(right - 7.0, 100.0, right - 2.0, 104.0, 1.0, t::INK_DIM);
        c.line(right - 7.0, 108.0, right - 2.0, 104.0, 1.0, t::INK_DIM);
    }
}

/// Called before layout decorations, so card surfaces never paint over control labels.
pub fn draw_background(ui: &Ui, c: &mut Canvas) {
    if ui.page != Page::Synth { return; }
    for r in [R::new(16.0, 122.0, 330.0, 398.0), R::new(358.0, 122.0, 560.0, 398.0), R::new(930.0, 122.0, 374.0, 398.0),
        R::new(16.0, 532.0, 648.0, 116.0), R::new(676.0, 532.0, 628.0, 116.0)] {
        c.rrect(r, 7.0, t::PANEL_TOP);
        c.rrect_stroke(r, 7.0, 1.0, t::LINE);
    }
    stage(c, "01", "SOURCE", 16.0, 346.0);
    stage(c, "02", "TONE", 358.0, 918.0);
    stage(c, "03", "VOICE", 930.0, 1304.0);
    c.text("OSCILLATOR 1", 34.0, 161.0, &Style::new(Face::Semi, 10.0, 0.65), t::INK, 0.0);
    c.text("OSCILLATOR 2", 34.0, 269.0, &Style::new(Face::Semi, 10.0, 0.65), t::INK, 0.0);
    c.hline(34.0, 328.0, 251.0, 1.0, t::LINE);
    c.hline(34.0, 328.0, 361.0, 1.0, t::LINE);
    c.hline(948.0, 1286.0, 334.0, 1.0, t::LINE);
    c.text("FILTER SHAPE", 376.0, 148.0, &Style::new(Face::Semi, 10.0, 0.65), t::INK, 0.0);
    c.text("DRAG FREQUENCY / RESONANCE", 900.0, 148.0, &Style::new(Face::Mono, 8.0, 0.0), t::INK_DIM, 1.0);
    c.text("ENV 2 / AMPLITUDE", 948.0, 148.0, &Style::new(Face::Semi, 10.0, 0.65), t::INK, 0.0);
    c.text("DRAG NODES", 1286.0, 148.0, &Style::new(Face::Mono, 8.0, 0.0), t::INK_DIM, 1.0);
    c.text("MOTION", 34.0, 553.0, &Style::new(Face::Bold, 10.0, 0.8), t::INK, 0.0);
    c.text("ARPEGGIATOR", 694.0, 553.0, &Style::new(Face::Bold, 10.0, 0.8), t::INK, 0.0);
}

pub fn draw(ui: &Ui, host: &dyn Host, c: &mut Canvas) {
    if ui.page != Page::Synth { return; }
    waveform(c, OSC1_RECT, 0, index_of(host, a::Osc1Wave), host.get(a::OscPwmAmount));
    waveform(c, OSC2_RECT, 1, index_of(host, a::Osc2Wave), host.get(a::OscPwmAmount));
    for (addr, r) in [(a::Osc1Wave, OSC1_RECT), (a::Osc2Wave, OSC2_RECT)] {
        let name = PARAMS[addr].display_string(host.get(addr) as f64).to_uppercase();
        c.text(&name, r.x, r.bottom() + 12.0, &Style::new(Face::Mono, 8.0, 0.3), t::INK_DIM, 0.0);
        c.text("SHAPE", r.right(), r.bottom() + 12.0, &Style::new(Face::Mono, 7.5, 0.0), t::INK_FAINT, 1.0);
    }
    filter(ui, host, c);
    let tone_addresses = envelope_addresses(host, TONE_ENVELOPE);
    let number = if tone_addresses == ENV1 { "1" } else { "2" };
    c.text(&format!("ENV {number} / FILTER CONTOUR"), 376.0, 404.0, &Style::new(Face::Semi, 9.0, 0.4), t::INK, 0.0);
    if tone_addresses == ENV1 && index_of(host, a::Env1_Polarity) != 0 {
        c.text("INVERTED", 620.0, 404.0, &Style::new(Face::Mono, 7.5, 0.0), t::ORANGE, 1.0);
    } else {
        c.text("DRAG", 620.0, 404.0, &Style::new(Face::Mono, 7.5, 0.0), t::INK_DIM, 1.0);
    }
    envelope(ui, host, c, TONE_ENVELOPE, TONE_ENV_RECT, tone_addresses);
    envelope(ui, host, c, AMP_ENVELOPE, AMP_ENV_RECT, ENV2);
}

fn waveform(c: &mut Canvas, r: R, oscillator: usize, wave: usize, pwm: f32) {
    c.rrect(r, 4.0, GRAPH);
    let g = r.inset_xy(10.0, 10.0);
    c.hline(g.x, g.right(), g.cy(), 1.0, GRAPH_LINE);
    for i in 1..4 { c.vline(g.x + i as f32 * g.w / 4.0, g.y, g.bottom(), 1.0, GRAPH_LINE); }
    let duty = vm::pulse_duty(pwm as f64) as f32;
    let noise = [0.4, -0.8, 0.1, 0.65, -0.3, 0.2, -0.65, 0.9, -0.1, -0.5, 0.55, -0.9, 0.3, 0.6, -0.2, 0.1];
    let mut pb = PathBuilder::new();
    for i in 0..=128 {
        let u = i as f32 / 128.0;
        let ph = (u * 2.0).fract();
        let v = match (oscillator, wave) {
            (0, 0) => 1.0 - 4.0 * (ph - 0.5).abs(),
            (1, 0) => (ph * std::f32::consts::TAU).sin(),
            (_, 1) => 2.0 * ph - 1.0,
            (_, 2) => if ph < duty { 1.0 } else { -1.0 },
            (0, _) => if ph < 0.5 { 1.0 } else { -1.0 },
            _ => noise[(i / 8).min(noise.len() - 1)],
        };
        let (x, y) = (g.x + u * g.w, g.cy() - v * g.h * 0.43);
        if i == 0 { pb.move_to(x, y); } else { pb.line_to(x, y); }
    }
    if let Some(p) = pb.finish() { c.stroke_path(&p, 1.6, ENV_COLOR); }
}

fn filter_plot() -> R { R::new(FILTER_RECT.x + 14.0, FILTER_RECT.y + 16.0, FILTER_RECT.w - 28.0, FILTER_RECT.h - 38.0) }

fn filter_values_at(x: f32, y: f32) -> (f32, f32) {
    let g = filter_plot();
    let hz = 20.0 * 1000.0f64.powf(((x - g.x) / g.w).clamp(0.0, 1.0) as f64);
    (vm::cutoff_normalized(hz).clamp(0.0, 1.0) as f32, ((g.bottom() - y) / g.h).clamp(0.0, 1.0))
}

fn filter_node(host: &dyn Host) -> (f32, f32) {
    let g = filter_plot();
    let hz = vm::cutoff_hz(host.get(a::LpfCut) as f64);
    (g.x + ((hz / 20.0).log10() / 3.0) as f32 * g.w, g.bottom() - host.get(a::LpfRes) * g.h)
}

fn filter(ui: &Ui, host: &dyn Host, c: &mut Canvas) {
    let r = FILTER_RECT;
    let g = filter_plot();
    c.rrect(r, 5.0, GRAPH);
    for hz in [100.0, 1000.0, 10000.0] {
        let x = g.x + ((hz / 20.0f64).log10() / 3.0) as f32 * g.w;
        c.vline(x, g.y, g.bottom(), 1.0, GRAPH_LINE);
    }
    for i in 0..=2 { c.hline(g.x, g.right(), g.y + g.h * i as f32 / 2.0, 1.0, GRAPH_LINE); }
    display::filter(host, c, g);
    for (hz, label, align) in [(20.0, "20 Hz", 0.0), (100.0, "100", 0.5), (1000.0, "1k", 0.5), (10000.0, "10k", 0.5), (20000.0, "20k", 1.0)] {
        let x = g.x + ((hz / 20.0f64).log10() / 3.0) as f32 * g.w;
        c.text(label, x, r.bottom() - 4.0, &Style::new(Face::Mono, 7.5, 0.0), t::DISPLAY_DIM, align);
    }
    let (x, y) = filter_node(host);
    c.vline(x, g.y, g.bottom(), 1.0, ENV_COLOR.with_alpha(0.3));
    c.hline(g.x, g.right(), y, 1.0, ENV_COLOR.with_alpha(0.16));
    let hot = matches!(ui.page_state.instrument_drag, Some(Drag::Filter { .. })) || r.contains(ui.mouse.0, ui.mouse.1);
    c.circle(x, y, if hot { 7.0 } else { 6.0 }, t::ORANGE);
    c.circle_stroke(x, y, 3.0, 1.3, t::PANEL_TOP);
    let cut = vm::cutoff_hz(host.get(a::LpfCut) as f64);
    let hz = if cut >= 1000.0 { format!("{:.2} kHz", cut / 1000.0) } else { format!("{cut:.0} Hz") };
    let value = format!("{hz}   RES {:.0}%", host.get(a::LpfRes) * 100.0);
    c.text(&value, g.right() - 4.0, g.y + 10.0, &Style::new(Face::MonoSemi, 9.0, 0.0), t::DISPLAY_INK, 1.0);
}

fn envelope_addresses(host: &dyn Host, id: u32) -> [usize; 4] {
    if id == TONE_ENVELOPE && index_of(host, a::LpfEnvSource) == 0 { ENV1 } else { ENV2 }
}

fn envelope_rect(id: u32) -> R { if id == TONE_ENVELOPE { TONE_ENV_RECT } else { AMP_ENV_RECT } }

fn physical_envelope(host: &dyn Host, ad: [usize; 4]) -> [f64; 4] {
    std::array::from_fn(|i| PARAMS[ad[i]].physical_f32(host.get(ad[i])))
}

struct EnvelopeGeometry {
    plot: R,
    nodes: [(f32, f32); 4],
    pixels_per_weight: f32,
}

/// Square-root time compression keeps both millisecond and multi-second contours legible.
/// A small minimum stage width also keeps all four handles available at the fastest settings.
fn envelope_geometry(r: R, values: [f64; 4]) -> EnvelopeGeometry {
    let plot = R::new(r.x + 12.0, r.y + 13.0, r.w - 24.0, r.h - 30.0);
    let w = [(values[0] / 1000.0).sqrt() as f32, (values[1] * 0.6 / 1000.0).sqrt() as f32, 0.4, (values[3] * 0.6 / 1000.0).sqrt() as f32];
    let base = 18.0;
    let k = (plot.w - 4.0 * base).max(1.0) / w.iter().sum::<f32>().max(0.01);
    let mut x = plot.x;
    let level = (values[2] / 100.0).clamp(0.0, 1.0) as f32;
    let nodes = std::array::from_fn(|i| {
        x += base + k * w[i];
        (x, if i == 0 { plot.y } else if i == 3 { plot.bottom() } else { plot.bottom() - level * plot.h })
    });
    EnvelopeGeometry { plot, nodes, pixels_per_weight: k }
}

fn nearest_node(g: &EnvelopeGeometry, x: f32, y: f32) -> Option<usize> {
    let nearest = g.nodes.iter().enumerate().map(|(i, p)| (i, (p.0 - x).hypot(p.1 - y)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    (nearest.1 <= 18.0).then_some(nearest.0)
}

fn envelope(ui: &Ui, host: &dyn Host, c: &mut Canvas, id: u32, r: R, ad: [usize; 4]) {
    let ph = physical_envelope(host, ad);
    let g = envelope_geometry(r, ph);
    c.rrect(r, 4.0, GRAPH);
    c.hline(g.plot.x, g.plot.right(), g.plot.bottom(), 1.0, GRAPH_LINE);
    c.hline(g.plot.x, g.plot.right(), g.plot.y, 1.0, GRAPH_LINE);
    let (ax, _) = g.nodes[0];
    let (dx, sy) = g.nodes[1];
    let (sx, _) = g.nodes[2];
    let (rx, _) = g.nodes[3];
    let y_at = |level: f64| g.plot.bottom() - level as f32 * g.plot.h;
    let sustain = ph[2] / 100.0;
    let mut pb = PathBuilder::new();
    pb.move_to(g.plot.x, g.plot.bottom());
    // The RC attack follows the same target as the engine; decay/release are exponential.
    let attack_end = -(1.0 - 1.0 / vm::ATTACK_TARGET).ln();
    for i in 1..=18 {
        let u = i as f64 / 18.0;
        let level = vm::ATTACK_TARGET * (1.0 - (-u * attack_end).exp());
        pb.line_to(g.plot.x + (ax - g.plot.x) * u as f32, y_at(level));
    }
    for i in 1..=18 {
        let u = i as f64 / 18.0;
        let decay = ((-u * 4.0).exp() - (-4.0f64).exp()) / (1.0 - (-4.0f64).exp());
        pb.line_to(ax + (dx - ax) * u as f32, y_at(sustain + (1.0 - sustain) * decay));
    }
    pb.line_to(sx, sy);
    for i in 1..=18 {
        let u = i as f64 / 18.0;
        let release = ((-u * 4.5).exp() - (-4.5f64).exp()) / (1.0 - (-4.5f64).exp());
        pb.line_to(sx + (rx - sx) * u as f32, y_at(sustain * release));
    }
    if let Some(p) = pb.finish() { c.stroke_path(&p, 1.6, ENV_COLOR); }
    let drag_node = match ui.page_state.instrument_drag {
        Some(Drag::Envelope { id: dragged, node, addresses, .. }) if dragged == id && addresses == ad => Some(node),
        _ => None,
    };
    let hover_node = if r.contains(ui.mouse.0, ui.mouse.1) { nearest_node(&g, ui.mouse.0, ui.mouse.1) } else { None };
    let hot = drag_node.or(hover_node);
    for (i, (x, y)) in g.nodes.iter().copied().enumerate() {
        c.vline(x, g.plot.y, g.plot.bottom(), 1.0, GRAPH_LINE);
        c.circle(x, y, if hot == Some(i) { 5.0 } else { 3.5 }, if hot == Some(i) { t::ORANGE } else { ENV_COLOR });
        c.circle_stroke(x, y, 2.0, 1.0, GRAPH);
        c.text(["A", "D", "S", "R"][i], x, r.bottom() - 4.0, &Style::new(Face::MonoSemi, 8.0, 0.0), if hot == Some(i) { t::DISPLAY_INK } else { t::DISPLAY_DIM }, 0.5);
    }
    if let Some(i) = hot {
        let value = PARAMS[ad[i]].display_string(host.get(ad[i]) as f64);
        c.text(&format!("{} {value}", ["A", "D", "S", "R"][i]), r.right() - 10.0, r.y + 10.0,
            &Style::new(Face::MonoSemi, 8.0, 0.0), t::DISPLAY_INK, 1.0);
    } else {
        c.text("TIME / COMPRESSED", r.x + 9.0, r.y + 9.0, &Style::new(Face::Mono, 6.5, 0.0), t::DISPLAY_DIM, 0.0);
    }
}

fn gesture_addresses(addresses: [usize; 4], node: usize) -> Vec<usize> {
    if node == 1 { vec![addresses[1], addresses[2]] } else { vec![addresses[node]] }
}

pub fn area_down(ui: &mut Ui, host: &dyn Host, id: u32, _r: R, x: f32, y: f32, double: bool) {
    if id == FILTER {
        if double {
            for addr in [a::LpfCut, a::LpfRes] { host.set_once(addr, PARAMS[addr].default as f32); }
            return;
        }
        area_up(ui, host);
        host.begin(a::LpfCut);
        host.begin(a::LpfRes);
        let (cutoff, resonance) = filter_values_at(x, y);
        ui.page_state.instrument_drag = Some(Drag::Filter { start: (x, y), cutoff, resonance });
        area_drag(ui, host, id, x, y, Mods::default());
    } else if id == TONE_ENVELOPE || id == AMP_ENVELOPE {
        let addresses = envelope_addresses(host, id);
        let physical = physical_envelope(host, addresses);
        let g = envelope_geometry(envelope_rect(id), physical);
        let Some(node) = nearest_node(&g, x, y) else { return; };
        if double {
            host.set_once(addresses[node], PARAMS[addresses[node]].default as f32);
            return;
        }
        area_up(ui, host);
        for addr in gesture_addresses(addresses, node) { host.begin(addr); }
        ui.page_state.instrument_drag = Some(Drag::Envelope { id, addresses, node, start: (x, y), physical,
            pixels_per_weight: g.pixels_per_weight, plot_height: g.plot.h });
        ui.focus = Some((addresses[node], Instant::now()));
    }
}

fn time_after_drag(ms: f64, factor: f64, pixels: f32, pixels_per_weight: f32) -> f64 {
    let initial = (ms * factor / 1000.0).sqrt();
    let weight = (initial + (pixels / pixels_per_weight.max(0.01)) as f64).max(0.0);
    weight * weight * 1000.0 / factor
}

fn set_physical(host: &dyn Host, addr: usize, value: f64) {
    let normalized = PARAMS[addr].normalized_from_physical(value).clamp(0.0, 1.0) as f32;
    if normalized.is_finite() { host.set(addr, normalized); }
}

pub fn area_drag(ui: &mut Ui, host: &dyn Host, id: u32, x: f32, y: f32, mods: Mods) {
    let Some(drag) = ui.page_state.instrument_drag else { return; };
    match drag {
        Drag::Filter { start, cutoff, resonance } if id == FILTER => {
            let (cut, res) = if mods.shift {
                let g = filter_plot();
                (cutoff + (x - start.0) / g.w * 0.1, resonance + (start.1 - y) / g.h * 0.1)
            } else { filter_values_at(x, y) };
            host.set(a::LpfCut, cut.clamp(0.0, 1.0));
            host.set(a::LpfRes, res.clamp(0.0, 1.0));
            ui.focus = Some((a::LpfCut, Instant::now()));
        }
        Drag::Envelope { id: captured, addresses, node, start, physical, pixels_per_weight, plot_height } if id == captured => {
            let fine = if mods.shift { 0.1 } else { 1.0 };
            let (dx, dy) = ((x - start.0) * fine, (start.1 - y) * fine);
            match node {
                0 | 3 => set_physical(host, addresses[node], time_after_drag(physical[node], if node == 0 { 1.0 } else { 0.6 }, dx, pixels_per_weight)),
                1 => {
                    set_physical(host, addresses[1], time_after_drag(physical[1], 0.6, dx, pixels_per_weight));
                    set_physical(host, addresses[2], (physical[2] + (dy / plot_height * 100.0) as f64).clamp(0.0, 100.0));
                }
                2 => set_physical(host, addresses[2], (physical[2] + (dy / plot_height * 100.0) as f64).clamp(0.0, 100.0)),
                _ => {}
            }
            ui.focus = Some((addresses[node], Instant::now()));
        }
        _ => {}
    }
}

pub fn area_up(ui: &mut Ui, host: &dyn Host) {
    match ui.page_state.instrument_drag.take() {
        Some(Drag::Filter { .. }) => { host.end(a::LpfCut); host.end(a::LpfRes); }
        Some(Drag::Envelope { addresses, node, .. }) => for addr in gesture_addresses(addresses, node) { host.end(addr); },
        None => {}
    }
}

pub fn area_wheel(ui: &mut Ui, host: &dyn Host, id: u32, dy: f32) {
    if id == FILTER {
        host.set_once(a::LpfCut, (host.get(a::LpfCut) + dy * 0.012).clamp(0.0, 1.0));
        ui.focus = Some((a::LpfCut, Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Button, Event, MockHost};
    use jovian_core::{dsp::Meters, preset::PatchState};
    use std::cell::RefCell;

    struct TrackingHost {
        inner: MockHost,
        gestures: RefCell<Vec<(bool, usize)>>,
    }

    impl Default for TrackingHost {
        fn default() -> Self {
            Self { inner: MockHost::new(PatchState::default()), gestures: RefCell::new(Vec::new()) }
        }
    }

    impl Host for TrackingHost {
        fn get(&self, addr: usize) -> f32 { self.inner.get(addr) }
        fn begin(&self, addr: usize) { self.gestures.borrow_mut().push((true, addr)); }
        fn set(&self, addr: usize, value: f32) { self.inner.set(addr, value); }
        fn end(&self, addr: usize) { self.gestures.borrow_mut().push((false, addr)); }
        fn load_patch(&self, patch: &PatchState) { self.inner.load_patch(patch); }
        fn current_patch(&self) -> PatchState { self.inner.current_patch() }
        fn rename(&self, name: &str, bank: &str) { self.inner.rename(name, bank); }
        fn patch_info(&self) -> (String, String, u64) { self.inner.patch_info() }
        fn meters(&self) -> Meters { self.inner.meters() }
        fn midi(&self, _status: u8, _d1: u8, _d2: u8) {}
    }

    fn down(ui: &mut Ui, host: &dyn Host, x: f32, y: f32) {
        ui.event(host, Event::Down { x, y, button: Button::Left, mods: Mods::default() });
    }

    #[test]
    fn frequency_axis_roundtrips_the_engine_cutoff_mapping() {
        let host = TrackingHost::default();
        for cutoff in [0.0, 0.25, 0.63, 1.0] {
            host.set(a::LpfCut, cutoff);
            host.set(a::LpfRes, 0.42);
            let (x, y) = filter_node(&host);
            let (cut, res) = filter_values_at(x, y);
            assert!((cut - cutoff).abs() < 0.00001);
            assert!((res - 0.42).abs() < 0.00001);
        }
        assert_eq!(filter_values_at(-1000.0, -1000.0), (0.0, 1.0));
        assert_eq!(filter_values_at(3000.0, 3000.0), (1.0, 0.0));
    }

    #[test]
    fn envelope_handles_remain_separate_at_parameter_extremes() {
        for values in [[0.8, 1.5, 100.0, 1.5], [7200.0, 46500.0, 0.0, 46500.0]] {
            let g = envelope_geometry(TONE_ENV_RECT, values);
            assert!(g.nodes.iter().all(|p| p.0 >= g.plot.x && p.0 <= g.plot.right() + 0.001));
            for pair in g.nodes.windows(2) { assert!(pair[1].0 - pair[0].0 >= 18.0); }
            for (i, (x, y)) in g.nodes.iter().copied().enumerate() { assert_eq!(nearest_node(&g, x, y), Some(i)); }
        }
    }

    #[test]
    fn sustain_drag_uses_physical_percent_and_cancellation_ends_once() {
        let host = TrackingHost::default();
        set_physical(&host, a::Env2_Sustain, 70.0);
        let mut ui = Ui::new(Page::Synth, 100);
        ui.tick(&host);
        let g = envelope_geometry(AMP_ENV_RECT, physical_envelope(&host, ENV2));
        let (x, y) = g.nodes[2];
        down(&mut ui, &host, x, y);
        ui.event(&host, Event::Move { x, y: y + g.plot.h * 0.25, mods: Mods::default() });
        assert!((PARAMS[a::Env2_Sustain].physical_f32(host.get(a::Env2_Sustain)) - 45.0).abs() < 0.001);
        ui.event(&host, Event::Cancel);
        ui.cancel_interaction(&host);
        ui.event(&host, Event::Up { x, y, button: Button::Left, mods: Mods::default() });
        assert_eq!(host.gestures.borrow().as_slice(), &[(true, a::Env2_Sustain), (false, a::Env2_Sustain)]);
        assert!(ui.page_state.instrument_drag.is_none());
    }

    #[test]
    fn envelope_source_automation_does_not_redirect_a_captured_gesture() {
        let host = TrackingHost::default();
        let mut ui = Ui::new(Page::Synth, 100);
        ui.tick(&host);
        let g = envelope_geometry(TONE_ENV_RECT, physical_envelope(&host, ENV1));
        let (x, y) = g.nodes[1];
        down(&mut ui, &host, x, y);
        host.set(a::LpfEnvSource, 1.0);
        let amp_decay = host.get(a::Env2_Decay);
        let amp_sustain = host.get(a::Env2_Sustain);
        ui.event(&host, Event::Move { x: x + 15.0, y: y + 8.0, mods: Mods::default() });
        assert!(host.get(a::Env1_Decay) > 0.0);
        assert!(host.get(a::Env1_Sustain) < 1.0);
        assert_eq!(host.get(a::Env2_Decay), amp_decay);
        assert_eq!(host.get(a::Env2_Sustain), amp_sustain);
        ui.event(&host, Event::Cancel);
        assert_eq!(host.gestures.borrow().as_slice(), &[
            (true, a::Env1_Decay), (true, a::Env1_Sustain), (false, a::Env1_Decay), (false, a::Env1_Sustain),
        ]);
    }

    #[test]
    fn time_drag_preserves_physical_milliseconds_at_the_origin_and_clamps_at_limits() {
        let host = TrackingHost::default();
        for (addr, factor) in [(a::Env1_Attack, 1.0), (a::Env1_Decay, 0.6), (a::Env1_Release, 0.6)] {
            set_physical(&host, addr, time_after_drag(500.0, factor, 0.0, 100.0));
            assert!((PARAMS[addr].physical_f32(host.get(addr)) - 500.0).abs() < 0.01);
            set_physical(&host, addr, time_after_drag(500.0, factor, 1e6, 100.0));
            assert_eq!(host.get(addr), 1.0);
            set_physical(&host, addr, time_after_drag(500.0, factor, -1e6, 100.0));
            assert_eq!(host.get(addr), 0.0);
        }
    }

    #[test]
    fn interrupted_filter_xy_gesture_closes_both_parameters_once() {
        let host = TrackingHost::default();
        let mut ui = Ui::new(Page::Synth, 100);
        ui.tick(&host);
        down(&mut ui, &host, FILTER_RECT.cx(), FILTER_RECT.cy());
        ui.event(&host, Event::Cancel);
        ui.cancel_interaction(&host);
        assert_eq!(host.gestures.borrow().as_slice(), &[
            (true, a::LpfCut), (true, a::LpfRes), (false, a::LpfCut), (false, a::LpfRes),
        ]);
    }
}
