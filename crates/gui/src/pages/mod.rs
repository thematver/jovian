//! Pages other than the main panel: modulation, effects and the preset library.

pub mod library;

use crate::canvas::{Canvas, R};
use crate::host::Host;
use crate::layout::Layout;
use crate::ui::{Event, Mods, Response, Ui};

/// Page specific interaction state.
#[derive(Default)]
pub struct State {}

pub fn layout(l: &mut Layout) {
    library::layout(l);
}

/// Gives a page the first look at an event (text entry, list scrolling). None = not handled.
pub fn event(ui: &mut Ui, host: &dyn Host, ev: &Event) -> Option<Response> {
    library::event(ui, host, ev)
}

pub fn area_down(ui: &mut Ui, host: &dyn Host, id: u32, r: R, x: f32, y: f32, double: bool) {
    library::area_down(ui, host, id, r, x, y, double);
}

pub fn area_drag(ui: &mut Ui, host: &dyn Host, id: u32, x: f32, y: f32, mods: Mods) {
    library::area_drag(ui, host, id, x, y, mods);
}

pub fn area_wheel(ui: &mut Ui, host: &dyn Host, id: u32, dy: f32) {
    library::area_wheel(ui, host, id, dy);
}

pub fn draw_background(ui: &Ui, c: &mut Canvas) {
    library::draw_background(ui, c);
}

pub fn draw(ui: &Ui, host: &dyn Host, c: &mut Canvas) {
    library::draw(ui, host, c);
}
