//! The preset library page.

use crate::canvas::{Canvas, R};
use crate::host::Host;
use crate::layout::Layout;
use crate::ui::{Event, Mods, Response, Ui};
use jovian_core::preset::library::{self, LibraryEntry};
use std::sync::mpsc::{channel, Receiver};

#[derive(Default)]
pub struct Library {
    entries: Vec<LibraryEntry>,
    scanning: Option<Receiver<Vec<LibraryEntry>>>,
    scanned: bool,
    pub current: Option<String>,
    pub reveal_current: bool,
    pub search: String,
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

    pub fn entries(&self) -> &[LibraryEntry] {
        &self.entries
    }

    pub fn filtered(&self) -> Vec<LibraryEntry> {
        let q = self.search.to_lowercase();
        self.entries
            .iter()
            .filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q) || e.bank.to_lowercase().contains(&q) || e.category.to_lowercase().contains(&q))
            .cloned()
            .collect()
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
            ui.set_status(if v.len() > 1 { format!("Loaded {}; {} presets added to the library", v[0].name, v.len()) } else { format!("Loaded {}", v[0].name) });
        }
        Ok(_) => ui.set_status("No Jup-8 V preset in that file"),
        Err(e) => ui.set_status(format!("Import failed: {e}")),
    }
}

pub fn layout(_l: &mut Layout) {}

pub fn event(_ui: &mut Ui, _host: &dyn Host, _ev: &Event) -> Option<Response> {
    None
}

pub fn area_down(_ui: &mut Ui, _host: &dyn Host, _id: u32, _r: R, _x: f32, _y: f32, _double: bool) {}
pub fn area_drag(_ui: &mut Ui, _host: &dyn Host, _id: u32, _x: f32, _y: f32, _mods: Mods) {}
pub fn area_wheel(_ui: &mut Ui, _host: &dyn Host, _id: u32, _dy: f32) {}
pub fn draw_background(_ui: &Ui, _c: &mut Canvas) {}
pub fn draw(_ui: &Ui, _host: &dyn Host, _c: &mut Canvas) {}
