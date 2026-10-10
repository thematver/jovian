//! Regression coverage for interrupted host gestures and cached editor frames.

use jovian_core::dsp::{modulation::CURVE_ADDRESSES, Meters};
use jovian_core::params::{self, addr, PARAMS};
use jovian_core::preset::PatchState;
use jovian_gui::canvas::{Canvas, R};
use jovian_gui::layout::{self, Kind, Page};
use jovian_gui::pages::modpage;
use jovian_gui::text::Fonts;
use jovian_gui::{Button, Cursor, Event, Host, MockHost, Mods, Ui};
use std::cell::RefCell;
use tiny_skia::Pixmap;

#[derive(Default)]
struct Calls {
    begins: Vec<usize>,
    ends: Vec<usize>,
    midi: Vec<(u8, u8, u8)>,
}

struct TrackingHost {
    inner: MockHost,
    calls: RefCell<Calls>,
}

impl Default for TrackingHost {
    fn default() -> Self {
        Self { inner: MockHost::new(PatchState::default()), calls: RefCell::new(Calls::default()) }
    }
}

impl Host for TrackingHost {
    fn get(&self, addr: usize) -> f32 { self.inner.get(addr) }
    fn begin(&self, addr: usize) { self.calls.borrow_mut().begins.push(addr); }
    fn set(&self, addr: usize, value: f32) { self.inner.set(addr, value); }
    fn end(&self, addr: usize) { self.calls.borrow_mut().ends.push(addr); }
    fn load_patch(&self, patch: &PatchState) { self.inner.load_patch(patch); }
    fn current_patch(&self) -> PatchState { self.inner.current_patch() }
    fn rename(&self, name: &str, bank: &str) { self.inner.rename(name, bank); }
    fn patch_info(&self) -> (String, String, u64) { self.inner.patch_info() }
    fn meters(&self) -> Meters { self.inner.meters() }
    fn midi(&self, status: u8, d1: u8, d2: u8) { self.calls.borrow_mut().midi.push((status, d1, d2)); }
}

fn control_rect(ui: &Ui, kind: impl Fn(&Kind) -> bool) -> R {
    ui.layout.controls.iter()
        .find(|c| c.page.map_or(true, |p| p == ui.page)
            && c.view.map_or(true, |view| match ui.page {
                Page::Mod => view == ui.page_state.mod_source,
                Page::Fx => view == ui.page_state.fx_selected,
                _ => true,
            }) && kind(&c.kind))
        .expect("control is present on the current page").r
}

fn down(ui: &mut Ui, host: &dyn Host, x: f32, y: f32) {
    assert!(ui.event(host, Event::Down { x, y, button: Button::Left, mods: Mods::default() }).captured);
}

fn up(ui: &mut Ui, host: &dyn Host, x: f32, y: f32) {
    ui.event(host, Event::Up { x, y, button: Button::Left, mods: Mods::default() });
}

fn move_to(ui: &mut Ui, host: &dyn Host, x: f32, y: f32) {
    ui.event(host, Event::Move { x, y, mods: Mods::default() });
}

fn select_mod_source(ui: &mut Ui, host: &dyn Host, source: usize) {
    let r = control_rect(ui, |k| *k == Kind::Area(modpage::SOURCE_BASE + source as u32));
    down(ui, host, r.cx(), r.cy());
    up(ui, host, r.cx(), r.cy());
    assert_eq!(ui.page_state.mod_source, source);
}

#[test]
fn focus_loss_releases_a_held_note_once_even_if_mouse_up_arrives_later() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Synth, 100);
    ui.tick(&host);
    let r = control_rect(&ui, |k| *k == Kind::Keyboard);
    let (_, key, _) = jovian_gui::ui::keyboard_keys(jovian_gui::ui::keybed(r))
        .into_iter().find(|(_, _, black)| !black).expect("keyboard has a white key");
    let (x, y) = (key.cx(), key.bottom() - 18.0);
    let (note, velocity) = jovian_gui::ui::key_at(r, x, y).expect("point is on a white key");

    down(&mut ui, &host, x, y);
    assert_eq!(ui.held_note, Some(note));
    assert_eq!(host.calls.borrow().midi.as_slice(), &[(0x90, note, velocity)]);

    assert!(ui.event(&host, Event::Cancel).captured);
    ui.cancel_interaction(&host);
    up(&mut ui, &host, x, y);

    assert_eq!(ui.held_note, None);
    assert_eq!(host.calls.borrow().midi.as_slice(), &[(0x90, note, velocity), (0x80, note, 0)]);
}

