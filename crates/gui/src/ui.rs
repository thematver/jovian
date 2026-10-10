//! The editor: owns the layout and interaction state, turns input events into parameter
//! changes and draws everything. Host and window agnostic.

use crate::canvas::{vgrad, Canvas, Fill, Rgba, R};
use crate::display;
use crate::host::{index_of, Host};
use crate::layout::{self, Control, Deco, Kind, Layout, Page};
use crate::pages;
use crate::text::{Face, Fonts, Style};
use crate::theme as t;
use crate::widgets as w;
use jovian_core::params::{self, Kind as PKind, COUNT, PARAMS};
use jovian_core::preset::LibraryEntry;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
    pub ctrl: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Enter,
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Tab,
    PageUp,
    PageDown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Down { x: f32, y: f32, button: Button, mods: Mods },
    Up { x: f32, y: f32, button: Button, mods: Mods },
    Move { x: f32, y: f32, mods: Mods },
    Leave,
    /// Lines (positive = up / away from the user).
    Wheel { dx: f32, dy: f32, mods: Mods },
    Key { key: Key, mods: Mods },
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cursor {
    #[default]
    Default,
    Hand,
    ResizeV,
    Text,
}

/// What an event did.
#[derive(Debug, Clone, Copy, Default)]
pub struct Response {
    /// The event was used (the host should not see it).
    pub captured: bool,
}

#[derive(Debug, Clone)]
enum Drag {
    Value { ctl: usize, addr: usize, start_v: f32, start_y: f32, start_x: f32, fine: bool, last_y: f32 },
    Keyboard { note: u8 },
    Bender,
    ModWheel { start_v: f32, start_y: f32 },
    Page(u32),
}

/// A popup list for discrete parameters with many items.
#[derive(Debug, Clone)]
pub struct Menu {
    pub addr: usize,
    pub r: R,
    pub scroll: f32,
    pub hover: Option<usize>,
}

pub const ZOOMS: [u32; 6] = [75, 85, 100, 115, 125, 150];

pub struct Ui {
    pub fonts: Fonts,
    pub layout: Layout,
    pub page: Page,
    pub zoom: u32,
    hover: Option<usize>,
    drag: Option<Drag>,
    last_click: Option<(Instant, usize)>,
    /// Parameter shown in the display and when it was last touched.
    pub focus: Option<(usize, Instant)>,
    bg: Option<(Page, f32, Pixmap)>,
    values: [f32; COUNT],
    /// Values of the patch as loaded (for the "modified" mark).
    loaded: [f32; COUNT],
    patch_gen: u64,
    pub patch_name: String,
    pub patch_bank: String,
    dirty: bool,
    pub mouse: (f32, f32),
    pub menu: Option<Menu>,
    pub bend: f32,
    pub mod_wheel: f32,
    pub held_note: Option<u8>,
    pub meters: jovian_core::dsp::Meters,
    pub library: pages::library::Library,
    pub page_state: pages::State,
    pub status: Option<(String, Instant)>,
    last: Frame,
    had_overlay: bool,
    force_full: bool,
    focus_shown: bool,
}

/// Signatures of what the last frame showed.
#[derive(Default)]
struct Frame {
    sigs: Vec<u64>,
    display: u64,
    page: u64,
    size: (u32, u32),
}

/// FNV-1a style signature builder.
pub struct Sig(pub u64);

impl Sig {
    pub fn new() -> Self {
        Sig(0xcbf2_9ce4_8422_2325)
    }
    pub fn add(&mut self, v: u64) {
        self.0 ^= v;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        self.0 ^= self.0 >> 29;
    }
    pub fn str(&mut self, s: &str) {
        for b in s.bytes() {
            self.add(b as u64);
        }
        self.add(0xFF);
    }
}

impl Default for Sig {
    fn default() -> Self {
        Self::new()
    }
}

/// Area a control may paint (shadows, glows, value labels).
fn bounds(c: &Control) -> R {
    match c.kind {
        Kind::Fader { .. } => c.r.inset_xy(-16.0, -6.0),
        Kind::Knob { .. } => c.r.inset(-6.0),
        Kind::Bender | Kind::ModWheel => R::new(c.r.x - 8.0, c.r.y - 4.0, c.r.w + 12.0, c.r.h + 20.0),
        Kind::Keyboard => c.r.inset(-2.0),
        _ => c.r.inset(-4.0),
    }
}

