//! The preset library: sources and categories, a searchable list, and the current patch card
//! with save / import.

use crate::canvas::{vgrad, Canvas, PathBuilder, Rgba, R};
use crate::host::Host;
use crate::layout::{Kind, Layout, Page, BODY};
use crate::text::{Face, Style};
use crate::theme as t;
use crate::ui::{Event, Key, Mods, Response, Ui};
use crate::widgets as w;
use jovian_core::preset::library::{self, LibraryEntry, Source};
use std::collections::BTreeSet;
use std::sync::mpsc::{channel, Receiver};

// area ids
pub const SOURCES: u32 = 100;
pub const LIST: u32 = 101;
pub const SEARCH: u32 = 102;
pub const NAME: u32 = 103;
pub const BANK: u32 = 104;
pub const SAVE: u32 = 105;
pub const IMPORT: u32 = 106;
pub const REVEAL: u32 = 107;
pub const RESCAN: u32 = 108;
pub const SCROLLBAR: u32 = 109;

const ROW_H: f32 = 26.0;
const SIDE_W: f32 = 210.0;
const CARD_W: f32 = 300.0;

fn side_rect() -> R {
    R::new(BODY.x, BODY.y, SIDE_W, BODY.h)
}
fn list_panel() -> R {
    R::new(BODY.x + SIDE_W + 8.0, BODY.y, BODY.w - SIDE_W - CARD_W - 16.0, BODY.h)
}
fn search_rect() -> R {
    let p = list_panel();
    R::new(p.x + 14.0, p.y + 12.0, p.w - 28.0 - 92.0, 28.0)
}
fn rescan_rect() -> R {
    let s = search_rect();
    R::new(s.right() + 8.0, s.y, 84.0, 28.0)
}
fn list_rect() -> R {
    let p = list_panel();
    R::new(p.x + 6.0, p.y + 76.0, p.w - 12.0, p.h - 82.0)
}
fn card_rect() -> R {
    R::new(BODY.right() - CARD_W, BODY.y, CARD_W, BODY.h)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    All,
    Source(Source),
    Favorites,
}

/// A single-line text field.
#[derive(Debug, Clone, Default)]
pub struct TextField {
    pub text: String,
    /// Caret position in chars.
    pub caret: usize,
}

impl TextField {
    fn set(&mut self, s: &str) {
        self.text = s.to_string();
        self.caret = self.text.chars().count();
    }
    fn insert(&mut self, s: &str) {
        let at = self.byte(self.caret);
        let clean: String = s.chars().filter(|c| !c.is_control()).collect();
        self.text.insert_str(at, &clean);
        self.caret += clean.chars().count();
    }
    fn byte(&self, ci: usize) -> usize {
        self.text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(self.text.len())
    }
    fn key(&mut self, k: Key) {
        let n = self.text.chars().count();
        match k {
            Key::Backspace if self.caret > 0 => {
                let (a, b) = (self.byte(self.caret - 1), self.byte(self.caret));
                self.text.replace_range(a..b, "");
                self.caret -= 1;
            }
            Key::Delete if self.caret < n => {
                let (a, b) = (self.byte(self.caret), self.byte(self.caret + 1));
                self.text.replace_range(a..b, "");
            }
            Key::Left => self.caret = self.caret.saturating_sub(1),
            Key::Right => self.caret = (self.caret + 1).min(n),
            Key::Home => self.caret = 0,
            Key::End => self.caret = n,
            _ => {}
        }
    }
}

#[derive(Default)]
pub struct Library {
    entries: Vec<LibraryEntry>,
    scanning: Option<Receiver<Vec<LibraryEntry>>>,
    scanned: bool,
    pub current: Option<String>,
    pub reveal_current: bool,
    pub search: String,
    pub filter: Option<Filter>,
    pub category: Option<String>,
    pub scroll: f32,
    pub hover_row: Option<usize>,
    pub hover_side: Option<usize>,
    pub focus: Option<u32>,
    pub search_field: TextField,
    pub name_field: TextField,
    pub bank_field: TextField,
    favorites: Option<BTreeSet<String>>,
    drag_scroll: Option<(f32, f32)>,
}

fn favorites_path() -> std::path::PathBuf {
    library::user_folder().join(".favorites")
}

