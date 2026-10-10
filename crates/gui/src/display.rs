//! The header display: patch name, and whatever parameter is being touched with a live graph
//! (envelope shape, filter response, LFO wave) — like the screen of a modern hardware synth.

use crate::canvas::{vgrad, Canvas, Fill, PathBuilder, Rgba, R};
use crate::host::Host;
use crate::text::{Face, Style};
use crate::theme as t;
use crate::ui::Ui;
use crate::widgets as w;
use jovian_core::dsp::voice_math;
use jovian_core::params::{addr as a, Group, PARAMS};
use std::time::Duration;

pub fn draw_static(c: &mut Canvas, r: R) {
    c.rrect(r.inset(-1.0), 7.0, Rgba::white(0.06));
    c.rrect(r, 6.0, vgrad(r.y, r.bottom(), Rgba::hex(0x060607), t::DISPLAY_BG));
    c.rrect_stroke(r, 6.0, 1.0, Rgba::black(0.9));
    // inner shadow at the top edge
    c.rrect(R::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, 6.0), 5.0, vgrad(r.y, r.y + 6.0, Rgba::black(0.5), Rgba::black(0.0)));
    c.vline(r.x + 252.0, r.y + 8.0, r.bottom() - 8.0, 1.0, Rgba::white(0.08));
}

pub fn draw(ui: &Ui, host: &dyn Host, c: &mut Canvas, r: R) {
    // patch
    let pr = R::new(r.x + 30.0, r.y, 192.0, r.h);
    let bank = if ui.patch_bank.is_empty() { "JOVIAN-8".to_string() } else { ui.patch_bank.to_uppercase() };
    c.set_clip(pr);
    c.text(&bank, pr.x, r.y + 15.0, &Style::new(Face::CondSemi, 8.5, 1.4), t::INK_DIM, 0.0);
    let nst = Style::new(Face::Semi, 14.5, 0.2);
    let name = if ui.patch_name.is_empty() { "Init" } else { &ui.patch_name };
    let wn = c.text(name, pr.x, r.y + 31.0, &nst, Rgba::hex(0xF3EEE4), 0.0);
    c.clear_clip();
    if ui.is_modified() {
        c.circle((pr.x + wn + 7.0).min(pr.right() - 3.0), r.y + 26.5, 2.4, t::AMBER);
    }

    // context
    let cr = R::new(r.x + 262.0, r.y, r.w - 270.0, r.h);
    let focus = ui.dragging_value().or_else(|| ui.hovered().and_then(|c| c.kind.addr())).or_else(|| {
        ui.focus.and_then(|(addr, at)| (at.elapsed() < Duration::from_millis(1600)).then_some(addr))
    });
    match focus {
        Some(addr) => {
            let spec = &PARAMS[addr];
            let v = host.get(addr);
            c.text(&spec.name.to_uppercase(), cr.x, r.y + 15.0, &Style::new(Face::CondSemi, 8.5, 1.2), t::INK_DIM, 0.0);
            c.text(&spec.display_string(v as f64), cr.x, r.y + 31.0, &Style::new(Face::MonoSemi, 13.0, 0.0), t::AMBER, 0.0);
            let gr = R::new(cr.right() - 92.0, r.y + 6.0, 90.0, r.h - 12.0);
            graph(host, c, gr, addr);
        }
        None => voices(ui, c, cr),
    }
}

