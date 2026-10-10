//! Instrument views and page-specific direct manipulation.

pub mod fx;
pub mod instrument;
pub mod library;
pub mod modpage;

use crate::canvas::{Canvas, R};
use crate::host::Host;
use crate::layout::Layout;
use crate::ui::{Event, Mods, Response, Ui};

/// Page specific interaction state.
#[derive(Default)]
pub struct State {
    /// Effect slot and modulation source selected in the focused editors.
    pub fx_selected: usize,
    pub mod_source: usize,
    /// The parameters captured when a main-instrument graph gesture began.
    pub instrument_drag: Option<instrument::Drag>,
    /// Keyboard curve being edited: (curve, point).
    pub curve_drag: Option<(usize, usize)>,
    /// EQ node being dragged: (slot, band).
    pub eq_drag: Option<(usize, usize)>,
}

pub fn layout(l: &mut Layout) {
    instrument::layout(l);
    modpage::layout(l);
    fx::layout(l);
    library::layout(l);
}

/// Gives a page the first look at an event (text entry, list scrolling). None = not handled.
pub fn event(ui: &mut Ui, host: &dyn Host, ev: &Event) -> Option<Response> {
    library::event(ui, host, ev)
}

pub fn area_down(ui: &mut Ui, host: &dyn Host, id: u32, r: R, x: f32, y: f32, double: bool) {
    match id {
        100..=199 => library::area_down(ui, host, id, r, x, y, double),
        200..=299 => modpage::area_down(ui, host, id, r, x, y, double),
        300..=399 => fx::area_down(ui, host, id, r, x, y, double),
        400..=499 => instrument::area_down(ui, host, id, r, x, y, double),
        _ => {}
    }
}

pub fn area_drag(ui: &mut Ui, host: &dyn Host, id: u32, x: f32, y: f32, mods: Mods) {
    match id {
        100..=199 => library::area_drag(ui, host, id, x, y, mods),
        200..=299 => modpage::area_drag(ui, host, id, x, y, mods),
        300..=399 => fx::area_drag(ui, host, id, x, y, mods),
        400..=499 => instrument::area_drag(ui, host, id, x, y, mods),
        _ => {}
    }
}

pub fn area_up(ui: &mut Ui, host: &dyn Host, _id: u32) {
    modpage::area_up(ui, host);
    fx::area_up(ui, host);
    instrument::area_up(ui, host);
}

pub fn area_wheel(ui: &mut Ui, host: &dyn Host, id: u32, dy: f32) {
    match id {
        100..=199 => library::area_wheel(ui, host, id, dy),
        300..=399 => fx::area_wheel(ui, host, id, dy),
        400..=499 => instrument::area_wheel(ui, host, id, dy),
        _ => {}
    }
}

pub fn draw_background(ui: &Ui, c: &mut Canvas) {
    modpage::draw_background(ui, c);
    fx::draw_background(ui, c);
    library::draw_background(ui, c);
}

pub fn draw(ui: &Ui, host: &dyn Host, c: &mut Canvas) {
    instrument::draw(ui, host, c);
    modpage::draw(ui, host, c);
    fx::draw(ui, host, c);
    library::draw(ui, host, c);
}