impl Library {
    pub fn ensure_scanned(&mut self) {
        if self.scanned || self.scanning.is_some() {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(library::scan());
        });
        self.scanning = Some(rx);
    }

    pub fn ensure_scanned_blocking(&mut self) {
        self.ensure_scanned();
        if let Some(rx) = self.scanning.take() {
            if let Ok(e) = rx.recv() {
                self.entries = e;
            }
            self.scanned = true;
        }
    }

    pub fn rescan(&mut self) {
        self.scanned = false;
        self.scanning = None;
        self.ensure_scanned();
    }

    /// Picks up a finished background scan; true if the list changed.
    pub fn poll(&mut self) -> bool {
        if let Some(rx) = &self.scanning {
            if let Ok(e) = rx.try_recv() {
                self.entries = e;
                self.scanned = true;
                self.scanning = None;
                return true;
            }
        }
        false
    }

    /// Changes whenever anything the page shows changes.
    pub fn signature(&self) -> u64 {
        let mut h = crate::ui::Sig::new();
        h.add(self.entries.len() as u64);
        h.add(self.scanning.is_some() as u64);
        h.str(self.current.as_deref().unwrap_or(""));
        h.str(&self.search);
        h.add(self.scroll.to_bits() as u64);
        h.add(self.hover_row.map_or(0, |v| v as u64 + 1));
        h.add(self.hover_side.map_or(0, |v| v as u64 + 1));
        h.add(self.focus.map_or(0, |v| v as u64 + 1));
        h.str(&self.name_field.text);
        h.add(self.name_field.caret as u64);
        h.str(&self.bank_field.text);
        h.add(self.bank_field.caret as u64);
        h.add(self.search_field.caret as u64);
        h.str(&format!("{:?}{:?}", self.filter, self.category));
        h.add(self.favorites.as_ref().map_or(0, |f| f.len() as u64));
        h.add(self.drag_scroll.is_some() as u64);
        h.0
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning.is_some()
    }

    pub fn entries(&self) -> &[LibraryEntry] {
        &self.entries
    }

    fn favorites(&mut self) -> &mut BTreeSet<String> {
        self.favorites.get_or_insert_with(|| {
            std::fs::read_to_string(favorites_path()).map(|s| s.lines().map(|l| l.to_string()).collect()).unwrap_or_default()
        })
    }

    pub fn is_favorite(&self, e: &LibraryEntry) -> bool {
        self.favorites.as_ref().is_some_and(|f| f.contains(&fav_key(e)))
    }

    fn toggle_favorite(&mut self, e: &LibraryEntry) {
        let k = fav_key(e);
        let f = self.favorites();
        if !f.remove(&k) {
            f.insert(k);
        }
        let text: Vec<String> = f.iter().cloned().collect();
        let _ = std::fs::create_dir_all(library::user_folder());
        let _ = std::fs::write(favorites_path(), text.join("\n"));
    }

    fn matches_filter(&self, e: &LibraryEntry) -> bool {
        match &self.filter {
            None | Some(Filter::All) => true,
            Some(Filter::Source(s)) => e.source == *s,
            Some(Filter::Favorites) => self.is_favorite(e),
        }
    }

    /// Entries passing the source / favourite filter (before category and search).
    fn in_source(&self) -> impl Iterator<Item = &LibraryEntry> {
        self.entries.iter().filter(|e| self.matches_filter(e))
    }

    pub fn filtered(&self) -> Vec<LibraryEntry> {
        let q = self.search.to_lowercase();
        self.in_source()
            .filter(|e| self.category.as_ref().is_none_or(|c| &e.category == c))
            .filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q) || e.bank.to_lowercase().contains(&q) || e.category.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    /// Categories in the current source with their counts, most common first.
    fn categories(&self) -> Vec<(String, usize)> {
        let mut m: std::collections::BTreeMap<String, usize> = Default::default();
        for e in self.in_source() {
            if !e.category.is_empty() {
                *m.entry(e.category.clone()).or_default() += 1;
            }
        }
        let mut v: Vec<(String, usize)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    fn count(&self, f: &Filter) -> usize {
        self.entries
            .iter()
            .filter(|e| match f {
                Filter::All => true,
                Filter::Source(s) => e.source == *s,
                Filter::Favorites => self.is_favorite(e),
            })
            .count()
    }
}

fn fav_key(e: &LibraryEntry) -> String {
    if e.source == Source::BuiltIn {
        format!("builtin/{}", e.name)
    } else {
        e.key()
    }
}

