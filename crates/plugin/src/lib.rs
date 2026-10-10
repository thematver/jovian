//! Jovian-8 as a CLAP / VST3 plugin (and AUv2 through clap-wrapper, see `au/`).

pub mod editor;
pub mod params;

use jovian_core::dsp::{HostTime, Meters, SynthEngine};
use nih_plug::prelude::*;
use params::JovianParams;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// MIDI-like events the editor sends to the audio thread (on-screen keyboard, wheels).
#[derive(Debug, Clone, Copy)]
pub struct GuiMidi(pub u8, pub u8, pub u8);

/// State shared between the audio thread and the editor.
pub struct Shared {
    pub active_voices: AtomicU32,
    pub gated_voices: AtomicU32,
    pub lfo: AtomicF32,
    pub env1: AtomicF32,
    pub env2: AtomicF32,
    pub comp_reduction: [AtomicF32; 3],
    pub sample_rate: AtomicF32,
    /// Editor -> audio thread note and controller messages.
    pub gui_midi: crossbeam::queue::ArrayQueue<GuiMidi>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            active_voices: AtomicU32::new(0),
            gated_voices: AtomicU32::new(0),
            lfo: AtomicF32::new(0.0),
            env1: AtomicF32::new(0.0),
            env2: AtomicF32::new(0.0),
            comp_reduction: [AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0)],
            sample_rate: AtomicF32::new(48_000.0),
            gui_midi: crossbeam::queue::ArrayQueue::new(256),
        }
    }
}

impl Shared {
    pub fn meters(&self) -> Meters {
        Meters {
            active_voices: self.active_voices.load(Ordering::Relaxed) as u16,
            gated_voices: self.gated_voices.load(Ordering::Relaxed) as u16,
            lfo: self.lfo.load(Ordering::Relaxed),
            env1: self.env1.load(Ordering::Relaxed),
            env2: self.env2.load(Ordering::Relaxed),
        }
    }
}

pub struct Jovian8 {
    params: Arc<JovianParams>,
    shared: Arc<Shared>,
    editor_open: Arc<std::sync::atomic::AtomicBool>,
    engine: Box<SynthEngine>,
    prepared: Option<(f32, u32)>,
    mono_l: Vec<f32>,
    mono_r: Vec<f32>,
}

impl Default for Jovian8 {
    fn default() -> Self {
        Jovian8 {
            params: Arc::new(JovianParams::default()),
            shared: Arc::new(Shared::default()),
            editor_open: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            engine: Box::new(SynthEngine::new()),
            prepared: None,
            mono_l: vec![0.0; 4096],
            mono_r: vec![0.0; 4096],
        }
    }
}

impl Jovian8 {
    fn handle_event(&mut self, ev: NoteEvent<()>) {
        let e = &mut self.engine;
        match ev {
            NoteEvent::NoteOn { note, velocity, .. } => {
                if velocity <= 0.0 {
                    e.note_off(note as i32)
                } else {
                    e.note_on(note as i32, (velocity * 127.0).round().clamp(1.0, 127.0) / 127.0)
                }
            }
            NoteEvent::NoteOff { note, .. } => e.note_off(note as i32),
            NoteEvent::MidiCC { cc, value, .. } => e.handle_midi(0xB0, cc, (value * 127.0).round() as u8),
            NoteEvent::MidiChannelPressure { pressure, .. } => e.handle_midi(0xD0, (pressure * 127.0).round() as u8, 0),
            NoteEvent::PolyPressure { note, pressure, .. } => e.handle_midi(0xA0, note, (pressure * 127.0).round() as u8),
            NoteEvent::MidiPitchBend { value, .. } => {
                let v = (value * 16383.0).round().clamp(0.0, 16383.0) as u16;
                e.handle_midi(0xE0, (v & 0x7F) as u8, (v >> 7) as u8)
            }
            _ => {}
        }
    }

