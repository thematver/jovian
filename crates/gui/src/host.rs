//! What the editor needs from the plugin. The plugin implements this on top of nih-plug; the
//! screenshot tool uses [`MockHost`].

use jovian_core::dsp::Meters;
use jovian_core::params::{self, Values, COUNT};
use jovian_core::preset::PatchState;
use std::cell::RefCell;

pub trait Host {
    /// Normalized value (Jup-8 V storage) of a parameter.
    fn get(&self, addr: usize) -> f32;
    /// Starts a gesture (drag) on a parameter.
    fn begin(&self, addr: usize);
    /// Sets a normalized value during a gesture.
    fn set(&self, addr: usize, normalized: f32);
    fn end(&self, addr: usize);
    /// Sets a value outside of a gesture (click on a switch).
    fn set_once(&self, addr: usize, normalized: f32) {
        self.begin(addr);
        self.set(addr, normalized);
        self.end(addr);
    }
    /// Loads a whole patch (all parameters and the pass-through data).
    fn load_patch(&self, patch: &PatchState);
    /// The current patch including the live values.
    fn current_patch(&self) -> PatchState;
    /// Renames the current patch.
    fn rename(&self, name: &str, bank: &str);
    /// Name and bank of the current patch, and a counter that changes whenever a whole patch is
    /// loaded (by the editor, a host preset or a project restore).
    fn patch_info(&self) -> (String, String, u64);
    fn meters(&self) -> Meters;
    /// Latest tempo supplied by the DAW, for synchronized visual previews.
    fn tempo_bpm(&self) -> f64 {
        120.0
    }
    fn compressor_reduction(&self, _slot: usize) -> f32 {
        0.0
    }
    /// Sends a MIDI message to the engine (on-screen keyboard and wheels).
    fn midi(&self, status: u8, d1: u8, d2: u8);
    /// Asks the host to resize the window to the editor's new size; false if it refused.
    fn resize(&self, _w: u32, _h: u32) -> bool {
        false
    }
}

/// An in-memory host for previews and tests.
pub struct MockHost {
    pub values: RefCell<Values>,
    pub patch: RefCell<PatchState>,
    pub meters: Meters,
    pub generation: std::cell::Cell<u64>,
}

impl MockHost {
    pub fn new(patch: PatchState) -> Self {
        MockHost {
            values: RefCell::new(patch.to_values()),
            patch: RefCell::new(patch),
            meters: Meters::default(),
            generation: std::cell::Cell::new(1),
        }
    }
}

impl Host for MockHost {
    fn get(&self, addr: usize) -> f32 {
        self.values.borrow()[addr]
    }
    fn begin(&self, _addr: usize) {}
    fn set(&self, addr: usize, normalized: f32) {
        if addr < COUNT {
            self.values.borrow_mut()[addr] = normalized.clamp(0.0, 1.0);
        }
    }
    fn end(&self, _addr: usize) {}
    fn load_patch(&self, patch: &PatchState) {
        *self.values.borrow_mut() = patch.to_values();
        *self.patch.borrow_mut() = patch.clone();
        self.generation.set(self.generation.get() + 1);
    }
    fn patch_info(&self) -> (String, String, u64) {
        let p = self.patch.borrow();
        (p.name.clone(), p.pack.clone(), self.generation.get())
    }
    fn current_patch(&self) -> PatchState {
        let mut p = self.patch.borrow().clone();
        p.capture(&self.values.borrow());
        p
    }
    fn rename(&self, name: &str, bank: &str) {
        let mut p = self.patch.borrow_mut();
        p.name = name.to_string();
        p.pack = bank.to_string();
    }
    fn meters(&self) -> Meters {
        self.meters
    }
    fn midi(&self, _s: u8, _a: u8, _b: u8) {}
}

/// Value of a discrete parameter as an item index.
pub fn index_of(host: &dyn Host, addr: usize) -> usize {
    params::PARAMS[addr].index_f32(host.get(addr))
}