/// Rows of the side panel: (label, count, filter or category, indent).
enum SideRow {
    Header(&'static str),
    Filter(&'static str, Filter),
    Category(String, usize),
}

fn side_rows(lib: &Library) -> Vec<SideRow> {
    let mut v = vec![
        SideRow::Header("SOURCES"),
        SideRow::Filter("All presets", Filter::All),
        SideRow::Filter("Favorites", Filter::Favorites),
        SideRow::Filter("Jovian-8", Filter::Source(Source::BuiltIn)),
        SideRow::Filter("Jup-8 V Factory", Filter::Source(Source::ArturiaFactory)),
        SideRow::Filter("Jup-8 V User", Filter::Source(Source::ArturiaUser)),
        SideRow::Filter("My Presets", Filter::Source(Source::User)),
    ];
    let _ = lib;
    v
}

/// The type filter as a cloud of chips under the sources.
fn type_chips(lib: &Library, fonts: &crate::text::Fonts) -> Vec<(String, usize, R)> {
    let s = side_rect();
    let st = chip_style();
    let mut x = s.x + 14.0;
    let mut y = side_row_rect(7).y + 30.0;
    let mut out = Vec::new();
    for (c, n) in lib.categories() {
        let w = fonts.measure(&c, &st) + 18.0;
        if x + w > s.right() - 12.0 {
            x = s.x + 14.0;
            y += 27.0;
        }
        if y + 22.0 > s.bottom() - 8.0 {
            break;
        }
        out.push((c, n, R::new(x, y, w, 22.0)));
        x += w + 6.0;
    }
    out
}

fn chip_style() -> Style {
    Style::new(Face::Medium, 11.0, 0.0)
}

fn side_row_rect(i: usize) -> R {
    let s = side_rect();
    R::new(s.x + 8.0, s.y + 12.0 + i as f32 * 24.0, s.w - 16.0, 24.0)
}

fn name_rect() -> R {
    let c = card_rect();
    R::new(c.x + 16.0, c.y + 64.0, c.w - 32.0, 30.0)
}
fn bank_rect() -> R {
    let c = card_rect();
    R::new(c.x + 16.0, c.y + 122.0, c.w - 32.0, 26.0)
}
fn save_rect() -> R {
    let c = card_rect();
    R::new(c.x + 16.0, c.bottom() - 112.0, c.w - 32.0, 30.0)
}
fn import_rect() -> R {
    let c = card_rect();
    R::new(c.x + 16.0, c.bottom() - 74.0, (c.w - 40.0) * 0.5, 28.0)
}
fn reveal_rect() -> R {
    let c = card_rect();
    let i = import_rect();
    R::new(i.right() + 8.0, i.y, c.right() - 16.0 - i.right() - 8.0, 28.0)
}

pub fn layout(l: &mut Layout) {
    let p = Some(Page::Library);
    l.push(p, side_rect(), Kind::Area(SOURCES));
    l.push(p, list_rect(), Kind::Area(LIST));
    l.push(p, search_rect(), Kind::Area(SEARCH));
    l.push(p, rescan_rect(), Kind::Area(RESCAN));
    l.push(p, name_rect(), Kind::Area(NAME));
    l.push(p, bank_rect(), Kind::Area(BANK));
    l.push(p, save_rect(), Kind::Area(SAVE));
    l.push(p, import_rect(), Kind::Area(IMPORT));
    l.push(p, reveal_rect(), Kind::Area(REVEAL));
}

fn list_metrics(ui: &Ui) -> (Vec<LibraryEntry>, f32) {
    let list = ui.library.filtered();
    let max = (list.len() as f32 * ROW_H - list_rect().h + 8.0).max(0.0);
    (list, max)
}

fn clamp_scroll(ui: &mut Ui) {
    let (_, max) = list_metrics(ui);
    ui.library.scroll = ui.library.scroll.clamp(0.0, max);
}

fn reveal(ui: &mut Ui) {
    let (list, _) = list_metrics(ui);
    if let Some(i) = ui.library.current.as_ref().and_then(|k| list.iter().position(|e| &e.key() == k)) {
        let lr = list_rect();
        let y = i as f32 * ROW_H;
        if y < ui.library.scroll {
            ui.library.scroll = y;
        } else if y + ROW_H > ui.library.scroll + lr.h - 8.0 {
            ui.library.scroll = y + ROW_H - lr.h + 8.0;
        }
    }
    clamp_scroll(ui);
}

fn commit_fields(ui: &mut Ui, host: &dyn Host) {
    let name = ui.library.name_field.text.trim().to_string();
    let bank = ui.library.bank_field.text.trim().to_string();
    if !name.is_empty() {
        host.rename(&name, if bank.is_empty() { "User" } else { &bank });
    }
}

fn focus(ui: &mut Ui, id: Option<u32>) {
    let lib = &mut ui.library;
    match id {
        Some(NAME) => lib.name_field.set(&ui.patch_name),
        Some(BANK) => lib.bank_field.set(&ui.patch_bank),
        Some(SEARCH) => {
            let s = lib.search.clone();
            lib.search_field.set(&s);
        }
        _ => {}
    }
    ui.library.focus = id;
}

pub fn event(ui: &mut Ui, host: &dyn Host, ev: &Event) -> Option<Response> {
    if ui.page != Page::Library {
        return None;
    }
    let captured = Some(Response { captured: true });
    match ev {
        Event::Move { x, y, .. } => {
            if let Some((start_y, start_scroll)) = ui.library.drag_scroll {
                let (list, _) = list_metrics(ui);
                let lr = list_rect();
                let content = list.len() as f32 * ROW_H;
                if content > lr.h {
                    ui.library.scroll = start_scroll + (y - start_y) * content / lr.h;
                    clamp_scroll(ui);
                }
                ui.invalidate();
                return captured;
            }
            let lr = list_rect();
            let row = if lr.contains(*x, *y) && *x < lr.right() - 12.0 {
                let i = ((y - lr.y - 4.0 + ui.library.scroll) / ROW_H).floor();
                (i >= 0.0).then_some(i as usize)
            } else {
                None
            };
            let side = (0..40).find(|i| side_row_rect(*i).contains(*x, *y));
            if row != ui.library.hover_row || side != ui.library.hover_side {
                ui.library.hover_row = row;
                ui.library.hover_side = side;
                ui.invalidate();
            }
            None
        }
        Event::Up { .. } if ui.library.drag_scroll.is_some() => {
            ui.library.drag_scroll = None;
            None
        }
        Event::Down { x, y, .. } => {
            // clicking outside a focused text field commits it
            if let Some(f) = ui.library.focus {
                let r = match f {
                    NAME => name_rect(),
                    BANK => bank_rect(),
                    _ => search_rect(),
                };
                if !r.contains(*x, *y) {
                    if f == NAME || f == BANK {
                        commit_fields(ui, host);
                    }
                    ui.library.focus = None;
                    ui.invalidate();
                }
            }
            None
        }
        Event::Text(s) if ui.library.focus.is_some() => {
            match ui.library.focus {
                Some(SEARCH) => {
                    ui.library.search_field.insert(s);
                    ui.library.search = ui.library.search_field.text.clone();
                    ui.library.scroll = 0.0;
                }
                Some(NAME) => ui.library.name_field.insert(s),
                Some(BANK) => ui.library.bank_field.insert(s),
                _ => {}
            }
            ui.invalidate();
            captured
        }
        Event::Key { key, .. } if ui.library.focus.is_some() => {
            match key {
                Key::Enter | Key::Tab => {
                    if matches!(ui.library.focus, Some(NAME) | Some(BANK)) {
                        commit_fields(ui, host);
                    }
                    ui.library.focus = if *key == Key::Tab && ui.library.focus == Some(NAME) { Some(BANK) } else { None };
                    if ui.library.focus == Some(BANK) {
                        focus(ui, Some(BANK));
                    }
                }
                Key::Escape => {
                    if ui.library.focus == Some(SEARCH) {
                        ui.library.search.clear();
                        ui.library.search_field.set("");
                    }
                    ui.library.focus = None;
                }
                Key::Up | Key::Down if ui.library.focus == Some(SEARCH) => {
                    step_selection(ui, host, if *key == Key::Up { -1 } else { 1 });
                }
                k => {
                    match ui.library.focus {
                        Some(SEARCH) => {
                            ui.library.search_field.key(*k);
                            ui.library.search = ui.library.search_field.text.clone();
                        }
                        Some(NAME) => ui.library.name_field.key(*k),
                        Some(BANK) => ui.library.bank_field.key(*k),
                        _ => {}
                    }
                }
            }
            ui.invalidate();
            captured
        }
        Event::Key { key: Key::Up | Key::Down, .. } => {
            let dir = if matches!(ev, Event::Key { key: Key::Up, .. }) { -1 } else { 1 };
            step_selection(ui, host, dir);
            captured
        }
        Event::Text(s) if s.chars().all(|c| c.is_alphanumeric()) && !s.is_empty() => {
            // typing anywhere on the page starts a search
            focus(ui, Some(SEARCH));
            ui.library.search_field.set("");
            ui.library.search_field.insert(s);
            ui.library.search = ui.library.search_field.text.clone();
            ui.library.scroll = 0.0;
            ui.invalidate();
            captured
        }
        _ => None,
    }
}

fn step_selection(ui: &mut Ui, host: &dyn Host, dir: i64) {
    ui.step_patch(host, dir);
    reveal(ui);
}

pub fn area_down(ui: &mut Ui, host: &dyn Host, id: u32, _r: R, x: f32, y: f32, _double: bool) {
    match id {
        SOURCES => {
            let rows = side_rows(&ui.library);
            if let Some(i) = (0..rows.len()).find(|i| side_row_rect(*i).contains(x, y)) {
                match &rows[i] {
                    SideRow::Filter(_, f) => {
                        ui.library.filter = Some(f.clone());
                        ui.library.category = None;
                    }
                    SideRow::Category(c, _) => {
                        ui.library.category = if ui.library.category.as_ref() == Some(c) { None } else { Some(c.clone()) };
                    }
                    SideRow::Header(_) => {}
                }
                ui.library.scroll = 0.0;
            } else if let Some((c, _, _)) = type_chips(&ui.library, &ui.fonts).into_iter().find(|ch| ch.2.contains(x, y)) {
                ui.library.category = if ui.library.category.as_ref() == Some(&c) { None } else { Some(c) };
                ui.library.scroll = 0.0;
            }
        }
        LIST => {
            let lr = list_rect();
            if x > lr.right() - 12.0 {
                ui.library.drag_scroll = Some((y, ui.library.scroll));
                return;
            }
            let (list, _) = list_metrics(ui);
            let i = ((y - lr.y - 4.0 + ui.library.scroll) / ROW_H).floor();
            if i < 0.0 || i as usize >= list.len() {
                return;
            }
            let e = list[i as usize].clone();
            // the star column toggles the favourite instead of loading
            if x < lr.x + 30.0 {
                ui.library.favorites();
                ui.library.toggle_favorite(&e);
            } else {
                ui.load_entry(host, &e);
            }
        }
        SEARCH => focus(ui, Some(SEARCH)),
        NAME => focus(ui, Some(NAME)),
        BANK => focus(ui, Some(BANK)),
        RESCAN => {
            ui.library.rescan();
            ui.set_status("Rescanning presets…");
        }
        SAVE => save(ui, host),
        IMPORT => import_dialog(ui, host),
        REVEAL => {
            let dir = library::user_folder();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("open").arg(&dir).spawn();
        }
        _ => {}
    }
    ui.invalidate();
}

pub fn area_drag(ui: &mut Ui, _host: &dyn Host, id: u32, _x: f32, y: f32, _mods: Mods) {
    if id == LIST {
        if let Some((start_y, start_scroll)) = ui.library.drag_scroll {
            let (list, _) = list_metrics(ui);
            let lr = list_rect();
            let content = list.len() as f32 * ROW_H;
            if content > lr.h {
                ui.library.scroll = start_scroll + (y - start_y) * content / lr.h;
                clamp_scroll(ui);
            }
        }
    }
}

pub fn area_wheel(ui: &mut Ui, _host: &dyn Host, id: u32, dy: f32) {
    if id == LIST {
        ui.library.scroll -= dy * ROW_H * 1.5;
        clamp_scroll(ui);
        ui.invalidate();
    }
}

fn save(ui: &mut Ui, host: &dyn Host) {
    if matches!(ui.library.focus, Some(NAME) | Some(BANK)) {
        commit_fields(ui, host);
        ui.library.focus = None;
    }
    let p = host.current_patch();
    let bank = if p.pack.is_empty() || ["Factory", "Vintage Factory", "Jovian-8"].contains(&p.pack.as_str()) { "User".to_string() } else { p.pack.clone() };
    let folder = library::user_folder().join(jovian_core::preset::jup4x::sanitize(&bank));
    let mut state = p.clone();
    state.pack = bank;
    match library::export(&[state], &p.name, Some(&folder)) {
        Ok(path) => {
            ui.set_status(format!("Saved to ~/Music/Jovian-8/{}", path.strip_prefix(library::user_folder()).map(|p| p.display().to_string()).unwrap_or_default()));
            ui.library.rescan();
            ui.library.current = Some(format!("{}#0", path.display()));
        }
        Err(e) => ui.set_status(format!("Save failed: {e}")),
    }
}

fn import_dialog(ui: &mut Ui, host: &dyn Host) {
    // AppleScript keeps the plugin free of AppKit dialog code and works in every host
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg("POSIX path of (choose file with prompt \"Import a Jup-8 V preset or .jup4x bank\")")
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !path.is_empty() {
                import_path(ui, host, std::path::Path::new(&path));
            }
        }
        _ => {}
    }
}

