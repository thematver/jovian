//! Jup-8 V preset interoperability: the preset file format, `.jup4x` archives, the plugin's
//! patch state, built-in patches and the preset library on disk.

pub mod arturia;
pub mod boost;
pub mod factory;
pub mod jup4x;
pub mod library;
pub mod state;

pub use arturia::ArturiaPreset;
pub use library::{LibraryEntry, Source};
pub use state::PatchState;