#[test]
fn interrupted_parameter_drag_balances_the_host_gesture_once() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Synth, 100);
    ui.tick(&host);
    let r = control_rect(&ui, |k| k.addr() == Some(addr::LpfCut));
    let y = r.cy();

    down(&mut ui, &host, r.cx(), y);
    move_to(&mut ui, &host, r.cx(), y - 18.0);
    assert_eq!(ui.cursor(), Cursor::ResizeV);
    assert_eq!(host.calls.borrow().begins.as_slice(), &[addr::LpfCut]);
    assert!(host.calls.borrow().ends.is_empty());

    ui.event(&host, Event::Cancel);
    ui.cancel_interaction(&host);
    up(&mut ui, &host, r.cx(), y - 18.0);

    assert_eq!(host.calls.borrow().ends.as_slice(), &[addr::LpfCut]);
    assert_eq!(ui.cursor(), Cursor::Default);
    assert!(ui.focus.is_none());
}

#[test]
fn interrupted_curve_node_drag_ends_both_host_parameters() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Mod, 100);
    ui.tick(&host);
    select_mod_source(&mut ui, &host, 3);
    let r = modpage::curve_box(0).inset(6.0);
    let addresses = CURVE_ADDRESSES[0];
    let x = r.x + host.get(addresses[1]) * r.w;
    let y = r.bottom() - host.get(addresses[2]) * r.h;

    down(&mut ui, &host, x, y);
    assert_eq!(ui.page_state.curve_drag, Some((0, 1)));
    assert_eq!(host.calls.borrow().begins.as_slice(), &[addresses[1], addresses[2]]);

    ui.event(&host, Event::Cancel);
    ui.cancel_interaction(&host);
    up(&mut ui, &host, x, y);

    assert_eq!(ui.page_state.curve_drag, None);
    assert_eq!(host.calls.borrow().ends.as_slice(), &[addresses[1], addresses[2]]);
}

#[test]
fn interrupted_pitch_wheel_returns_to_center_once() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Synth, 100);
    ui.tick(&host);
    let r = control_rect(&ui, |k| *k == Kind::Bender);
    down(&mut ui, &host, r.cx(), r.y + 10.0);
    assert!(ui.bend > 0.0);
    assert_ne!(host.calls.borrow().midi.last(), Some(&(0xE0, 0, 64)));

    ui.event(&host, Event::Cancel);
    ui.cancel_interaction(&host);
    up(&mut ui, &host, r.cx(), r.cy());

    assert_eq!(ui.bend, 0.0);
    let calls = host.calls.borrow();
    assert_eq!(calls.midi.last(), Some(&(0xE0, 0, 64)));
    assert_eq!(calls.midi.iter().filter(|&&m| m == (0xE0, 0, 64)).count(), 1);
}

#[test]
fn focus_loss_resets_a_latched_mod_wheel_once() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Synth, 100);
    ui.tick(&host);
    let r = control_rect(&ui, |k| *k == Kind::ModWheel);
    down(&mut ui, &host, r.cx(), r.cy());
    move_to(&mut ui, &host, r.cx(), r.cy() - 24.0);
    up(&mut ui, &host, r.cx(), r.cy() - 24.0);
    assert!(ui.mod_wheel > 0.0, "normal mouse-up leaves the modulation position latched");

    ui.event(&host, Event::Cancel);
    ui.cancel_interaction(&host);

    assert_eq!(ui.mod_wheel, 0.0);
    let calls = host.calls.borrow();
    assert_eq!(calls.midi.last(), Some(&(0xB0, 1, 0)));
    assert_eq!(calls.midi.iter().filter(|&&m| m == (0xB0, 1, 0)).count(), 1);
}