    fn publish_meters(&self) {
        let m = self.engine.meters;
        let s = &self.shared;
        s.active_voices.store(m.active_voices as u32, Ordering::Relaxed);
        s.gated_voices.store(m.gated_voices as u32, Ordering::Relaxed);
        s.lfo.store(m.lfo, Ordering::Relaxed);
        s.env1.store(m.env1, Ordering::Relaxed);
        s.env2.store(m.env2, Ordering::Relaxed);
        for i in 0..3 {
            s.comp_reduction[i].store(self.engine.compressor_reduction_db(i), Ordering::Relaxed);
        }
    }
}

impl Plugin for Jovian8 {
    const NAME: &'static str = "Jovian-8";
    const VENDOR: &'static str = "Jovian";
    const URL: &'static str = "https://anomatver.ru";
    const EMAIL: &'static str = "lepomor@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout { main_input_channels: None, main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() },
        AudioIOLayout { main_input_channels: None, main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default() },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(Box::new(editor::JovianEditor { params: self.params.clone(), shared: self.shared.clone(), open: self.editor_open.clone() }))
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, config: &BufferConfig, _context: &mut impl InitContext<Self>) -> bool {
        let key = (config.sample_rate, config.max_buffer_size);
        if self.prepared != Some(key) {
            self.engine.prepare(config.sample_rate as f64, config.max_buffer_size as usize);
            let n = (config.max_buffer_size as usize).max(1);
            self.mono_l = vec![0.0; n];
            self.mono_r = vec![0.0; n];
            self.prepared = Some(key);
        }
        self.shared.sample_rate.store(config.sample_rate, Ordering::Relaxed);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        for (i, h) in self.params.list.iter().enumerate() {
            self.engine.params[i] = h.normalized();
        }
        while let Some(GuiMidi(a, b, c)) = self.shared.gui_midi.pop() {
            self.engine.handle_midi(a, b, c);
        }
        let t = context.transport();
        let time = HostTime {
            tempo: t.tempo.filter(|x| *x > 0.0).unwrap_or(120.0),
            beat_position: t.pos_beats().unwrap_or(0.0),
            is_playing: t.playing,
        };
        let sr = self.engine.sample_rate();
        let n = buffer.samples();
        let channels = buffer.as_slice();
        let mut pos = 0;
        let mut next = context.next_event();
        while pos < n {
            while let Some(ev) = next {
                if ev.timing() as usize > pos {
                    break;
                }
                self.handle_event(ev);
                next = context.next_event();
            }
            let end = match next {
                Some(ev) => (ev.timing() as usize).clamp(pos + 1, n),
                None => n,
            };
            let mut seg = time;
            seg.beat_position += pos as f64 / sr * time.tempo / 60.0;
            if channels.len() >= 2 {
                let (l, r) = channels.split_at_mut(1);
                self.engine.render(&mut l[0][pos..end], &mut r[0][pos..end], seg);
            } else if let Some(out) = channels.first_mut() {
                let m = end - pos;
                if self.mono_l.len() < m {
                    // never happens: the host promised max_buffer_size
                    break;
                }
                let (ml, mr) = (&mut self.mono_l[..m], &mut self.mono_r[..m]);
                self.engine.render(ml, mr, seg);
                for i in 0..m {
                    out[pos + i] = 0.5 * (self.mono_l[i] + self.mono_r[i]);
                }
            }
            pos = end;
        }
        while let Some(ev) = next {
            self.handle_event(ev);
            next = context.next_event();
        }
        self.publish_meters();
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for Jovian8 {
    const CLAP_ID: &'static str = "com.anomatver.jovian8";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Polyphonic synthesizer in the spirit of the Jupiter-8, Jup-8 V preset compatible");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Instrument, ClapFeature::Synthesizer, ClapFeature::Stereo];
}

impl Vst3Plugin for Jovian8 {
    const VST3_CLASS_ID: [u8; 16] = *b"JovianEightSynth";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(Jovian8);
nih_export_vst3!(Jovian8);
