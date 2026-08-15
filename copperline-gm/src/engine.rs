//! The engine an emulator embeds: a byte in from the serial line, stereo
//! frames out, and nothing else to hold. The MT-32 translator sits in
//! front of the synthesizer permanently -- in `Off` mode it is a plain
//! GM byte parser -- so the host wires exactly one thing whichever mode
//! is chosen.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::mt32::translator::{Event, Mt32Mode, Mt32Translator};

pub struct GmEngine {
    synth: Synthesizer,
    translator: Mt32Translator,
    /// Display text from MT-32 sysex, kept for the host's OSD.
    display: Vec<String>,
    sample_rate: u32,
}

impl GmEngine {
    /// Open a soundfont and build the engine at `sample_rate`.
    pub fn open(soundfont: &Path, sample_rate: u32, mode: Mt32Mode) -> Result<Self, String> {
        let mut file = File::open(soundfont)
            .map_err(|e| format!("opening {}: {e}", soundfont.display()))?;
        let font = SoundFont::new(&mut file)
            .map_err(|e| format!("reading {}: {e}", soundfont.display()))?;
        Self::from_font(Arc::new(font), sample_rate, mode)
    }

    pub fn from_font(
        font: Arc<SoundFont>,
        sample_rate: u32,
        mode: Mt32Mode,
    ) -> Result<Self, String> {
        let settings = SynthesizerSettings::new(sample_rate as i32);
        let synth = Synthesizer::new(&font, &settings).map_err(|e| e.to_string())?;
        Ok(Self {
            synth,
            translator: Mt32Translator::new(mode),
            display: Vec::new(),
            sample_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Whether MT-32 translation is currently applied (in auto mode this
    /// flips once the traffic identifies itself).
    pub fn translating(&self) -> bool {
        self.translator.is_translating()
    }

    /// Take one byte off the serial line.
    pub fn write_byte(&mut self, byte: u8) {
        // Drain into locals first: the borrow of the translator ends
        // before the synthesizer is touched.
        let events: Vec<Event> = self.translator.push(byte).collect();
        for event in events {
            match event {
                Event::Midi {
                    command,
                    channel,
                    data1,
                    data2,
                } => self.synth.process_midi_message(
                    channel as i32,
                    command as i32,
                    data1 as i32,
                    data2 as i32,
                ),
                Event::MasterVolume(v) => {
                    // 0..=127 mapped so the GM default (127) is unity.
                    self.synth.set_master_volume(v as f32 / 127.0);
                }
                Event::Display(text) => self.display.push(text),
                Event::Translating(_) => {}
            }
        }
    }

    /// Render the next `frames.len()` stereo frames.
    pub fn render(&mut self, frames: &mut [(f32, f32)]) {
        // The synthesizer wants split channels; a small scratch pair per
        // call keeps the public surface simple and the block sizes are
        // the host's business.
        let n = frames.len();
        let mut left = vec![0f32; n];
        let mut right = vec![0f32; n];
        self.synth.render(&mut left, &mut right);
        for (i, frame) in frames.iter_mut().enumerate() {
            *frame = (left[i], right[i]);
        }
    }

    /// Display lines received since the last call, oldest first.
    pub fn take_display(&mut self) -> Vec<String> {
        std::mem::take(&mut self.display)
    }

    /// Silence everything without dropping the soundfont.
    pub fn reset(&mut self) {
        self.synth.reset();
    }
}