impl Ui {
    pub fn new(page: Page, zoom: u32) -> Self {
        Ui {
            fonts: Fonts::new(),
            layout: layout::build_all(),
            page,
            zoom,
            hover: None,
            drag: None,
            last_click: None,
            focus: None,
            bg: None,
            values: [-1.0; COUNT],
            loaded: [0.0; COUNT],
            patch_gen: 0,
            patch_name: String::new(),
            patch_bank: String::new(),
            dirty: true,
            mouse: (-1.0, -1.0),
            menu: None,
            bend: 0.0,
            mod_wheel: 0.0,
            held_note: None,
            meters: Default::default(),
            library: pages::library::Library::default(),
            page_state: pages::State::default(),
            status: None,
            last: Frame::default(),
            had_overlay: false,
            force_full: true,
            focus_shown: false,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        let z = self.zoom as f32 / 100.0;
        ((layout::W * z).round() as u32, (layout::H * z).round() as u32)
    }

    pub fn scale_for(&self, backing: f32) -> f32 {
        backing * self.zoom as f32 / 100.0
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// The next render redraws everything (the target buffer is new or was overwritten).
    pub fn invalidate_all(&mut self) {
        self.force_full = true;
        self.dirty = true;
    }

    pub fn invalidate_background(&mut self) {
        self.bg = None;
        self.dirty = true;
        self.force_full = true;
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = Some((s.into(), Instant::now()));
        self.dirty = true;
    }

    /// Polls the host for changes; returns true if a redraw is needed.
    pub fn tick(&mut self, host: &dyn Host) -> bool {
        let (name, bank, generation) = host.patch_info();
        if generation != self.patch_gen {
            self.patch_gen = generation;
            self.patch_name = name;
            self.patch_bank = bank;
            for i in 0..COUNT {
                self.loaded[i] = host.get(i);
            }
            self.dirty = true;
        } else if name != self.patch_name || bank != self.patch_bank {
            self.patch_name = name;
            self.patch_bank = bank;
            self.dirty = true;
        }
        for i in 0..COUNT {
            let v = host.get(i);
            if v != self.values[i] {
                self.values[i] = v;
                self.dirty = true;
            }
        }
        let m = host.meters();
        if m.active_voices != self.meters.active_voices
            || m.gated_voices != self.meters.gated_voices
            || (m.lfo - self.meters.lfo).abs() > 0.02
            || (m.env1 - self.meters.env1).abs() > 0.01
            || (m.env2 - self.meters.env2).abs() > 0.01
        {
            self.meters = m;
            self.dirty = true;
        }
        if let Some((_, at)) = &self.status {
            if at.elapsed() > Duration::from_secs(4) {
                self.status = None;
                self.dirty = true;
            }
        }
        if self.library.poll() {
            self.dirty = true;
        }
        let shown = display::focus(self).is_some();
        if shown != self.focus_shown {
            self.focus_shown = shown;
            self.dirty = true;
        }
        let d = self.dirty;
        d
    }

    pub fn is_modified(&self) -> bool {
        (0..COUNT).any(|i| (self.values[i] - self.loaded[i]).abs() > 1e-6)
    }

    fn visible(&self, c: &Control) -> bool {
        c.page.map_or(true, |p| p == self.page)
            && c.when.map_or(true, |(addr, i)| PARAMS[addr].index_f32(self.values[addr].max(0.0)) == i)
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        // later controls are on top
        (0..self.layout.controls.len()).rev().find(|&i| {
            let c = &self.layout.controls[i];
            self.visible(c) && hit_rect(c).contains(x, y)
        })
    }

    pub fn cursor(&self) -> Cursor {
        use crate::pages::library::{BANK, NAME, SEARCH};
        match (&self.drag, self.hover.map(|i| &self.layout.controls[i].kind)) {
            (Some(Drag::Value { .. }), _) => Cursor::ResizeV,
            (_, Some(Kind::Fader { .. } | Kind::Knob { .. })) => Cursor::ResizeV,
            (_, Some(Kind::Area(id))) if [SEARCH, NAME, BANK].contains(id) => Cursor::Text,
            (_, Some(_)) => Cursor::Hand,
            _ => Cursor::Default,
        }
    }

    // MARK: - events

    pub fn event(&mut self, host: &dyn Host, ev: Event) -> Response {
        let r = self.event_inner(host, ev);
        if r.captured {
            self.dirty = true;
        }
        r
    }

    fn event_inner(&mut self, host: &dyn Host, ev: Event) -> Response {
        let captured = Response { captured: true };
        if let Some(r) = pages::event(self, host, &ev) {
            return r;
        }
        match ev {
            Event::Move { x, y, mods } => {
                self.mouse = (x, y);
                if let Some(m) = &mut self.menu {
                    let h = menu_item_at(m, x, y);
                    if h != m.hover {
                        m.hover = h;
                        self.dirty = true;
                    }
                    return captured;
                }
                if let Some(d) = self.drag.clone() {
                    self.drag_to(host, d, x, y, mods);
                    return captured;
                }
                let h = self.hit(x, y);
                if h != self.hover {
                    self.hover = h;
                    if let Some(addr) = h.and_then(|i| self.layout.controls[i].kind.addr()) {
                        self.focus = Some((addr, Instant::now()));
                    }
                    self.dirty = true;
                }
                Response::default()
            }
            Event::Leave => {
                if self.drag.is_none() {
                    self.hover = None;
                    self.dirty = true;
                }
                Response::default()
            }
            Event::Down { x, y, button, mods } => {
                self.mouse = (x, y);
                if self.menu.is_some() {
                    self.menu_click(host, x, y);
                    return captured;
                }
                let Some(i) = self.hit(x, y) else { return Response::default() };
                let now = Instant::now();
                let double = matches!(self.last_click, Some((t0, j)) if j == i && now.duration_since(t0) < Duration::from_millis(350));
                self.last_click = Some((now, i));
                self.down(host, i, x, y, button, mods, double);
                captured
            }
            Event::Up { x, y, .. } => {
                self.mouse = (x, y);
                if let Some(d) = self.drag.take() {
                    match d {
                        Drag::Value { addr, .. } => host.end(addr),
                        Drag::Keyboard { note } => {
                            host.midi(0x80, note, 0);
                            self.held_note = None;
                        }
                        Drag::Bender => {
                            self.bend = 0.0;
                            host.midi(0xE0, 0, 64);
                        }
                        Drag::Page(id) => pages::area_up(self, host, id),
                        _ => {}
                    }
                    self.hover = self.hit(x, y);
                    return captured;
                }
                Response::default()
            }
            Event::Wheel { dy, mods, .. } => {
                if let Some(m) = &mut self.menu {
                    m.scroll = (m.scroll - dy * 18.0).max(0.0);
                    return captured;
                }
                let Some(i) = self.hit(self.mouse.0, self.mouse.1) else { return Response::default() };
                self.wheel(host, i, dy, mods);
                captured
            }
            Event::Key { key: Key::Escape, .. } if self.menu.is_some() => {
                self.menu = None;
                captured
            }
            _ => Response::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn down(&mut self, host: &dyn Host, i: usize, x: f32, y: f32, button: Button, mods: Mods, double: bool) {
        let c = self.layout.controls[i].clone();
        if let Some(addr) = c.kind.addr() {
            self.focus = Some((addr, Instant::now()));
            let reset = double || mods.cmd || (mods.alt && !matches!(c.kind, Kind::Switch { .. }));
            if reset && !matches!(c.kind, Kind::Switch { .. } | Kind::Toggle { .. }) {
                host.set_once(addr, PARAMS[addr].default as f32);
                return;
            }
        }
        match &c.kind {
            Kind::Fader { addr, .. } | Kind::Knob { addr, .. } | Kind::HSlider { addr, .. } => {
                if button == Button::Right {
                    return;
                }
                host.begin(*addr);
                let v = host.get(*addr);
                // a click on the fader track (not the cap) jumps there, like a hardware slider
                let mut start = v;
                if let Kind::Fader { .. } = c.kind {
                    let cap_y = w::fader_cap_y(c.r, v);
                    if (y - cap_y).abs() > w::FADER_CAP_H * 0.5 + 2.0 {
                        let g = w::fader_geom(c.r);
                        start = ((g.bottom - y) / (g.bottom - g.top)).clamp(0.0, 1.0);
                        host.set(*addr, quantize(*addr, start));
                    }
                } else if let Kind::HSlider { .. } = c.kind {
                    start = ((x - c.r.x) / c.r.w).clamp(0.0, 1.0);
                    host.set(*addr, quantize(*addr, start));
                }
                self.drag = Some(Drag::Value { ctl: i, addr: *addr, start_v: start, start_y: y, start_x: x, fine: mods.shift, last_y: y });
            }
            Kind::Switch { addr, order, vertical, .. } => {
                let items = w::switch_items(c.r, order.len(), *vertical, if *vertical { 4.0 } else { 3.0 });
                if let Some(k) = items.iter().position(|r| r.contains(x, y)) {
                    let n = PARAMS[*addr].item_count();
                    host.set_once(*addr, params::normalized_index(order[k] as i64, n) as f32);
                }
            }
            Kind::Toggle { addr, .. } => {
                let on = index_of(host, *addr) == 1;
                let n = PARAMS[*addr].item_count();
                host.set_once(*addr, params::normalized_index(if on { 0 } else { 1 }, n) as f32);
            }
            Kind::Stepper { addr } => {
                if x < c.r.x + 14.0 {
                    self.step(host, *addr, -1);
                } else if x > c.r.right() - 14.0 {
                    self.step(host, *addr, 1);
                } else {
                    self.open_menu(host, *addr, c.r);
                }
            }
            Kind::Tab(p) => self.set_page(*p),
            Kind::PatchPrev => self.step_patch(host, -1),
            Kind::PatchNext => self.step_patch(host, 1),
            Kind::PatchName => self.set_page(if self.page == Page::Library { Page::Synth } else { Page::Library }),
            Kind::Zoom => {
                let k = ZOOMS.iter().position(|z| *z == self.zoom).unwrap_or(2);
                let next = ZOOMS[(k + 1) % ZOOMS.len()];
                let old = self.zoom;
                self.zoom = next;
                let (ww, hh) = self.size();
                if host.resize(ww, hh) {
                    self.bg = None;
                    self.set_status(format!("Zoom {next} %"));
                } else {
                    self.zoom = old;
                    self.set_status("The host does not allow resizing");
                }
            }
            Kind::Keyboard => {
                if let Some((note, vel)) = key_at(c.r, x, y) {
                    host.midi(0x90, note, vel);
                    self.held_note = Some(note);
                    self.drag = Some(Drag::Keyboard { note });
                }
            }
            Kind::Bender => {
                self.drag = Some(Drag::Bender);
                self.drag_to(host, Drag::Bender, x, y, mods);
            }
            Kind::ModWheel => {
                self.drag = Some(Drag::ModWheel { start_v: self.mod_wheel, start_y: y });
            }
            Kind::Area(id) => {
                let id = *id;
                pages::area_down(self, host, id, c.r, x, y, double);
                self.drag = Some(Drag::Page(id));
            }
        }
    }

    fn drag_to(&mut self, host: &dyn Host, d: Drag, x: f32, y: f32, mods: Mods) {
        match d {
            Drag::Value { ctl, addr, start_v, start_y, start_x, fine, last_y } => {
                let c = &self.layout.controls[ctl];
                let span = match c.kind {
                    Kind::Fader { .. } => {
                        let g = w::fader_geom(c.r);
                        g.bottom - g.top
                    }
                    Kind::HSlider { .. } => c.r.w,
                    _ => 180.0,
                };
                // switching fine mode mid-drag re-anchors so the value does not jump
                if mods.shift != fine {
                    let v = host.get(addr);
                    self.drag = Some(Drag::Value { ctl, addr, start_v: v, start_y: y, start_x: x, fine: mods.shift, last_y: y });
                    return;
                }
                let k = if fine { 0.1 } else { 1.0 };
                let delta = if let Kind::HSlider { .. } = c.kind { (x - start_x) / span } else { (start_y - y) / span };
                let v = (start_v + delta * k).clamp(0.0, 1.0);
                host.set(addr, quantize(addr, v));
                self.focus = Some((addr, Instant::now()));
                self.drag = Some(Drag::Value { ctl, addr, start_v, start_y, start_x, fine, last_y: y });
                let _ = last_y;
            }
            Drag::Keyboard { note } => {
                let kr = self.layout.controls.iter().find(|c| c.kind == Kind::Keyboard).map(|c| c.r);
                if let Some((n, vel)) = kr.and_then(|r| key_at(r, x, y)) {
                    if n != note {
                        host.midi(0x80, note, 0);
                        host.midi(0x90, n, vel);
                        self.held_note = Some(n);
                        self.drag = Some(Drag::Keyboard { note: n });
                    }
                }
            }
            Drag::Bender => {
                let r = self.layout.controls.iter().find(|c| c.kind == Kind::Bender).map(|c| c.r).unwrap_or_default();
                let v = ((r.cy() - y) / (r.h * 0.5 - 8.0)).clamp(-1.0, 1.0);
                self.bend = v;
                let b = ((v + 1.0) * 8191.5).round().clamp(0.0, 16383.0) as u16;
                host.midi(0xE0, (b & 0x7F) as u8, (b >> 7) as u8);
            }
            Drag::ModWheel { start_v, start_y } => {
                let r = self.layout.controls.iter().find(|c| c.kind == Kind::ModWheel).map(|c| c.r).unwrap_or_default();
                let v = (start_v + (start_y - y) / (r.h - 16.0)).clamp(0.0, 1.0);
                self.mod_wheel = v;
                host.midi(0xB0, 1, (v * 127.0).round() as u8);
            }
            Drag::Page(id) => pages::area_drag(self, host, id, x, y, mods),
        }
    }

    fn wheel(&mut self, host: &dyn Host, i: usize, dy: f32, mods: Mods) {
        let c = self.layout.controls[i].clone();
        let Some(addr) = c.kind.addr() else {
            if let Kind::Area(id) = c.kind {
                pages::area_wheel(self, host, id, dy);
            }
            return;
        };
        self.focus = Some((addr, Instant::now()));
        let spec = &PARAMS[addr];
        if spec.is_discrete() {
            let dir = if dy > 0.0 { 1 } else { -1 };
            // switches are drawn top-down in display order; wheel up moves up the list
            let dir = match &c.kind {
                Kind::Switch { order, vertical: true, .. } => {
                    let cur = index_of(host, addr);
                    let pos = order.iter().position(|o| *o == cur).unwrap_or(0) as i64;
                    let np = (pos - dir).clamp(0, order.len() as i64 - 1) as usize;
                    host.set_once(addr, params::normalized_index(order[np] as i64, spec.item_count()) as f32);
                    return;
                }
                _ => dir,
            };
            self.step(host, addr, dir);
        } else {
            let step = if mods.shift { 0.002 } else { 0.02 };
            let v = (host.get(addr) + dy.signum() * step * dy.abs().min(4.0)).clamp(0.0, 1.0);
            host.set_once(addr, v);
        }
    }

    fn step(&mut self, host: &dyn Host, addr: usize, dir: i64) {
        let n = PARAMS[addr].item_count();
        let i = (index_of(host, addr) as i64 + dir).clamp(0, n as i64 - 1);
        host.set_once(addr, params::normalized_index(i, n) as f32);
        self.focus = Some((addr, Instant::now()));
    }

    pub fn set_page(&mut self, p: Page) {
        if self.page != p {
            self.page = p;
            self.hover = None;
            self.menu = None;
            self.dirty = true;
            if p == Page::Library {
                self.library.ensure_scanned();
            }
        }
    }

    /// Loads the previous / next preset of the library list.
    pub fn step_patch(&mut self, host: &dyn Host, dir: i64) {
        self.library.ensure_scanned_blocking();
        let list: Vec<LibraryEntry> = self.library.filtered();
        if list.is_empty() {
            return;
        }
        let cur = self.library.current.as_ref().and_then(|k| list.iter().position(|e| &e.key() == k));
        let i = match cur {
            Some(i) => (i as i64 + dir).rem_euclid(list.len() as i64) as usize,
            None => {
                if dir > 0 {
                    0
                } else {
                    list.len() - 1
                }
            }
        };
        let e = list[i].clone();
        self.load_entry(host, &e);
    }

    pub fn load_entry(&mut self, host: &dyn Host, e: &LibraryEntry) {
        match e.load() {
            Ok(p) => {
                host.load_patch(&p);
                self.library.current = Some(e.key());
                self.library.reveal_current = true;
            }
            Err(err) => self.set_status(format!("Could not open {}: {err}", e.name)),
        }
        self.dirty = true;
    }

    // MARK: - menus

    fn open_menu(&mut self, _host: &dyn Host, addr: usize, anchor: R) {
        let n = PARAMS[addr].item_count();
        let ih = 20.0;
        let h = (n as f32 * ih + 8.0).min(layout::H - 40.0);
        let wdt = (PARAMS[addr].items().iter().map(|s| self.fonts.measure(s, &menu_style())).fold(0.0, f32::max) + 40.0).max(anchor.w);
        let mut y = anchor.bottom() + 4.0;
        if y + h > layout::H - 8.0 {
            y = (anchor.y - h - 4.0).max(8.0);
        }
        let x = anchor.x.min(layout::W - wdt - 8.0);
        self.menu = Some(Menu { addr, r: R::new(x, y, wdt, h), scroll: 0.0, hover: None });
    }

    fn menu_click(&mut self, host: &dyn Host, x: f32, y: f32) {
        if let Some(m) = self.menu.take() {
            if let Some(i) = menu_item_at(&m, x, y) {
                let n = PARAMS[m.addr].item_count();
                host.set_once(m.addr, params::normalized_index(i as i64, n) as f32);
                self.focus = Some((m.addr, Instant::now()));
            }
        }
    }

    // MARK: - rendering

    /// Draws the editor. `canvas.scale` must be backing scale × zoom. The canvas must keep the
    /// previous frame: only regions whose state changed are redrawn.
    pub fn render(&mut self, host: &dyn Host, c: &mut Canvas) {
        let key = (self.page, c.scale);
        let stale = !matches!(&self.bg, Some((p, s, pm)) if *p == key.0 && *s == key.1 && pm.width() == c.pix.width() && pm.height() == c.pix.height());
        if stale {
            if let Some(mut pm) = Pixmap::new(c.pix.width(), c.pix.height()) {
                {
                    let mut bc = Canvas::new(pm.as_mut(), c.scale, c.swap_rb, &self.fonts);
                    self.draw_background(&mut bc);
                }
                self.bg = Some((self.page, c.scale, pm));
            }
        }
        let sigs: Vec<u64> = (0..self.layout.controls.len()).map(|i| self.control_sig(host, i)).collect();
        let disp = self.display_sig(host);
        let page = self.page_sig(host);
        let overlay = self.menu.is_some() || self.status.is_some();
        let full = stale || self.force_full || overlay || self.had_overlay || self.last.sigs.len() != sigs.len() || self.last.size != (c.pix.width(), c.pix.height());
        if full {
            if let Some((_, _, pm)) = &self.bg {
                c.blit_all(pm.as_ref());
            }
            self.draw_all(host, c);
        } else {
            let mut regions: Vec<R> = Vec::new();
            let body_page = self.page != Page::Synth;
            let mut body_dirty = body_page && page != self.last.page;
            for (i, sig) in sigs.iter().enumerate() {
                if *sig != self.last.sigs[i] {
                    let ctl = &self.layout.controls[i];
                    let b = bounds(ctl);
                    if body_page && ctl.page.is_some() {
                        // page controls share the area with dynamic page content
                        body_dirty = true;
                    } else {
                        regions.push(b);
                    }
                }
            }
            if body_dirty {
                regions.push(layout::BODY.inset(-4.0));
            }
            let disp_dirty = disp != self.last.display;
            if disp_dirty {
                regions.push(layout::display_rect().inset(-2.0));
            }
            // grow every region over the controls it touches so nothing is composited twice
            let regions = self.close_regions(regions);
            for r in &regions {
                if let Some((_, _, pm)) = &self.bg {
                    c.blit_region(pm.as_ref(), *r);
                }
                for i in 0..self.layout.controls.len() {
                    let ctl = &self.layout.controls[i];
                    if self.visible(ctl) && bounds(ctl).intersects(r) {
                        self.draw_one(host, c, i);
                    }
                }
                if body_page && r.intersects(&layout::BODY.inset(4.0)) {
                    pages::draw(self, host, c);
                }
                if r.intersects(&layout::display_rect()) {
                    display::draw(self, host, c, layout::display_rect());
                }
            }
        }
        self.last = Frame { sigs, display: disp, page, size: (c.pix.width(), c.pix.height()) };
        self.had_overlay = overlay;
        self.force_full = false;
        self.dirty = false;
    }

    fn close_regions(&self, mut regions: Vec<R>) -> Vec<R> {
        loop {
            let mut changed = false;
            for r in regions.iter_mut() {
                for ctl in &self.layout.controls {
                    if !self.visible(ctl) {
                        continue;
                    }
                    let b = bounds(ctl);
                    if b.intersects(r) {
                        let u = r.union(&b);
                        if u != *r {
                            *r = u;
                            changed = true;
                        }
                    }
                }
            }
            // merge overlapping regions
            let mut merged: Vec<R> = Vec::new();
            for r in regions {
                if let Some(m) = merged.iter_mut().find(|m| m.intersects(&r)) {
                    *m = m.union(&r);
                    changed = true;
                } else {
                    merged.push(r);
                }
            }
            regions = merged;
            if !changed {
                return regions;
            }
        }
    }

    fn control_sig(&self, host: &dyn Host, i: usize) -> u64 {
        let ctl = &self.layout.controls[i];
        if !self.visible(ctl) {
            return 0;
        }
        let mut h = Sig::new();
        h.add(1);
        let hover = self.hover == Some(i) && self.menu.is_none();
        h.add(hover as u64);
        h.add(matches!(&self.drag, Some(Drag::Value { ctl, .. }) if *ctl == i) as u64);
        if let Some(addr) = ctl.kind.addr() {
            h.add(host.get(addr).to_bits() as u64);
            h.add(inactive(addr, host) as u64);
            if hover {
                if let Kind::Switch { .. } = ctl.kind {
                    h.add(self.mouse.0.to_bits() as u64);
                    h.add(self.mouse.1.to_bits() as u64);
                }
            }
        }
        match &ctl.kind {
            Kind::Keyboard => h.add(self.held_note.map_or(999, |n| n as u64)),
            Kind::Bender => h.add(self.bend.to_bits() as u64),
            Kind::ModWheel => h.add(self.mod_wheel.to_bits() as u64),
            Kind::Tab(p) => h.add((self.page == *p) as u64),
            Kind::Zoom => h.add(self.zoom as u64),
            _ => {}
        }
        h.0
    }

    fn display_sig(&self, host: &dyn Host) -> u64 {
        let mut h = Sig::new();
        h.str(&self.patch_name);
        h.str(&self.patch_bank);
        h.add(self.is_modified() as u64);
        match display::focus(self) {
            Some(addr) => {
                h.add(addr as u64 + 1);
                // the graph may depend on the whole group
                let g = PARAMS[addr].group;
                for (i, p) in PARAMS.iter().enumerate() {
                    if p.group == g || addr == i {
                        h.add(host.get(i).to_bits() as u64);
                    }
                }
                if matches!(g, jovian_core::params::Group::Vcf | jovian_core::params::Group::Vco) {
                    for a in [jovian_core::params::addr::LpfCut, jovian_core::params::addr::LpfRes, jovian_core::params::addr::LpfSlop, jovian_core::params::addr::HPF_Cutoff] {
                        h.add(host.get(a).to_bits() as u64);
                    }
                }
            }
            None => {
                let m = self.meters;
                h.add(m.active_voices as u64);
                h.add(m.gated_voices as u64);
                h.add((m.lfo * 20.0) as i64 as u64);
                h.add((m.env1 * 60.0) as i64 as u64);
                h.add((m.env2 * 60.0) as i64 as u64);
            }
        }
        h.0
    }

    fn page_sig(&self, host: &dyn Host) -> u64 {
        if self.page == Page::Synth {
            return 0;
        }
        let mut h = Sig::new();
        for i in 0..COUNT {
            h.add(host.get(i).to_bits() as u64);
        }
        h.add(self.library.signature());
        h.add(self.mouse.0.to_bits() as u64);
        h.add(self.mouse.1.to_bits() as u64);
        h.add(self.page_state.curve_drag.map_or(0, |(a, b)| (a * 8 + b + 1) as u64));
        h.add(self.page_state.eq_drag.map_or(0, |(a, b)| (a * 8 + b + 1) as u64));
        for s in 0..3 {
            h.add((host.compressor_reduction(s) * 4.0) as i64 as u64);
        }
        h.str(&self.patch_name);
        h.0
    }

    fn draw_one(&self, host: &dyn Host, c: &mut Canvas, i: usize) {
        let ctl = &self.layout.controls[i];
        let hover = self.hover == Some(i) && self.menu.is_none();
        let active = matches!(&self.drag, Some(Drag::Value { ctl, .. }) if *ctl == i);
        draw_control(self, host, c, ctl, hover, active);
        if let Some(addr) = ctl.kind.addr() {
            if inactive(addr, host) && !active {
                // controls that currently have no effect are veiled, like a dimmed panel label
                let r = match ctl.kind {
                    Kind::Fader { .. } => ctl.r.inset_xy(-4.0, -2.0),
                    _ => ctl.r.inset(-1.0),
                };
                c.rrect(r, 3.0, Rgba::hex(0x1F1F22).with_alpha(0.62));
            }
        }
    }

    fn draw_all(&mut self, host: &dyn Host, c: &mut Canvas) {
        for i in 0..self.layout.controls.len() {
            if self.visible(&self.layout.controls[i]) {
                self.draw_one(host, c, i);
            }
        }
        pages::draw(self, host, c);
        display::draw(self, host, c, layout::display_rect());
        if let Some(m) = &self.menu {
            draw_menu(c, host, m);
        }
        if let Some((s, _)) = &self.status {
            let st = Style::new(Face::Medium, 11.0, 0.0);
            let wdt = c.fonts.measure(s, &st) + 28.0;
            let r = R::new((layout::W - wdt) * 0.5, layout::H - 44.0, wdt, 26.0);
            c.shadow(r, 13.0, 8.0, 3.0, Rgba::black(0.6));
            c.rrect(r, 13.0, Rgba::hex(0x2E2E33));
            c.rrect_stroke(r, 13.0, 0.75, Rgba::white(0.12));
            c.text_in(s, r, &st, t::INK, 0.5);
        }
    }

    fn draw_background(&self, c: &mut Canvas) {
        c.rect(R::new(0.0, 0.0, layout::W, layout::H), t::WINDOW);
        // header
        let hr = R::new(0.0, 0.0, layout::W, layout::HEADER_H);
        c.rect(hr, vgrad(0.0, hr.h, t::HEADER_TOP, t::HEADER_BOTTOM));
        c.hline(0.0, layout::W, layout::HEADER_H - 1.0, 1.0, Rgba::black(0.9));
        c.hline(0.0, layout::W, layout::HEADER_H, 1.0, Rgba::white(0.05));
        draw_logo(c, 20.0, 31.0);
        display::draw_static(c, layout::display_rect());
        if let Some(k) = self.layout.controls.iter().find(|k| k.kind == Kind::Keyboard) {
            keyboard_static(c, k.r);
        }
        // body
        for (p, d) in &self.layout.decos {
            if p.map_or(true, |p| p == self.page) {
                if let Deco::Section { .. } = d {
                    draw_deco(c, d);
                }
            }
        }
        grain(c);
        for (p, d) in &self.layout.decos {
            if p.map_or(true, |p| p == self.page) {
                if !matches!(d, Deco::Section { .. }) {
                    draw_deco(c, d);
                }
            }
        }
        pages::draw_background(self, c);
    }

    pub fn hovered(&self) -> Option<&Control> {
        self.hover.map(|i| &self.layout.controls[i])
    }

    pub fn dragging_value(&self) -> Option<usize> {
        match &self.drag {
            Some(Drag::Value { addr, .. }) => Some(*addr),
            _ => None,
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
}

/// A faint, fixed grain over the panels (like bead-blasted metal); drawn once into the cache.
fn grain(c: &mut Canvas) {
    let w = c.pix.width() as usize;
    let data = c.pix.data_mut();
    let mut seed = 0x2545_F491u32;
    for (i, px) in data.chunks_exact_mut(4).enumerate() {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        // a little coarser than one device pixel, and slightly streaked horizontally
        let row = (i / w) as u32;
        let n = ((seed >> 24) as i32 - 128) / 64 + ((row.wrapping_mul(2_654_435_761) >> 30) as i32 - 2) / 2;
        for ch in px.iter_mut().take(3) {
            *ch = (*ch as i32 + n).clamp(0, 255) as u8;
        }
    }
}

/// True when a parameter currently has no effect (its mode switch points elsewhere).
pub fn inactive(addr: usize, host: &dyn Host) -> bool {
    use jovian_core::params::addr as a;
    let idx = |x: usize| index_of(host, x);
    match addr {
        a::LFO_RateUnSynced => idx(a::LFO_MidiSync) == 1,
        a::LFO_RateSynced => idx(a::LFO_MidiSync) == 0,
        a::ArpSeq_RateUnSynced => idx(a::ArpSeq_MidiSync) == 1,
        a::ArpSeq_RateSynced => idx(a::ArpSeq_MidiSync) == 0,
        a::LFO1_RateUnSynced => idx(a::LFO1_RateMode) != 0,
        a::LFO1_RateSynced => idx(a::LFO1_RateMode) == 0,
        a::LFO2_RateUnSynced => idx(a::LFO2_RateMode) != 0,
        a::LFO2_RateSynced => idx(a::LFO2_RateMode) == 0,
        a::UniVPN | a::VoiceDetuneAmount => idx(a::AssignMode) != 1,
        a::Polyphon => idx(a::AssignMode) == 0,
        a::PlPrio => idx(a::AssignMode) >= 2,
        a::Portamento_Time => idx(a::Portamento_On) == 0,
        a::BendVCFAmount => idx(a::BendVCF) == 0,
        a::LfoModVCOAmount => idx(a::LfoModVCO) == 0,
        a::LfoModVcfAmount => idx(a::LfoModVCF) == 0,
        _ => {
            let id = PARAMS[addr].id;
            if let Some(rest) = id.strip_prefix("ModMatrixAmount_Src") {
                // amount of a matrix slot without destination
                let d = rest.split("_Dest").nth(1).unwrap_or("");
                return jovian_core::params::address(&format!("ModMatrixDest{d}")).is_some_and(|da| idx(da) == 0);
            }
            if id.len() > 4 && id.starts_with("FX") && &id[3..4] == "_" {
                let fx = |sfx: &str| jovian_core::params::address(&format!("{}{}", &id[..4], sfx)).map(idx).unwrap_or(0);
                return match &id[4..] {
                    "Delay_DelayTimeMs" => fx("Delay_MidiSync") == 1,
                    "Delay_DelayTimeSynced" | "Delay_RateSyncedType" => fx("Delay_MidiSync") == 0,
                    "Delay_StereoWidth" => fx("Delay_StereoMode") == 1,
                    "Delay_StereoWidthPP" => fx("Delay_StereoMode") == 0,
                    "Phaser_LFO_RateUnSynced" => fx("Phaser_LFO_MidiSync") == 1,
                    "Phaser_LFO_RateSynced" | "Phaser_LFO_RateSyncedType" => fx("Phaser_LFO_MidiSync") == 0,
                    "StereoPan_LFOSimple_RateUnSynced" => fx("StereoPan_LFOSimple_RateType") != 0,
                    "StereoPan_LFOSimple_RateSynced" => fx("StereoPan_LFOSimple_RateType") == 0,
                    "MultiModeFilter_CombFreq" => fx("MultiModeFilter_Mode") < 3,
                    "MultiModeFilter_Freq" | "MultiModeFilter_Slope" => fx("MultiModeFilter_Mode") >= 3,
                    _ => false,
                };
            }
            false
        }
    }
}

fn hit_rect(c: &Control) -> R {
    match c.kind {
        Kind::Fader { .. } => c.r.inset_xy(-2.0, -4.0),
        _ => c.r,
    }
}

/// Snaps discrete parameters to their items while dragging.
fn quantize(addr: usize, v: f32) -> f32 {
    let p = &PARAMS[addr];
    match p.kind {
        PKind::Discrete(items) => {
            let n = items.len();
            let i = (v * (n - 1) as f32).round() as i64;
            params::normalized_index(i, n) as f32
        }
        _ => v,
    }
}

fn menu_style() -> Style {
    Style::new(Face::Medium, 11.5, 0.0)
}

fn menu_item_at(m: &Menu, x: f32, y: f32) -> Option<usize> {
    if !m.r.contains(x, y) {
        return None;
    }
    let i = ((y - m.r.y - 4.0 + m.scroll) / 20.0).floor();
    let n = PARAMS[m.addr].item_count();
    (i >= 0.0 && (i as usize) < n).then_some(i as usize)
}

fn draw_menu(c: &mut Canvas, host: &dyn Host, m: &Menu) {
    let r = m.r;
    c.shadow(r, 6.0, 14.0, 6.0, Rgba::black(0.7));
    c.rrect(r, 6.0, Rgba::hex(0x2A2A2F));
    c.rrect_stroke(r, 6.0, 0.75, Rgba::white(0.14));
    let cur = index_of(host, m.addr);
    c.set_clip(r.inset(2.0));
    for (i, s) in PARAMS[m.addr].items().iter().enumerate() {
        let ir = R::new(r.x + 4.0, r.y + 4.0 + i as f32 * 20.0 - m.scroll, r.w - 8.0, 20.0);
        if ir.bottom() < r.y || ir.y > r.bottom() {
            continue;
        }
        if m.hover == Some(i) {
            c.rrect(ir, 4.0, t::ORANGE.with_alpha(0.9));
        }
        let ink = if m.hover == Some(i) { t::INK_DARK } else if i == cur { t::AMBER } else { t::INK };
        if i == cur {
            c.circle(ir.x + 8.0, ir.cy(), 2.5, ink);
        }
        c.text_in(s, ir.inset_xy(18.0, 0.0), &menu_style(), ink, 0.0);
    }
    c.clear_clip();
}

fn draw_logo(c: &mut Canvas, x: f32, base: f32) {
    let st = Style::new(Face::Bold, 21.0, 4.2);
    let wdt = c.text("JOVIAN", x, base, &st, t::INK, 0.0);
    // the dash between name and number in the Jupiter colours
    w::rainbow(c, x + wdt + 5.0, base - 9.5, 22.0, 4.0);
    let st8 = Style::new(Face::Bold, 21.0, 0.0);
    c.text("8", x + wdt + 32.0, base, &st8, t::INK, 0.0);
    let sub = Style::new(Face::CondSemi, 8.5, 2.2);
    c.text("POLYPHONIC SYNTHESIZER", x + 1.0, base + 12.0, &sub, t::INK_DIM, 0.0);
}

fn draw_deco(c: &mut Canvas, d: &Deco) {
    match d {
        Deco::Section { r, title, accent } => w::section(c, *r, title, *accent),
        Deco::Caption { text, cx, y } => w::caption(c, text, *cx, *y),
        Deco::Bracket { x0, x1, y, text } => w::bracket(c, *x0, *x1, *y, text),
        Deco::FaderSlot { r, origin } => w::fader_static(c, *r, *origin),
        Deco::KnobRing { cx, cy, radius } => w::knob_static(c, *cx, *cy, *radius),
        Deco::Label { text, x, y, align } => {
            c.text(text, *x, *y, &t::SMALL, t::INK_DIM, *align);
        }
    }
}

/// Note and velocity under a point of the keyboard.
pub fn key_at(r: R, x: f32, y: f32) -> Option<(u8, u8)> {
    let keys = drawn_keys(r);
    // black keys first (they are on top)
    for (note, kr, black) in keys.iter().filter(|k| k.2).chain(keys.iter().filter(|k| !k.2)) {
        if kr.contains(x, y) {
            let v = ((y - kr.y) / kr.h).clamp(0.0, 1.0);
            let vel = (40.0 + 87.0 * v).round() as u8;
            let _ = black;
            return Some((*note, vel));
        }
    }
    None
}

/// Rectangles of every key: (note, rect, is_black).
pub fn keyboard_keys(r: R) -> Vec<(u8, R, bool)> {
    let whites: Vec<u8> = (layout::KEY_LO..=layout::KEY_HI).filter(|n| !is_black(*n)).collect();
    let ww = r.w / whites.len() as f32;
    let top = r.y + 6.0;
    let h = r.h - 6.0;
    let mut out = Vec::new();
    let mut wi = 0;
    for n in layout::KEY_LO..=layout::KEY_HI {
        if is_black(n) {
            let x = r.x + wi as f32 * ww;
            let off = match n % 12 {
                1 | 6 => -0.62,
                3 | 10 => -0.38,
                _ => -0.5,
            };
            out.push((n, R::new(x + ww * off, top, ww * 0.62, h * 0.6), true));
        } else {
            out.push((n, R::new(r.x + wi as f32 * ww, top, ww, h), false));
            wi += 1;
        }
    }
    out
}

pub fn is_black(n: u8) -> bool {
    matches!(n % 12, 1 | 3 | 6 | 8 | 10)
}

fn draw_control(ui: &Ui, host: &dyn Host, c: &mut Canvas, ctl: &Control, hover: bool, active: bool) {
    let r = ctl.r;
    match &ctl.kind {
        Kind::Fader { addr, label, color, origin } => {
            let v = host.get(*addr);
            let val = PARAMS[*addr].display_string(v as f64);
            w::fader(c, r, v, *color, *origin, label, Some(&val), hover, active);
        }
        Kind::Switch { addr, items, order, color, vertical } => {
            let cur = index_of(host, *addr);
            let rects = w::switch_items(r, order.len(), *vertical, if *vertical { 4.0 } else { 3.0 });
            let small = !*vertical && order.len() > 5;
            for (k, ir) in rects.iter().enumerate() {
                let it = &items[order[k]];
                let h = hover && ir.contains(ui.mouse.0, ui.mouse.1);
                w::button(c, *ir, it, *color, order[k] == cur, h, small);
            }
        }
        Kind::Toggle { addr, label, color } => {
            let on = index_of(host, *addr) == 1;
            w::button(c, r, &w::Item::Text(label), *color, on, hover, r.w < 36.0);
        }
        Kind::Knob { addr, label, color, bipolar, radius } => {
            let v = host.get(*addr);
            let cy = r.y + radius + 6.0;
            w::knob_static(c, r.cx(), cy, *radius);
            w::knob(c, r.cx(), cy, *radius, v, *color, *bipolar, hover, active);
            let ly = cy + radius + 18.0;
            if hover || active {
                let val = PARAMS[*addr].display_string(v as f64);
                c.text(&val, r.cx(), ly, &t::VALUE, t::AMBER, 0.5);
            } else {
                c.text(label, r.cx(), ly, &t::LABEL, t::INK.with_alpha(0.86), 0.5);
            }
        }
        Kind::Stepper { addr } => {
            let v = host.get(*addr);
            let s = short_item(*addr, PARAMS[*addr].display_string(v as f64));
            w::value_box(c, r, &s, hover, true);
        }
        Kind::HSlider { addr, color, bipolar } => {
            w::hslider(c, r.inset_xy(6.0, 0.0), host.get(*addr), *color, *bipolar, hover, active);
        }
        Kind::Tab(p) => w::tab(c, r, p.title(), ui.page == *p, hover),
        Kind::PatchPrev | Kind::PatchNext => {
            let col = if hover { t::INK } else { t::INK_DIM };
            let dir = if ctl.kind == Kind::PatchPrev { -1.0 } else { 1.0 };
            if hover {
                c.rrect(r, 4.0, Rgba::white(0.06));
            }
            w::chevron(c, r.cx(), r.cy(), dir, col);
        }
        Kind::PatchName => {
            if hover {
                c.rrect(r, 4.0, Rgba::white(0.04));
            }
        }
        Kind::Zoom => {
            if hover {
                c.rrect(r, 4.0, Rgba::white(0.06));
            }
            let st = Style::new(Face::Mono, 9.0, 0.0);
            c.text_in(&format!("{}%", ui.zoom), r, &st, if hover { t::INK } else { t::INK_DIM }, 0.5);
        }
        Kind::Keyboard => draw_keyboard(ui, c, r),
        Kind::Bender => draw_wheel(c, r, ui.bend * 0.5 + 0.5, true, hover, "BEND"),
        Kind::ModWheel => draw_wheel(c, r, ui.mod_wheel, false, hover, "MOD"),
        Kind::Area(_) => {}
    }
}

/// Compact item labels for steppers.
fn short_item(addr: usize, s: String) -> String {
    if addr == layout_addr::BEND_RANGE {
        return s.replace("+/- ", "±").replace(" semi-tones", " st").replace(" semi-tone", " st");
    }
    s.replace(" Bar", "")
}

mod layout_addr {
    pub const BEND_RANGE: usize = jovian_core::params::addr::BendVCOAmount;
}

fn draw_wheel(c: &mut Canvas, r: R, v: f32, center: bool, hover: bool, label: &str) {
    // slot
    c.rrect(r.inset(-2.0), 6.0, Rgba::black(0.6));
    c.rrect(r, 5.0, vgrad(r.y, r.bottom(), Rgba::hex(0x0A0A0B), Rgba::hex(0x151517)));
    // wheel surface with ribs, shifted with the value
    let inner = r.inset(2.5);
    c.set_clip(inner);
    c.rrect(inner, 4.0, Fill::Linear(inner.x, 0.0, inner.right(), 0.0, vec![(0.0, Rgba::hex(0x1C1C1F)), (0.5, Rgba::hex(0x3A3A40)), (1.0, Rgba::hex(0x1C1C1F))]));
    let travel = inner.h - 14.0;
    let pos = inner.bottom() - 7.0 - v * travel;
    let off = (v * 60.0) % 6.0;
    let mut y = inner.y - 6.0 + off;
    while y < inner.bottom() + 6.0 {
        let d = ((y - inner.cy()) / (inner.h * 0.5)).abs().min(1.0);
        c.hline(inner.x + 1.0, inner.right() - 1.0, y, 1.0, Rgba::black(0.35 + 0.3 * d));
        y += 6.0;
    }
    // shading: top and bottom fall off into darkness (cylinder)
    c.rect(inner, vgrad(inner.y, inner.bottom(), Rgba::black(0.55), Rgba::black(0.0)));
    c.rect(R::new(inner.x, inner.cy(), inner.w, inner.h * 0.5), vgrad(inner.cy(), inner.bottom(), Rgba::black(0.0), Rgba::black(0.6)));
    // marker
    let mk = R::new(inner.x, pos - 2.0, inner.w, 4.0);
    c.rect(mk, if hover { t::ORANGE.lighten(0.15) } else { t::ORANGE });
    c.clear_clip();
    if center {
        c.hline(r.x - 5.0, r.x - 1.0, r.cy(), 1.0, Rgba::white(0.35));
    }
    c.text(label, r.cx(), r.bottom() + 13.0, &Style::new(Face::CondSemi, 8.5, 0.6), t::INK_DIM, 0.5);
}

/// Key rectangles as drawn: (note, rect, is_black).
fn drawn_keys(r: R) -> Vec<(u8, R, bool)> {
    keyboard_keys(keybed(r))
        .into_iter()
        .map(|(n, kr, black)| if black { (n, R::new(kr.x, kr.y + 4.0, kr.w, kr.h), true) } else { (n, R::new(kr.x + 0.5, kr.y + 4.0, kr.w - 1.0, kr.h - 10.0), false) })
        .collect()
}

/// The keybed inside the keyboard control.
pub fn keybed(r: R) -> R {
    r.inset_xy(10.0, 0.0).offset(0.0, 4.0)
}

fn white_key(c: &mut Canvas, n: u8, kr: R, down: bool) {
    let (a, b) = if down { (Rgba::hex(0xD9D6CD), Rgba::hex(0xC9C5BA)) } else { (Rgba::hex(0xF4F2EC), Rgba::hex(0xDCD9D0)) };
    c.rrect(kr, 3.0, vgrad(kr.y, kr.bottom(), a, b));
    c.rrect_stroke(kr, 3.0, 0.75, Rgba::black(0.45));
    if down {
        c.rrect(R::new(kr.x + 2.0, kr.bottom() - 10.0, kr.w - 4.0, 6.0), 2.0, t::ORANGE.with_alpha(0.85));
    }
    if n % 12 == 0 {
        let st = Style::new(Face::CondSemi, 8.0, 0.4);
        c.text(&format!("C{}", n as i32 / 12 - 1), kr.cx(), kr.bottom() - 6.0, &st, Rgba::black(0.45), 0.5);
    }
}

fn black_key(c: &mut Canvas, kr: R, down: bool) {
    c.shadow(kr, 2.5, 3.0, 2.0, Rgba::black(0.6));
    let (a, b) = if down { (Rgba::hex(0x2A2A2E), Rgba::hex(0x1A1A1D)) } else { (Rgba::hex(0x35353A), Rgba::hex(0x0E0E10)) };
    c.rrect(kr, 2.5, vgrad(kr.y, kr.bottom(), a, b));
    c.rrect(R::new(kr.x + 2.0, kr.bottom() - 7.0, kr.w - 4.0, 4.0), 1.5, if down { t::ORANGE } else { Rgba::white(0.08) });
}

fn keyboard_static(c: &mut Canvas, r: R) {
    c.rrect(r, 6.0, vgrad(r.y, r.bottom(), Rgba::hex(0x1F1F22), Rgba::hex(0x151517)));
    c.rrect_stroke(r, 6.0, 1.0, Rgba::black(0.7));
    let felt = R::new(r.x + 10.0, r.y + 8.0, r.w - 20.0, 3.0);
    c.rect(felt, Rgba::hex(0x7A1E17));
    let keys = drawn_keys(r);
    for (n, kr, _) in keys.iter().filter(|k| !k.2) {
        white_key(c, *n, *kr, false);
    }
    for (_, kr, _) in keys.iter().filter(|k| k.2) {
        black_key(c, *kr, false);
    }
    c.rect(R::new(r.x + 10.0, r.y + 11.0, r.w - 20.0, 6.0), vgrad(r.y + 11.0, r.y + 17.0, Rgba::black(0.45), Rgba::black(0.0)));
}

fn draw_keyboard(ui: &Ui, c: &mut Canvas, r: R) {
    // the keyboard is part of the background; only a held key is drawn on top
    let Some(held) = ui.held_note else { return };
    let keys = drawn_keys(r);
    let Some((_, kr, black)) = keys.iter().find(|k| k.0 == held).copied() else { return };
    if black {
        black_key(c, kr, true);
    } else {
        white_key(c, held, kr, true);
        for (_, br, _) in keys.iter().filter(|k| k.2 && k.1.intersects(&kr)) {
            black_key(c, *br, false);
        }
        c.rect(R::new(kr.x, r.y + 11.0, kr.w, 6.0), vgrad(r.y + 11.0, r.y + 17.0, Rgba::black(0.45), Rgba::black(0.0)));
    }
}