/// Loads a dropped / imported preset file or `.jup4x` and copies it into the user folder.
pub fn import_path(ui: &mut Ui, host: &dyn Host, path: &std::path::Path) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            ui.set_status(format!("Could not read {}: {e}", path.display()));
            return;
        }
    };
    match jovian_core::preset::jup4x::read(&data) {
        Ok(v) if !v.is_empty() => {
            host.load_patch(&jovian_core::preset::PatchState::from_preset(&v[0]));
            let _ = library::import_file(path);
            ui.library.rescan();
            ui.set_status(if v.len() > 1 {
                format!("Loaded {}; {} presets added to My Presets", v[0].name, v.len())
            } else {
                format!("Loaded {}", v[0].name)
            });
        }
        Ok(_) => ui.set_status("No Jup-8 V preset in that file"),
        Err(e) => ui.set_status(format!("Import failed: {e}")),
    }
}

// MARK: - drawing

fn panel(c: &mut Canvas, r: R) {
    c.shadow(r, 6.0, 6.0, 3.0, Rgba::black(0.45));
    c.rrect(r, 6.0, vgrad(r.y, r.bottom(), Rgba::hex(0x232327), Rgba::hex(0x1C1C1F)));
    c.rrect_stroke(r, 6.0, 1.0, Rgba::black(0.65));
    c.rrect_stroke(r.inset(1.0), 5.0, 1.0, Rgba::white(0.045));
}

