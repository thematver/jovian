//! Renders the editor to PNG files for design review:
//!   jovian-shot <out-dir> [preset file] [scale]
//! Writes one image per page plus interaction states.

use jovian_core::preset::{jup4x, PatchState};
use jovian_gui::canvas::Canvas;
use jovian_gui::layout::Page;
use jovian_gui::{Event, Host, MockHost, Mods, Ui};
use tiny_skia::Pixmap;

fn render(ui: &mut Ui, host: &MockHost, scale: f32, path: &str) {
    let (w, h) = (jovian_gui::layout::W, jovian_gui::layout::H);
    let mut pm = Pixmap::new((w * scale) as u32, (h * scale) as u32).unwrap();
    ui.tick(host);
    ui.invalidate_all();
    let t0 = std::time::Instant::now();
    {
        let fonts_ptr: *const jovian_gui::text::Fonts = &ui.fonts;
        // the canvas borrows the fonts while the ui renders (fonts are never mutated)
        let fonts = unsafe { &*fonts_ptr };
        let mut c = Canvas::new(pm.as_mut(), scale, false, fonts);
        ui.render(host, &mut c);
    }
    let t1 = t0.elapsed();
    // second frame: background cached
    let t2 = std::time::Instant::now();
    {
        let fonts_ptr: *const jovian_gui::text::Fonts = &ui.fonts;
        let fonts = unsafe { &*fonts_ptr };
        let mut c = Canvas::new(pm.as_mut(), scale, false, fonts);
        ui.render(host, &mut c);
    }
    let t3 = t2.elapsed();
    // a fader moved by the host: partial redraw
    host.set(jovian_core::params::addr::LpfCut, (host.get(jovian_core::params::addr::LpfCut) * 0.9).max(0.05));
    ui.tick(host);
    let t4 = std::time::Instant::now();
    {
        let fonts_ptr: *const jovian_gui::text::Fonts = &ui.fonts;
        let fonts = unsafe { &*fonts_ptr };
        let mut c = Canvas::new(pm.as_mut(), scale, false, fonts);
        ui.render(host, &mut c);
    }
    println!("  partial (one fader) {:.2} ms", t4.elapsed().as_secs_f64() * 1e3);
    pm.save_png(path).unwrap();
    println!("{path}: first frame {:.1} ms, cached {:.1} ms", t1.as_secs_f64() * 1e3, t3.as_secs_f64() * 1e3);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| ".".into());
    let patch = args
        .get(2)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|d| jup4x::read(&d).ok())
        .and_then(|v| v.first().map(PatchState::from_preset))
        .unwrap_or_default();
    let scale: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let host = MockHost::new(patch);
    host.load_patch(&host.current_patch());
    let mut ui = Ui::new(Page::Synth, 100);
    std::fs::create_dir_all(&out).unwrap();
    render(&mut ui, &host, scale, &format!("{out}/synth.png"));
    // hover a fader and a switch
    ui.event(&host, Event::Move { x: 600.0, y: 420.0, mods: Mods::default() });
    render(&mut ui, &host, scale, &format!("{out}/synth-hover.png"));
    for p in [Page::Mod, Page::Fx, Page::Library] {
        ui.set_page(p);
        ui.library.ensure_scanned_blocking();
        render(&mut ui, &host, scale, &format!("{out}/{}.png", p.title().to_lowercase()));
    }
    // effects demo: EQ, delay and compressor in the three slots
    use jovian_core::params::{address, normalized_index, spec};
    let set_item = |id: &str, item: &str| {
        let s = spec(id);
        let i = s.items().iter().position(|x| *x == item).unwrap();
        host.set(address(id).unwrap(), normalized_index(i as i64, s.item_count()) as f32);
    };
    set_item("FX_Enable", "On");
    for (slot, kind) in [(1, "Param EQ"), (2, "Delay"), (3, "Compressor")] {
        set_item(&format!("FX{slot}_ModuleType"), kind);
        set_item(&format!("FX{slot}_Bypass"), "On");
    }
    host.set(address("FX1_Dry_Wet").unwrap(), 1.0);
    host.set(address("FX1_ParamEq_PeakGain1").unwrap(), 0.8);
    host.set(address("FX1_ParamEq_PeakGain2").unwrap(), 0.25);
    host.set(address("FX1_ParamEq_HFShelvGain").unwrap(), 0.7);
    host.set(address("FX2_Delay_Feedback").unwrap(), 0.6);
    host.set(address("FX3_Compressor_Threshold").unwrap(), 0.6);
    ui.set_page(Page::Fx);
    render(&mut ui, &host, scale, &format!("{out}/effects-demo.png"));
}