fn voices(ui: &Ui, c: &mut Canvas, r: R) {
    c.text("VOICES", r.x, r.y + 15.0, &Style::new(Face::CondSemi, 8.5, 1.2), t::INK_DIM, 0.0);
    let m = ui.meters;
    for i in 0..16 {
        let x = r.x + 4.0 + (i % 8) as f32 * 11.0;
        let y = r.y + 24.0 + (i / 8) as f32 * 8.0;
        let active = m.active_voices & (1 << i) != 0;
        let gated = m.gated_voices & (1 << i) != 0;
        let col = if gated {
            t::AMBER
        } else if active {
            t::AMBER_DIM
        } else {
            Rgba::white(0.08)
        };
        c.rrect(R::new(x - 3.5, y - 2.0, 7.0, 4.0), 1.0, col);
    }
    // LFO lamp
    let lx = r.x + 104.0;
    c.text("LFO", lx, r.y + 15.0, &Style::new(Face::CondSemi, 8.5, 1.2), t::INK_DIM, 0.0);
    w::led(c, lx + 6.0, r.y + 26.0, (m.lfo * 0.5 + 0.5).clamp(0.0, 1.0));
    // envelope meters
    let ex = r.x + 140.0;
    c.text("ENV", ex, r.y + 15.0, &Style::new(Face::CondSemi, 8.5, 1.2), t::INK_DIM, 0.0);
    for (k, v) in [m.env1, m.env2].iter().enumerate() {
        let br = R::new(ex, r.y + 21.0 + k as f32 * 7.0, 60.0, 4.0);
        c.rrect(br, 1.0, Rgba::white(0.07));
        c.rrect(R::new(br.x, br.y, br.w * v.clamp(0.0, 1.0), br.h), 1.0, if k == 0 { t::RED } else { t::AMBER });
    }
}

fn frame(c: &mut Canvas, r: R) {
    c.rrect(r, 3.0, Rgba::white(0.025));
    for k in 1..4 {
        let y = r.y + r.h * k as f32 / 4.0;
        c.hline(r.x + 2.0, r.right() - 2.0, y, 1.0, Rgba::white(0.04));
    }
}

fn stroke_fill(c: &mut Canvas, pts: &[(f32, f32)], base_y: f32, col: Rgba) {
    if pts.len() < 2 {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, base_y);
    for p in pts {
        pb.line_to(p.0, p.1);
    }
    pb.line_to(pts[pts.len() - 1].0, base_y);
    pb.close();
    if let Some(p) = pb.finish() {
        c.fill_path(&p, Fill::Linear(0.0, pts.iter().map(|p| p.1).fold(f32::MAX, f32::min), 0.0, base_y, vec![(0.0, col.with_alpha(0.32)), (1.0, col.with_alpha(0.02))]));
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        pb.line_to(p.0, p.1);
    }
    if let Some(p) = pb.finish() {
        c.stroke_path(&p, 1.4, col);
    }
}

/// Mini graph for the group of `addr`.
pub fn graph(host: &dyn Host, c: &mut Canvas, r: R, addr: usize) {
    frame(c, r);
    let g = r.inset_xy(3.0, 3.0);
    match PARAMS[addr].group {
        Group::Env1 => adsr(host, c, g, [a::Env1_Attack, a::Env1_Decay, a::Env1_Sustain, a::Env1_Release]),
        Group::Env2 => adsr(host, c, g, [a::Env2_Attack, a::Env2_Decay, a::Env2_Sustain, a::Env2Release]),
        Group::Vcf | Group::Vca | Group::Vco if [a::LpfCut, a::LpfRes, a::LpfSlop, a::HPF_Cutoff].contains(&addr) => filter(host, c, g),
        Group::Lfo => {
            let wave = PARAMS[a::LFO_Waveform].index_f32(host.get(a::LFO_Waveform));
            lfo(c, g, wave);
        }
        _ => {
            let v = host.get(addr);
            let br = R::new(g.x, g.cy() - 3.0, g.w, 6.0);
            c.rrect(br, 3.0, Rgba::white(0.06));
            c.rrect(R::new(br.x, br.y, br.w * v, br.h), 3.0, t::AMBER);
        }
    }
}

