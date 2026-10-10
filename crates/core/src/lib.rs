//! Jovian-8: a Jupiter-8 style polyphonic synthesizer whose presets are interchangeable with
//! Arturia Jup-8 V4. This crate holds everything that is not tied to a plugin format: the
//! parameter table, the DSP engine, the effects and the preset formats.

pub mod dsp;
pub mod fx;
pub mod params;
pub mod preset;

pub use dsp::{HostTime, SynthEngine};