pub fn draw_background(ui: &Ui, c: &mut Canvas) {
    if ui.page != Page::Library {
        return;
    }
    panel(c, side_rect());
    panel(c, list_panel());
    panel(c, card_rect());
    // list header
    let lr = list_rect();
    let hy = lr.y - 14.0;
    let st = Style::new(Face::CondSemi, 9.0, 1.2);
    c.text("NAME", lr.x + 34.0, hy, &st, t::INK_DIM, 0.0);
    c.text("TYPE", lr.x + lr.w * 0.56, hy, &st, t::INK_DIM, 0.0);
    c.text("BANK", lr.x + lr.w * 0.74, hy, &st, t::INK_DIM, 0.0);
    c.hline(lr.x + 4.0, lr.right() - 4.0, lr.y - 4.0, 1.0, Rgba::white(0.08));
    // card captions
    let cr = card_rect();
    c.text("CURRENT PATCH", cr.x + 16.0, cr.y + 26.0, &t::TITLE, t::INK, 0.0);
    c.text("NAME", cr.x + 16.0, cr.y + 56.0, &t::SMALL, t::INK_DIM, 0.0);
    c.text("BANK", cr.x + 16.0, cr.y + 114.0, &t::SMALL, t::INK_DIM, 0.0);
    c.text("NOTES", cr.x + 16.0, cr.y + 172.0, &t::SMALL, t::INK_DIM, 0.0);
}

