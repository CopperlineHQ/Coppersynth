//! The engine an emulator embeds: a byte in from the serial line, stereo
//! frames out, and nothing else to hold. The MT-32 translator sits in
//! front of the synthesizer permanently -- in `Off` mode it is a plain
//! GM byte parser -- so the host wires exactly one thing whichever mode
//! is chosen.
//!
//! Between the translator and the synthesizer sits the part layer: the
//! sixteen parts a Sound Canvas class device has, each listening on a
//! MIDI channel (its own, until told otherwise), each with a mute switch
//! and a key shift. At its defaults the layer is a wire.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::mt32::translator::{Event, Mt32Mode, Mt32Translator};

/// How many parts the device has, which is also how many synthesizer
/// channels there are to put them on.
pub const PARTS: usize = Synthesizer::CHANNEL_COUNT;

/// The part that plays drums whatever is routed to it: the synthesizer
/// keys its percussion bank off this channel.
pub const DRUM_PART: usize = 9;

/// Nothing sounding on this key of this part.
const SILENT: u8 = 0xFF;

/// Everything the front panel shows about one part, read live from the
/// synthesizer rather than shadowed -- what the screen says is what is
/// actually set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartView {
    /// The program in force, 0..=127.
    pub instrument: u8,
    /// What the instrument is called: the soundfont preset's name -- or,
    /// translating, the name the game gave the timbre it uploaded.
    pub name: String,
    /// Raw controller values, as the wire carries them.
    pub level: u8,
    pub pan: u8,
    pub reverb: u8,
    pub chorus: u8,
    /// Semitones the part layer moves incoming notes, -24..=24.
    pub key_shift: i8,
    /// The MIDI channel the part answers to, or `None` for off.
    pub rx_channel: Option<u8>,
    pub muted: bool,
    /// Whether this is the drum part, whose instrument is a kit.
    pub drums: bool,
}

/// The part layer's own state; everything else about a part lives in the
/// synthesizer and is read back from it.
struct Parts {
    /// The wire channel each part listens on, or `None` for off. The
    /// default is one-to-one, which is what General MIDI expects.
    rx_channel: [Option<u8>; PARTS],
    mute: [bool; PARTS],
    key_shift: [i8; PARTS],
    /// Where each sounding note landed after the shift, so its note-off
    /// finds it whatever the shift has become since.
    sounded: [[u8; 128]; PARTS],
}

impl Parts {
    fn new() -> Self {
        let mut rx = [None; PARTS];
        for (channel, slot) in rx.iter_mut().enumerate() {
            *slot = Some(channel as u8);
        }
        Self {
            rx_channel: rx,
            mute: [false; PARTS],
            key_shift: [0; PARTS],
            sounded: [[SILENT; 128]; PARTS],
        }
    }
}

pub struct GmEngine {
    synth: Synthesizer,
    translator: Mt32Translator,
    parts: Parts,
    /// Display text from MT-32 or GS sysex, kept for the host.
    display: Vec<String>,
    sample_rate: u32,
    /// The front panel's VOLUME knob: an analogue pot after the DAC, so
    /// it scales the output without touching the synth's own master
    /// volume, which sysex owns.
    output_gain: f32,
}

impl GmEngine {
    /// Open a soundfont and build the engine at `sample_rate`.
    pub fn open(soundfont: &Path, sample_rate: u32, mode: Mt32Mode) -> Result<Self, String> {
        let mut file =
            File::open(soundfont).map_err(|e| format!("opening {}: {e}", soundfont.display()))?;
        Self::open_reader(&mut file, sample_rate, mode)
            .map_err(|e| format!("{}: {e}", soundfont.display()))
    }

