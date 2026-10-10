//! Palette and type scale.

use crate::canvas::Rgba;
use crate::text::{Face, Style};

// Surfaces
pub const WINDOW: Rgba = Rgba::hex(0x0C0C0E);
pub const PANEL_TOP: Rgba = Rgba::hex(0x232327);
pub const PANEL_BOTTOM: Rgba = Rgba::hex(0x1A1A1D);
pub const HEADER_TOP: Rgba = Rgba::hex(0x18181B);
pub const HEADER_BOTTOM: Rgba = Rgba::hex(0x121214);
pub const WELL: Rgba = Rgba::hex(0x101012);
pub const LINE: Rgba = Rgba::rgba(1.0, 1.0, 1.0, 0.13);
pub const LINE_STRONG: Rgba = Rgba::rgba(1.0, 1.0, 1.0, 0.32);
pub const HIGHLIGHT: Rgba = Rgba::rgba(1.0, 1.0, 1.0, 0.06);

// Ink
pub const INK: Rgba = Rgba::hex(0xECEAE6);
pub const INK_DIM: Rgba = Rgba::hex(0x9B9AA0);
pub const INK_FAINT: Rgba = Rgba::hex(0x5E5E64);
pub const INK_DARK: Rgba = Rgba::hex(0x141414);

// The Jupiter colour code
pub const RED: Rgba = Rgba::hex(0xE2452F);
pub const ORANGE: Rgba = Rgba::hex(0xF08A2C);
pub const YELLOW: Rgba = Rgba::hex(0xF3C43A);
pub const BLUE: Rgba = Rgba::hex(0x3F86DB);
pub const WHITE_KEY: Rgba = Rgba::hex(0xE7E4DC);
pub const LED: Rgba = Rgba::hex(0xFF3B26);

// Display
pub const DISPLAY_BG: Rgba = Rgba::hex(0x09090A);
pub const AMBER: Rgba = Rgba::hex(0xFFB547);
pub const AMBER_DIM: Rgba = Rgba::hex(0x8A6430);

pub const TITLE: Style = Style::new(Face::CondBold, 11.5, 1.6);
pub const LABEL: Style = Style::new(Face::CondSemi, 10.5, 0.7);
pub const SMALL: Style = Style::new(Face::CondSemi, 9.5, 0.6);
pub const BUTTON: Style = Style::new(Face::CondBold, 9.5, 0.6);
pub const VALUE: Style = Style::new(Face::Mono, 9.5, 0.0);
pub const UI: Style = Style::new(Face::Medium, 12.0, 0.0);
pub const UI_SMALL: Style = Style::new(Face::Medium, 10.5, 0.0);