fn field(c: &mut Canvas, r: R, text: &str, placeholder: &str, focused: bool, caret: Option<usize>, big: bool) {
    c.rrect(r, 4.0, vgrad(r.y, r.bottom(), Rgba::hex(0x0B0B0C), Rgba::hex(0x121214)));
    c.rrect_stroke(r, 4.0, if focused { 1.5 } else { 0.75 }, if focused { t::ORANGE } else { Rgba::white(0.12) });
    let st = if big { Style::new(Face::Semi, 14.0, 0.1) } else { t::UI };
    let tr = r.inset_xy(10.0, 0.0);
    c.set_clip(r.inset(2.0));
    if text.is_empty() && !focused {
        c.text_in(placeholder, tr, &st, t::INK_FAINT, 0.0);
    } else {
        c.text_in(text, tr, &st, t::INK, 0.0);
    }
    if let (true, Some(ci)) = (focused, caret) {
        let pre: String = text.chars().take(ci).collect();
        let x = tr.x + c.fonts.measure(&pre, &st) + 0.5;
        c.rect(R::new(x, r.cy() - 8.0, 1.5, 16.0), t::ORANGE);
    }
    c.clear_clip();
}

fn star(c: &mut Canvas, cx: f32, cy: f32, r: f32, on: bool, hover: bool) {
    let mut pb = PathBuilder::new();
    for k in 0..10 {
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
        let rr = if k % 2 == 0 { r } else { r * 0.45 };
        let (x, y) = (cx + rr * a.cos(), cy + rr * a.sin());
        if k == 0 {
            pb.move_to(x, y)
        } else {
            pb.line_to(x, y)
        }
    }
    pb.close();
    if let Some(p) = pb.finish() {
        if on {
            c.fill_path(&p, t::AMBER);
        } else {
            c.stroke_path(&p, 1.0, if hover { t::INK_DIM } else { Rgba::white(0.16) });
        }
    }
}