#[test]
fn long_popup_reveals_the_selection_and_cannot_scroll_past_its_items() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Mod, 100);
    select_mod_source(&mut ui, &host, 1);
    let address = addr::ModMatrixDest1;
    let count = PARAMS[address].item_count();
    let selected = count - 1;
    host.set(address, params::normalized_index(selected as i64, count) as f32);
    ui.tick(&host);
    let r = control_rect(&ui, |k| matches!(k, Kind::Stepper { addr: a } if *a == address));
    down(&mut ui, &host, r.cx(), r.cy());

    let menu = ui.menu.as_ref().expect("clicking the value opens a popup");
    assert_eq!(menu.addr, address);
    let max_scroll = (count as f32 * 20.0 + 8.0 - menu.r.h).max(0.0);
    assert!(max_scroll > 0.0, "the destination list exceeds the popup viewport");
    let row_top = menu.r.y + 4.0 + selected as f32 * 20.0 - menu.scroll;
    assert!(row_top >= menu.r.y + 4.0 && row_top + 20.0 <= menu.r.bottom() - 4.0,
        "the current destination must be fully visible when opening the list");

    ui.event(&host, Event::Wheel { dx: 0.0, dy: -100_000.0, mods: Mods::default() });
    assert_eq!(ui.menu.as_ref().unwrap().scroll, max_scroll);
    ui.event(&host, Event::Wheel { dx: 0.0, dy: 100_000.0, mods: Mods::default() });
    assert_eq!(ui.menu.as_ref().unwrap().scroll, 0.0);
    ui.event(&host, Event::Cancel);
    assert!(ui.menu.is_none());
    assert!(host.calls.borrow().begins.is_empty(), "browsing a popup must not change the parameter");
}

fn render(ui: &mut Ui, host: &dyn Host, fonts: &Fonts, pixmap: &mut Pixmap, scale: f32) {
    ui.tick(host);
    let mut canvas = Canvas::new(pixmap.as_mut(), scale, false, fonts);
    ui.render(host, &mut canvas);
}

fn assert_cached_matches_full(ui: &mut Ui, host: &dyn Host, fonts: &Fonts, pixmap: &mut Pixmap, scale: f32, stage: &str) {
    render(ui, host, fonts, pixmap, scale);
    let cached = pixmap.data().to_vec();
    ui.invalidate_all();
    render(ui, host, fonts, pixmap, scale);
    let first_difference = cached.iter().zip(pixmap.data()).position(|(a, b)| a != b);
    assert!(first_difference.is_none(), "cached frame differs from full rendering after {stage}; first byte: {first_difference:?}");
}

#[test]
fn cached_frames_match_full_rendering_after_cutoff_hover_drag_and_leave() {
    let host = TrackingHost::default();
    host.set(addr::LpfCut, 1.0);
    let mut ui = Ui::new(Page::Synth, 100);
    let fonts = Fonts::new();
    let scale = 1.0;
    let mut pixmap = Pixmap::new((layout::W * scale) as u32, (layout::H * scale) as u32).unwrap();
    render(&mut ui, &host, &fonts, &mut pixmap, scale);
    let r = control_rect(&ui, |k| k.addr() == Some(addr::LpfCut));
    let y = r.cy();

    move_to(&mut ui, &host, r.cx(), y);
    assert_cached_matches_full(&mut ui, &host, &fonts, &mut pixmap, scale, "hovering the cutoff control");
    down(&mut ui, &host, r.cx(), y);
    move_to(&mut ui, &host, r.cx(), y + 42.0);
    assert_cached_matches_full(&mut ui, &host, &fonts, &mut pixmap, scale, "dragging the cutoff control");
    up(&mut ui, &host, r.cx(), y + 42.0);
    ui.event(&host, Event::Leave);
    assert_cached_matches_full(&mut ui, &host, &fonts, &mut pixmap, scale, "removing a value tooltip");
    host.set(addr::LpfCut, 0.2);
    assert_cached_matches_full(&mut ui, &host, &fonts, &mut pixmap, scale, "host automation");
}

