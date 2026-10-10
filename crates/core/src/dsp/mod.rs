//! The synthesizer engine.

pub mod arp;
pub mod engine;
pub mod modulation;
pub mod patch;
pub mod primitives;
pub mod voice;
pub mod voice_math;

pub use engine::{HostTime, Meters, SynthEngine, MAX_VOICES};