pub fn adsr(host: &dyn Host, c: &mut Canvas, r: R, ad: [usize; 4]) {
    let ph = |i: usize| PARAMS[ad[i]].physical_f32(host.get(ad[i]));
    let (at, dt, s, rt) = (ph(0), ph(1), ph(2) / 100.0, ph(3));
    // compressed time axis: sqrt keeps short and long envelopes readable
    let f = |ms: f64| (ms.max(0.0) / 1000.0).sqrt() as f32;
    let (wa, wd, wr) = (f(at), f(dt * 0.6), f(rt * 0.6));
    let ws = 0.35;
    let total = wa + wd + ws + wr;
    let k = r.w / total.max(0.01);
    let y = |lv: f64| r.bottom() - lv as f32 * r.h;
    let mut pts = vec![(r.x, r.bottom())];
    // attack curve (RC towards 1.5, cut at 1)
    let n = 10;
    for i in 1..=n {
        let u = i as f64 / n as f64;
        let lv = 1.5 * (1.0 - (-u * 1.0986).exp()) / (1.0 - (-1.0986f64).exp()) / 1.5;
        pts.push((r.x + wa * k * u as f32, y(lv.min(1.0))));
    }
    let x1 = r.x + wa * k;
    for i in 1..=n {
        let u = i as f64 / n as f64;
        let lv = s + (1.0 - s) * (-u * 4.0).exp();
        pts.push((x1 + wd * k * u as f32, y(lv)));
    }
    let x2 = x1 + wd * k;
    let x3 = x2 + ws * k;
    pts.push((x3, y(s)));
    for i in 1..=n {
        let u = i as f64 / n as f64;
        let lv = s * (-u * 4.5).exp();
        pts.push((x3 + wr * k * u as f32, y(lv)));
    }
    stroke_fill(c, &pts, r.bottom(), t::AMBER);
}

/// Magnitude response of the ladder model (4 one-poles in a feedback loop).
pub fn filter(host: &dyn Host, c: &mut Canvas, r: R) {
    let cut = voice_math::cutoff_hz(host.get(a::LpfCut) as f64);
    let k = voice_math::resonance_k(host.get(a::LpfRes) as f64);
    let two = PARAMS[a::LpfSlop].index_f32(host.get(a::LpfSlop)) == 0;
    let hp = PARAMS[a::HPF_Cutoff].physical_f32(host.get(a::HPF_Cutoff));
    let n = 64;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let u = i as f64 / n as f64;
        let f = 20.0 * 1000f64.powf(u);
        // one pole: 1 / (1 + j f/fc)
        let (re, im) = (1.0, f / cut);
        let d = re * re + im * im;
        let (gr, gi) = (re / d, -im / d);
        let mul = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
        let g2 = mul((gr, gi), (gr, gi));
        let g4 = mul(g2, g2);
        let num = if two { g2 } else { g4 };
        let den = (1.0 + k * g4.0, k * g4.1);
        let dd = den.0 * den.0 + den.1 * den.1;
        let h = ((num.0 * den.0 + num.1 * den.1) / dd, (num.1 * den.0 - num.0 * den.1) / dd);
        let mut mag = (h.0 * h.0 + h.1 * h.1).sqrt() * (1.0 + 0.11 * k);
        // the 6 dB high-pass in front
        mag *= (f / hp) / (1.0 + (f / hp) * (f / hp)).sqrt();
        let db = 20.0 * mag.max(1e-6).log10();
        let yy = ((db + 36.0) / 52.0).clamp(0.0, 1.0);
        pts.push((r.x + r.w * u as f32, r.bottom() - r.h * yy as f32));
    }
    stroke_fill(c, &pts, r.bottom(), t::AMBER);
}

pub fn lfo(c: &mut Canvas, r: R, wave: usize) {
    let n = 48;
    let mut pts = Vec::with_capacity(n + 1);
    let held = [0.3f32, -0.7, 0.8, -0.2, 0.55, -0.9, 0.1, 0.65];
    for i in 0..=n {
        let u = i as f32 / n as f32 * 2.0;
        let ph = u.fract();
        let v = match wave {
            0 => -(ph * std::f32::consts::TAU).cos(),
            1 => 1.0 - 2.0 * ph,
            2 => {
                if ph < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            _ => held[((u * 4.0) as usize).min(7)],
        };
        pts.push((r.x + r.w * u / 2.0, r.cy() - v * r.h * 0.42));
    }
    stroke_fill(c, &pts, r.bottom(), t::AMBER);
}
