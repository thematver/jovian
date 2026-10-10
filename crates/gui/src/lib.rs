//! The Jovian-8 editor: a vector-drawn panel rendered on the CPU with tiny-skia, independent of
//! the plugin wrapper (see `host::Host`) and of the window system (see `ui::Event`).

pub mod canvas;
pub mod display;
pub mod host;
pub mod layout;
pub mod pages;
pub mod text;
pub mod theme;
pub mod ui;
pub mod widgets;

pub use host::{Host, MockHost};
pub use ui::{Button, Cursor, Event, Key, Mods, Ui};