pub fn draw(ui: &Ui, host: &dyn Host, c: &mut Canvas) {
    if ui.page != Page::Library {
        return;
    }
    let lib = &ui.library;
    // side
    let rows = side_rows(lib);
    for (i, row) in rows.iter().enumerate() {
        let r = side_row_rect(i);
        match row {
            SideRow::Header(s) => {
                c.text(s, r.x + 8.0, r.y + 17.0, &Style::new(Face::CondBold, 9.5, 1.4), t::INK_DIM, 0.0);
            }
            SideRow::Filter(label, f) => {
                let on = lib.filter.as_ref().unwrap_or(&Filter::All) == f && lib.category.is_none();
                side_item(c, r, label, lib.count(f), on, lib.hover_side == Some(i));
            }
            SideRow::Category(cat, n) => {
                let on = lib.category.as_ref() == Some(cat);
                side_item(c, r, cat, *n, on, lib.hover_side == Some(i));
            }
        }
    }
    let sr0 = side_row_rect(7);
    c.text("TYPES", sr0.x + 8.0, sr0.y + 17.0, &Style::new(Face::CondBold, 9.5, 1.4), t::INK_DIM, 0.0);
    for (cat, n, r) in type_chips(lib, c.fonts) {
        let on = lib.category.as_ref() == Some(&cat);
        let hover = r.contains(ui.mouse.0, ui.mouse.1);
        if on {
            c.rrect(r, 11.0, t::ORANGE);
        } else {
            c.rrect(r, 11.0, Rgba::white(if hover { 0.1 } else { 0.055 }));
        }
        let _ = n;
        c.text_in(&cat, r, &chip_style(), if on { t::INK_DARK } else { t::INK.with_alpha(0.82) }, 0.5);
    }
    // search
    let sr = search_rect();
    field(c, sr, &lib.search_field.text, "Search presets", lib.focus == Some(SEARCH), Some(lib.search_field.caret), false);
    // magnifier
    if lib.focus != Some(SEARCH) && lib.search_field.text.is_empty() {
        let (mx, my) = (sr.right() - 16.0, sr.cy() - 1.0);
        c.circle_stroke(mx, my, 4.5, 1.3, t::INK_DIM);
        c.line(mx + 3.3, my + 3.3, mx + 6.5, my + 6.5, 1.4, t::INK_DIM);
    }
    let rr = rescan_rect();
    let hover_rescan = rr.contains(ui.mouse.0, ui.mouse.1);
    w::text_button(c, rr, if lib.is_scanning() { "Scanning…" } else { "Rescan" }, hover_rescan, false);

    // list
    let lr = list_rect();
    let (list, max) = list_metrics(ui);
    c.set_clip(lr);
    let first = (lib.scroll / ROW_H).floor().max(0.0) as usize;
    let visible = (lr.h / ROW_H).ceil() as usize + 1;
    let cur = lib.current.as_deref();
    let name_st = Style::new(Face::Medium, 12.0, 0.0);
    let meta_st = Style::new(Face::Regular, 11.0, 0.0);
    for i in first..(first + visible).min(list.len()) {
        let e = &list[i];
        let y = lr.y + 4.0 + i as f32 * ROW_H - lib.scroll;
        let row = R::new(lr.x + 4.0, y, lr.w - 18.0, ROW_H - 2.0);
        let is_cur = match cur {
            Some(k) => k == e.key(),
            None => e.name == ui.patch_name && e.bank == ui.patch_bank,
        };
        let hover = lib.hover_row == Some(i);
        if is_cur {
            c.rrect(row, 4.0, t::ORANGE.with_alpha(0.16));
            c.rrect(R::new(row.x, row.y + 4.0, 3.0, row.h - 8.0), 1.5, t::ORANGE);
        } else if hover {
            c.rrect(row, 4.0, Rgba::white(0.05));
        } else if i % 2 == 1 {
            c.rrect(row, 4.0, Rgba::white(0.015));
        }
        star(c, row.x + 15.0, row.cy(), 5.5, lib.is_favorite(e), hover);
        let ink = if is_cur { t::INK } else { t::INK.with_alpha(0.9) };
        c.text_in(&e.name, R::new(row.x + 30.0, row.y, row.w * 0.5, row.h), &name_st, ink, 0.0);
        c.text_in(&e.category, R::new(lr.x + lr.w * 0.56, row.y, lr.w * 0.17, row.h), &meta_st, t::INK_DIM, 0.0);
        c.text_in(&e.bank, R::new(lr.x + lr.w * 0.74, row.y, lr.w * 0.24, row.h), &meta_st, t::INK_DIM, 0.0);
    }
    c.clear_clip();
    if list.is_empty() {
        let msg = if lib.is_scanning() { "Reading presets…" } else { "No presets match" };
        c.text_in(msg, lr, &t::UI, t::INK_FAINT, 0.5);
    }
    // scrollbar
    if max > 0.0 {
        let content = list.len() as f32 * ROW_H;
        let th = (lr.h * lr.h / content).max(24.0);
        let ty = lr.y + (lr.h - th) * (lib.scroll / max);
        c.rrect(R::new(lr.right() - 8.0, ty, 4.0, th), 2.0, Rgba::white(if lib.drag_scroll.is_some() { 0.4 } else { 0.18 }));
    }
    let count = format!("{} presets", list.len());
    c.text(&count, list_panel().right() - 14.0, list_panel().y + 61.0, &t::SMALL, t::INK_FAINT, 1.0);
    if let Some(cat) = &lib.category {
        c.text(&cat.to_uppercase(), lr.x + 6.0, list_panel().y + 61.0, &Style::new(Face::CondBold, 9.5, 1.2), t::ORANGE, 0.0);
    }

    // card
    let name = if lib.focus == Some(NAME) { lib.name_field.text.clone() } else { ui.patch_name.clone() };
    let bank = if lib.focus == Some(BANK) { lib.bank_field.text.clone() } else { ui.patch_bank.clone() };
    field(c, name_rect(), &name, "Name", lib.focus == Some(NAME), Some(lib.name_field.caret), true);
    field(c, bank_rect(), &bank, "Bank", lib.focus == Some(BANK), Some(lib.bank_field.caret), false);
    let cr = card_rect();
    let notes = R::new(cr.x + 16.0, cr.y + 180.0, cr.w - 32.0, save_rect().y - cr.y - 196.0);
    notes_block(c, host, notes);
    let m = ui.mouse;
    w::text_button(c, save_rect(), "Save to My Presets", save_rect().contains(m.0, m.1), true);
    w::text_button(c, import_rect(), "Import…", import_rect().contains(m.0, m.1), false);
    w::text_button(c, reveal_rect(), "Show Folder", reveal_rect().contains(m.0, m.1), false);
    c.text_in(
        "Saved files are .jup4x: Jup-8 V imports them as is.",
        R::new(cr.x + 16.0, cr.bottom() - 36.0, cr.w - 32.0, 20.0),
        &Style::new(Face::Regular, 10.0, 0.0),
        t::INK_FAINT,
        0.0,
    );
}