#[test]
fn restoring_the_library_tab_starts_a_scan_on_the_first_frame() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Library, 100);
    ui.tick(&host);
    assert!(ui.library.is_scanning() || !ui.library.entries().is_empty());
}

#[test]
fn extra_mouse_buttons_do_not_overwrite_an_active_parameter_gesture() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Synth, 100);
    ui.tick(&host);
    let r = control_rect(&ui, |k| k.addr() == Some(addr::LpfCut));
    let y = r.cy();
    down(&mut ui, &host, r.cx(), y);
    ui.event(&host, Event::Down { x: r.cx(), y, button: Button::Right, mods: Mods::default() });
    up(&mut ui, &host, r.cx(), y);
    assert_eq!(host.calls.borrow().begins.as_slice(), &[addr::LpfCut]);
    assert_eq!(host.calls.borrow().ends.as_slice(), &[addr::LpfCut]);
}

#[test]
fn modulation_source_selection_hits_its_own_destination_controls() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Mod, 100);
    ui.tick(&host);
    for (source, destination) in [(1, addr::ModMatrixDest1), (3, addr::ModMatrixDest11), (7, addr::ModMatrixDest23)] {
        select_mod_source(&mut ui, &host, source);
        let r = control_rect(&ui, |k| k.addr() == Some(destination));
        move_to(&mut ui, &host, r.cx(), r.cy());
        assert_eq!(ui.hovered().and_then(|c| c.kind.addr()), Some(destination));
        down(&mut ui, &host, r.cx(), r.cy());
        assert_eq!(ui.menu.as_ref().map(|m| m.addr), Some(destination));
        ui.event(&host, Event::Cancel);
    }
    assert!(host.calls.borrow().begins.is_empty());
}

#[test]
fn modulation_editors_have_separate_hit_targets_inside_the_workspace() {
    let host = TrackingHost::default();
    let ui = Ui::new(Page::Mod, 100);
    for source in 0..modpage::SOURCE_COUNT {
        let active: Vec<_> = ui.layout.controls.iter().filter(|c| {
            c.page == Some(Page::Mod)
                && c.view.map_or(true, |view| view == source)
                && c.when.map_or(true, |(a, item)| PARAMS[a].index_f32(host.get(a)) == item)
        }).collect();
        for (i, control) in active.iter().enumerate() {
            let r = control.r;
            assert!(r.w > 0.0 && r.h > 0.0 && r.x >= layout::BODY.x && r.y >= layout::BODY.y
                && r.right() <= layout::BODY.right() && r.bottom() <= layout::BODY.bottom(),
                "source {source}: control {:?} falls outside the modulation workspace", control.kind);
            for other in &active[i + 1..] {
                assert!(!r.intersects(&other.r), "source {source}: hit targets overlap: {:?} and {:?}", control.kind, other.kind);
            }
        }
    }
}

#[test]
fn classic_lfo_clock_exposes_the_effective_rate_control() {
    let host = TrackingHost::default();
    let mut ui = Ui::new(Page::Mod, 100);
    for (synced, expected) in [(false, addr::LFO_RateUnSynced), (true, addr::LFO_RateSynced)] {
        host.set(addr::LFO_MidiSync, if synced { 1.0 } else { 0.0 });
        ui.tick(&host);
        let control = ui.layout.controls.iter().find(|c| c.page == Some(Page::Mod)
            && c.view == Some(0) && c.kind.addr() == Some(expected)
            && c.when.map_or(true, |(a, item)| PARAMS[a].index_f32(host.get(a)) == item)).unwrap();
        let r = control.r;
        move_to(&mut ui, &host, r.cx(), r.cy());
        assert_eq!(ui.hovered().and_then(|c| c.kind.addr()), Some(expected));
    }
}