    /// Open a soundfont from any byte stream: a file, or a host's
    /// decompressor -- how the bank arrives is the host's business.
    pub fn open_reader<R: std::io::Read>(
        reader: &mut R,
        sample_rate: u32,
        mode: Mt32Mode,
    ) -> Result<Self, String> {
        let font = SoundFont::new(reader).map_err(|e| e.to_string())?;
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
            parts: Parts::new(),
            display: Vec::new(),
            sample_rate,
            output_gain: 1.0,
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
                } => self.deliver(command, channel, data1, data2),
                Event::MasterVolume(v) => {
                    // 0..=127 mapped so the GM default (127) is unity.
                    self.synth.set_master_volume(v as f32 / 127.0);
                }
                Event::Display(text) => self.display.push(text),
                Event::Translating(_) => {}
            }
        }
    }

    /// Hand a wire message to every part listening on its channel,
    /// through that part's mute and key shift.
    fn deliver(&mut self, command: u8, channel: u8, data1: u8, data2: u8) {
        for part in 0..PARTS {
            if self.parts.rx_channel[part] != Some(channel) {
                continue;
            }
            match command {
                0x90 if data2 > 0 => {
                    if self.parts.mute[part] {
                        continue;
                    }
                    let key = self.shifted_key(part, data1);
                    self.parts.sounded[part][data1 as usize] = key;
                    self.synth
                        .process_midi_message(part as i32, 0x90, key as i32, data2 as i32);
                }
                0x80 | 0x90 => {
                    let sounded = &mut self.parts.sounded[part][data1 as usize];
                    let key = std::mem::replace(sounded, SILENT);
                    if key != SILENT {
                        self.synth
                            .process_midi_message(part as i32, 0x80, key as i32, data2 as i32);
                    }
                }
                _ => self.synth.process_midi_message(
                    part as i32,
                    command as i32,
                    data1 as i32,
                    data2 as i32,
                ),
            }
        }
    }

    /// Where the part's key shift puts a note. Drums are never shifted:
    /// moving a kit's keys renames its instruments rather than
    /// transposing them.
    fn shifted_key(&self, part: usize, key: u8) -> u8 {
        if part == DRUM_PART {
            return key;
        }
        (key as i16 + self.parts.key_shift[part] as i16).clamp(0, 127) as u8
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
        let gain = self.output_gain;
        for (i, frame) in frames.iter_mut().enumerate() {
            *frame = (left[i] * gain, right[i] * gain);
        }
    }

    /// Display lines received since the last call, oldest first.
    pub fn take_display(&mut self) -> Vec<String> {
        std::mem::take(&mut self.display)
    }

    /// Silence everything without dropping the soundfont.
    pub fn reset(&mut self) {
        self.synth.reset();
        self.parts = Parts::new();
    }

    // --- what the front panel reads and turns ---------------------------

    /// Everything the panel shows about `part`, read live.
    pub fn part_view(&self, part: usize) -> PartView {
        let (bank, patch) = self.synth.channel_bank_patch(part).unwrap_or((0, 0));
        let cc = |controller| self.synth.channel_cc(part, controller).unwrap_or(0);
        let name = self
            .translator
            .channel_name(part)
            .map(str::to_string)
            .unwrap_or_else(|| self.preset_name(bank, patch));
        PartView {
            instrument: (patch & 0x7F) as u8,
            name,
            level: cc(7),
            pan: cc(10),
            reverb: cc(91),
            chorus: cc(93),
            key_shift: self.parts.key_shift[part],
            rx_channel: self.parts.rx_channel.get(part).copied().flatten(),
            muted: self.parts.mute[part],
            drums: part == DRUM_PART,
        }
    }

    /// What the soundfont calls the preset on `bank`/`patch`, falling
    /// back the way the synthesizer falls back: the drum part to bank
    /// 128, anything unknown to the bank's patch 0, and failing that to
    /// nothing at all.
    fn preset_name(&self, bank: i32, patch: i32) -> String {
        let presets = self.synth.get_sound_font().get_presets();
        let find = |bank: i32, patch: i32| {
            presets
                .iter()
                .find(|p| p.get_bank_number() == bank && p.get_patch_number() == patch)
                .map(|p| p.get_name().trim().to_string())
        };
        find(bank, patch)
            .or_else(|| find(bank, 0))
            .or_else(|| find(0, patch))
            .unwrap_or_default()
    }

    /// Peak amplitude per part, 0..=1-ish, for the level meters.
    pub fn part_activity(&self) -> [f32; PARTS] {
        self.synth.channel_activity()
    }

    /// Voices sounding, and the most that can.
    pub fn voices(&self) -> (usize, usize) {
        (
            self.synth.active_voice_count(),
            self.synth.get_maximum_polyphony(),
        )
    }

    /// The VOLUME knob: a linear gain on the rendered output, 1.0 at
    /// its top. It is deliberately not the sysex master volume -- on the
    /// unit the knob is a pot after the DAC, so the two do not fight.
    pub fn output_gain(&self) -> f32 {
        self.output_gain
    }

    pub fn set_output_gain(&mut self, gain: f32) {
        self.output_gain = gain.clamp(0.0, 1.0);
    }

    /// Panel edits: each goes in as the wire message it stands for, so
    /// an edit and a game's own writes land in exactly one place.
    pub fn set_part_level(&mut self, part: usize, value: u8) {
        self.part_cc(part, 7, value);
    }

    pub fn set_part_pan(&mut self, part: usize, value: u8) {
        self.part_cc(part, 10, value);
    }

    pub fn set_part_reverb(&mut self, part: usize, value: u8) {
        self.part_cc(part, 91, value);
    }

    pub fn set_part_chorus(&mut self, part: usize, value: u8) {
        self.part_cc(part, 93, value);
    }

    fn part_cc(&mut self, part: usize, controller: u8, value: u8) {
        self.synth.process_midi_message(
            part as i32,
            0xB0,
            controller as i32,
            (value & 0x7F) as i32,
        );
    }

    pub fn set_part_instrument(&mut self, part: usize, program: u8) {
        self.synth
            .process_midi_message(part as i32, 0xC0, (program & 0x7F) as i32, 0);
    }

    pub fn set_part_key_shift(&mut self, part: usize, semitones: i8) {
        if let Some(shift) = self.parts.key_shift.get_mut(part) {
            *shift = semitones.clamp(-24, 24);
        }
    }

    pub fn set_part_rx_channel(&mut self, part: usize, channel: Option<u8>) {
        if let Some(rx) = self.parts.rx_channel.get_mut(part) {
            *rx = channel.map(|c| c & 0x0F);
        }
    }

    /// Mute or unmute a part. Muting silences what is already sounding
    /// as well as gating what arrives, as the unit's MUTE does.
    pub fn set_part_mute(&mut self, part: usize, mute: bool) {
        let Some(slot) = self.parts.mute.get_mut(part) else {
            return;
        };
        *slot = mute;
        if mute {
            self.synth.note_off_all_channel(part as i32, false);
            self.parts.sounded[part] = [SILENT; 128];
        }
    }

    /// Everything off at once, releases respected.
    pub fn all_notes_off(&mut self) {
        self.synth.note_off_all(false);
        for sounded in &mut self.parts.sounded {
            *sounded = [SILENT; 128];
        }
    }
}