fn side_item(c: &mut Canvas, r: R, label: &str, n: usize, on: bool, hover: bool) {
    if on {
        c.rrect(r, 4.0, Rgba::white(0.08));
        c.rrect(R::new(r.x, r.y + 5.0, 2.5, r.h - 10.0), 1.0, t::ORANGE);
    } else if hover {
        c.rrect(r, 4.0, Rgba::white(0.04));
    }
    let st = Style::new(Face::Medium, 11.5, 0.0);
    c.text_in(label, r.inset_xy(12.0, 0.0), &st, if on { t::INK } else { t::INK.with_alpha(0.78) }, 0.0);
    c.text_in(&n.to_string(), r.inset_xy(10.0, 0.0), &Style::new(Face::Mono, 9.5, 0.0), t::INK_FAINT, 1.0);
}

/// Author, comment and character tags of the current patch.
fn notes_block(c: &mut Canvas, host: &dyn Host, r: R) {
    // reading the whole patch is cheap enough for a page that redraws only on change
    let p = host.current_patch();
    let mut y = r.y + 14.0;
    let st = Style::new(Face::Regular, 11.5, 0.0);
    if !p.author.is_empty() {
        c.text(&format!("by {}", p.author), r.x, y, &st, t::INK_DIM, 0.0);
        y += 20.0;
    }
    if let Some(ch) = p.meta.get("Characteristics") {
        // "Characteristics,Analog|Vintage Factory;Genres,House|80s;Styles,Hard|Funky;"
        let tags: Vec<&str> = ch.split(';').filter_map(|g| g.split_once(',')).flat_map(|(_, v)| v.split('|')).filter(|s| !s.is_empty()).collect();
        let mut x = r.x;
        let tst = Style::new(Face::CondSemi, 10.0, 0.4);
        for tag in tags.iter().take(10) {
            let wdt = c.fonts.measure(tag, &tst) + 14.0;
            if x + wdt > r.right() {
                x = r.x;
                y += 24.0;
            }
            let tr = R::new(x, y - 13.0, wdt, 19.0);
            c.rrect(tr, 9.5, Rgba::white(0.06));
            c.text_in(tag, tr, &tst, t::INK.with_alpha(0.8), 0.5);
            x += wdt + 6.0;
        }
        y += 28.0;
    }
    if !p.comment.is_empty() {
        wrap(c, &p.comment, r.x, y, r.w, r.bottom(), &st, t::INK_DIM);
    }
}

fn wrap(c: &mut Canvas, s: &str, x: f32, mut y: f32, w: f32, bottom: f32, st: &Style, col: Rgba) {
    let mut line = String::new();
    for word in s.split_whitespace() {
        let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if c.fonts.measure(&cand, st) > w && !line.is_empty() {
            if y > bottom {
                return;
            }
            c.text(&line, x, y, st, col, 0.0);
            y += 17.0;
            line = word.to_string();
        } else {
            line = cand;
        }
    }
    if !line.is_empty() && y <= bottom {
        c.text(&line, x, y, st, col, 0.0);
    }
}
