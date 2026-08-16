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

pub use crate::mt32::translator::Mt32Mode;
use crate::mt32::translator::{Event, Mt32Translator};

use crate::panel::Feed;

/// One of the settings the panel's pairs turn, named for the cheap
/// per-part read the ALL screen scans with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartSetting {
    Level,
    Pan,
    Reverb,
    Chorus,
    KeyShift,
}

/// What the MUTE-and-ALL monitor is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Monitor {
    Off,
    /// Only this part sounds.
    Solo(usize),
    /// Everything sounds, mutes and all.
    All,
}

/// How many parts the device has, which is also how many synthesizer
/// channels there are to put them on.
pub const PARTS: usize = Synthesizer::CHANNEL_COUNT;

/// The part that plays drums whatever is routed to it: the synthesizer
/// keys its percussion bank off this channel.
pub const DRUM_PART: usize = 9;

/// Nothing sounding on this key of this part.
const SILENT: u8 = 0xFF;

/// Which of a part's settings the panel has taken over: a locked
/// setting ignores the wire until the unit is switched off or
/// initialised -- the game can no more turn the knob back than it
/// could on the desk.
const LOCK_PROGRAM: u8 = 1;
const LOCK_PAN: u8 = 2;
const LOCK_REVERB: u8 = 4;
const LOCK_CHORUS: u8 = 8;

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
    /// The panel's LEVEL: a ceiling on the part's volume, whatever the
    /// wire asks for.
    level_cap: [u8; PARTS],
    /// The volume the wire last asked for, so raising the cap restores
    /// it.
    wire_level: [u8; PARTS],
    /// Settings the panel has taken over, as LOCK_ bits.
    locks: [u8; PARTS],
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
            level_cap: [127; PARTS],
            wire_level: [100; PARTS],
            locks: [0; PARTS],
            sounded: [[SILENT; 128]; PARTS],
        }
    }
}

pub struct GmEngine {
    synth: Synthesizer,
    translator: Mt32Translator,
    parts: Parts,
    /// The programs the soundfont actually carries, sorted, for the
    /// melodic banks and the drum bank: what the panel cycles through
    /// on a sparse font.
    melodic_programs: Vec<u8>,
    drum_programs: Vec<u8>,
    /// Scratch for the split-channel render, kept so a running mixer
    /// never allocates.
    scratch_left: Vec<f32>,
    scratch_right: Vec<f32>,
    /// Display text from MT-32 or GS sysex, kept for the host.
    display: Vec<String>,
    /// Letters and pictures on their way to the front panel.
    panel_feed: Vec<Feed>,
    sample_rate: u32,
    /// The front panel's VOLUME knob: an analogue pot after the DAC, so
    /// it scales the output without touching the synth's own master
    /// volume, which sysex owns.
    output_gain: f32,
    /// The mode the host configured, which is what an Init GS returns
    /// to -- auto detection stays alive there.
    configured_mode: Mt32Mode,
    monitor: Monitor,
    /// The master volume as the wire's 0..=127, mirrored raw so the
    /// panel shows exactly what was asked for; the audible mapping puts
    /// 127 at the engine's power-on gain.
    master_volume_cc: u8,
    /// The "of ALL" values behind the panel's master rows.
    master_pan: u8,
    master_shift: i8,
    master_reverb_cc: u8,
    master_chorus_cc: u8,
    /// The sysex device ID, 1..=32, as the panel edits it. Reception
    /// stays permissive -- games address whatever ID they like and a
    /// lone unit on a private cable answers -- so this is what the
    /// panel shows, not a filter.
    device_id: u8,
    /// Whether the CM-64/32L kit was put on the drum part for MT-32
    /// traffic, so leaving that mode can put Standard back.
    cm64_selected: bool,
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
        let mut melodic_programs: Vec<u8> = font
            .get_presets()
            .iter()
            .filter(|p| p.get_bank_number() == 0)
            .map(|p| p.get_patch_number() as u8)
            .collect();
        melodic_programs.sort_unstable();
        melodic_programs.dedup();
        let mut drum_programs: Vec<u8> = font
            .get_presets()
            .iter()
            .filter(|p| p.get_bank_number() == 128)
            .map(|p| p.get_patch_number() as u8)
            .collect();
        drum_programs.sort_unstable();
        drum_programs.dedup();
        let mut engine = Self {
            synth,
            translator: Mt32Translator::new(mode),
            parts: Parts::new(),
            melodic_programs,
            drum_programs,
            scratch_left: Vec::new(),
            scratch_right: Vec::new(),
            display: Vec::new(),
            panel_feed: Vec::new(),
            sample_rate,
            output_gain: 1.0,
            configured_mode: mode,
            monitor: Monitor::Off,
            master_volume_cc: 127,
            master_pan: 64,
            master_shift: 0,
            master_reverb_cc: 64,
            master_chorus_cc: 64,
            device_id: 17,
            cm64_selected: false,
        };
        engine.set_master_volume_cc(127);
        Ok(engine)
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
                Event::MasterVolume(v) => self.set_master_volume_cc(v),
                Event::Display(text) => {
                    self.panel_feed.push(Feed::Text(text.clone()));
                    self.display.push(text);
                }
                Event::Picture(rows) => self.panel_feed.push(Feed::Picture(rows)),
                Event::Translating(active) => self.follow_translation(active),
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
                    let gated = match self.monitor {
                        Monitor::All => false,
                        Monitor::Solo(solo) => part != solo,
                        Monitor::Off => self.parts.mute[part],
                    };
                    if gated {
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
                        self.synth.process_midi_message(
                            part as i32,
                            0x80,
                            key as i32,
                            data2 as i32,
                        );
                    }
                }
                // A locked setting is the panel's now; the wire's word
                // no longer reaches it.
                0xC0 if self.parts.locks[part] & LOCK_PROGRAM != 0 => {}
                0xB0 => {
                    let locks = self.parts.locks[part];
                    let handled = match data1 {
                        7 => {
                            // Volume passes through the cap.
                            self.parts.wire_level[part] = data2;
                            let capped = data2.min(self.parts.level_cap[part]);
                            self.synth
                                .process_midi_message(part as i32, 0xB0, 7, capped as i32);
                            true
                        }
                        10 => locks & LOCK_PAN != 0,
                        91 => locks & LOCK_REVERB != 0,
                        93 => locks & LOCK_CHORUS != 0,
                        _ => false,
                    };
                    if !handled {
                        self.synth.process_midi_message(
                            part as i32,
                            0xB0,
                            data1 as i32,
                            data2 as i32,
                        );
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
        (key as i16 + self.parts.key_shift[part] as i16 + self.master_shift as i16).clamp(0, 127)
            as u8
    }

    /// Render the next `frames.len()` stereo frames.
    pub fn render(&mut self, frames: &mut [(f32, f32)]) {
        // The synthesizer wants split channels; the scratch pair grows
        // to the largest block asked for and is never given back.
        let n = frames.len();
        self.scratch_left.resize(n, 0.0);
        self.scratch_right.resize(n, 0.0);
        let (left, right) = (&mut self.scratch_left, &mut self.scratch_right);
        self.synth.render(left, right);
        // The knob, then the master pan: a balance that leaves the
        // centre untouched and fades the far side out.
        let towards_right = (self.master_pan as f32 - 1.0) / 126.0;
        let pan_l = ((1.0 - towards_right) * 2.0).min(1.0);
        let pan_r = (towards_right * 2.0).min(1.0);
        let gain = self.output_gain;
        for (i, frame) in frames.iter_mut().enumerate() {
            *frame = (left[i] * gain * pan_l, right[i] * gain * pan_r);
        }
    }

    /// Display lines received since the last call, oldest first.
    pub fn take_display(&mut self) -> Vec<String> {
        std::mem::take(&mut self.display)
    }

    /// Letters and pictures for the front panel, oldest first.
    pub fn take_panel_feed(&mut self) -> Vec<Feed> {
        std::mem::take(&mut self.panel_feed)
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
            level: self.parts.level_cap.get(part).copied().unwrap_or(127),
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
    /// back exactly as playback does -- a melodic miss to the GM set,
    /// a drum miss to the standard kit, and finally to the font's
    /// lowest preset -- so the name on the glass is the sound heard.
    fn preset_name(&self, bank: i32, patch: i32) -> String {
        let presets = self.synth.get_sound_font().get_presets();
        let find = |bank: i32, patch: i32| {
            presets
                .iter()
                .find(|p| p.get_bank_number() == bank && p.get_patch_number() == patch)
                .map(|p| p.get_name().trim().to_string())
        };
        find(bank, patch)
            .or_else(|| {
                if bank < 128 {
                    find(0, patch)
                } else {
                    find(128, 0)
                }
            })
            .or_else(|| {
                presets
                    .iter()
                    .min_by_key(|p| (p.get_bank_number() << 16) | p.get_patch_number())
                    .map(|p| p.get_name().trim().to_string())
            })
            .unwrap_or_default()
    }

    /// The next instrument the panel's arrows land on: the neighbour in
    /// the soundfont's own program list, so a sparse font skips the
    /// numbers it never loaded. `None` when the font offers nothing for
    /// the part's bank.
    pub fn neighbour_instrument(&self, part: usize, step: i32) -> Option<u8> {
        let list = if part == DRUM_PART {
            &self.drum_programs
        } else {
            &self.melodic_programs
        };
        if list.is_empty() {
            return None;
        }
        let current = self
            .synth
            .channel_bank_patch(part)
            .map(|(_, patch)| patch as u8)
            .unwrap_or(0);
        // Where the current program sits, or would sit; a step then
        // walks the list with wraparound.
        let len = list.len() as i32;
        let at = match list.binary_search(&current) {
            Ok(i) => i as i32 + step,
            // Not in the list: the nearest entry in the step's own
            // direction counts as the first step.
            Err(i) if step > 0 => i as i32,
            Err(i) => i as i32 - 1,
        };
        Some(list[at.rem_euclid(len) as usize])
    }

    /// Peak amplitude per part, 0..=1-ish, for the level meters.
    pub fn part_activity(&self) -> [f32; PARTS] {
        self.synth.channel_activity()
    }

    /// Whether `part` is muted, cheap enough for every bar of every
    /// frame.
    pub fn part_muted(&self, part: usize) -> bool {
        self.parts.mute.get(part).copied().unwrap_or(false)
    }

    /// One setting across a part, cheap: no name lookup. What the ALL
    /// screen scans sixteen of.
    pub fn part_setting(&self, part: usize, pair: PartSetting) -> i32 {
        let cc = |controller| self.synth.channel_cc(part, controller).unwrap_or(0).into();
        match pair {
            PartSetting::Level => self
                .parts
                .level_cap
                .get(part)
                .copied()
                .unwrap_or(127)
                .into(),
            PartSetting::Pan => cc(10),
            PartSetting::Reverb => cc(91),
            PartSetting::Chorus => cc(93),
            PartSetting::KeyShift => self.parts.key_shift.get(part).copied().unwrap_or(0).into(),
        }
    }

    /// What the soundfont calls itself, from its own metadata.
    pub fn bank_name(&self) -> &str {
        self.synth
            .get_sound_font()
            .get_info()
            .get_bank_name()
            .trim()
    }

    /// Switch MT-32 mode at runtime -- the panel's own switch. The
    /// translator starts over in the new mode, and Init GS returns to
    /// it from now on.
    pub fn set_mt32_mode(&mut self, mode: Mt32Mode) {
        self.translator = Mt32Translator::new(mode);
        self.configured_mode = mode;
        let translating = self.translator.is_translating();
        self.follow_translation(translating);
    }

    /// The drum kit follows the translation, as ScummVM's MT-32-to-GM
    /// driver does it: the GS CM-64/32L kit (PC 128) carries the
    /// MT-32 rhythm map, keys 24-87 and all, so it is selected whenever
    /// the font offers it and MT-32 traffic is in force -- and Standard
    /// comes back when the traffic stops, unless someone changed kits
    /// by hand in the meantime.
    fn follow_translation(&mut self, translating: bool) {
        let want = translating && self.drum_programs.contains(&127);
        if want == self.cm64_selected {
            return;
        }
        self.cm64_selected = want;
        self.translator.set_cm64_kit(want);
        if want {
            self.synth
                .process_midi_message(DRUM_PART as i32, 0xC0, 127, 0);
        } else if self
            .synth
            .channel_bank_patch(DRUM_PART)
            .is_some_and(|(_, patch)| patch == 127)
        {
            self.synth
                .process_midi_message(DRUM_PART as i32, 0xC0, 0, 0);
        }
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

    /// The panel's LEVEL: a cap on the part's volume. The wire's own
    /// volume still moves underneath it and comes back when the cap is
    /// raised.
    pub fn set_part_level(&mut self, part: usize, cap: u8) {
        let Some(slot) = self.parts.level_cap.get_mut(part) else {
            return;
        };
        *slot = cap.min(127);
        let capped = self.parts.wire_level[part].min(cap);
        self.synth
            .process_midi_message(part as i32, 0xB0, 7, capped as i32);
    }

    /// The other panel edits go in as the wire message they stand for,
    /// and lock that setting against the wire until power-off or an
    /// initialisation -- a game re-programming its channels can no
    /// longer turn them back.
    pub fn set_part_pan(&mut self, part: usize, value: u8) {
        self.lock(part, LOCK_PAN);
        self.part_cc(part, 10, value);
    }

    pub fn set_part_reverb(&mut self, part: usize, value: u8) {
        self.lock(part, LOCK_REVERB);
        self.part_cc(part, 91, value);
    }

    pub fn set_part_chorus(&mut self, part: usize, value: u8) {
        self.lock(part, LOCK_CHORUS);
        self.part_cc(part, 93, value);
    }

    fn lock(&mut self, part: usize, bit: u8) {
        if let Some(locks) = self.parts.locks.get_mut(part) {
            *locks |= bit;
        }
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
        self.lock(part, LOCK_PROGRAM);
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

    // --- the masters the panel's ALL mode turns --------------------------

    /// Master volume as the wire's 0..=127. The audible mapping puts
    /// 127 at the engine's power-on gain, so the display, the sysex and
    /// the sound all agree.
    pub fn master_volume_cc(&self) -> u8 {
        self.master_volume_cc
    }

    pub fn set_master_volume_cc(&mut self, value: u8) {
        self.master_volume_cc = value.min(127);
        self.synth
            .set_master_volume(self.master_volume_cc as f32 / 127.0 * 0.5);
    }

    /// Master pan, 1..=127 around a centre of 64. It is a balance on
    /// the mix, matching the knob's place in the chain.
    pub fn master_pan(&self) -> u8 {
        self.master_pan
    }

    pub fn set_master_pan(&mut self, value: u8) {
        self.master_pan = value.clamp(1, 127);
    }

    /// Master key shift, +/-24 semitones on every part but the drums.
    pub fn master_key_shift(&self) -> i8 {
        self.master_shift
    }

    pub fn set_master_key_shift(&mut self, semitones: i8) {
        self.master_shift = semitones.clamp(-24, 24);
    }

    /// The reverb return level, 0..=127 with the factory 64 as unity.
    pub fn master_reverb(&self) -> u8 {
        self.master_reverb_cc
    }

    pub fn set_master_reverb(&mut self, value: u8) {
        self.master_reverb_cc = value.min(127);
        self.synth
            .set_master_reverb_gain(self.master_reverb_cc as f32 / 64.0);
    }

    /// The chorus return level, likewise.
    pub fn master_chorus(&self) -> u8 {
        self.master_chorus_cc
    }

    pub fn set_master_chorus(&mut self, value: u8) {
        self.master_chorus_cc = value.min(127);
        self.synth
            .set_master_chorus_gain(self.master_chorus_cc as f32 / 64.0);
    }

    /// The sysex device ID the panel shows, 1..=32.
    pub fn device_id(&self) -> u8 {
        self.device_id
    }

    pub fn set_device_id(&mut self, id: u8) {
        self.device_id = id.clamp(1, 32);
    }

    /// The ALL-and-MUTE monitor.
    pub fn monitor(&self) -> Monitor {
        self.monitor
    }

    pub fn set_monitor(&mut self, monitor: Monitor) {
        self.monitor = monitor;
        if let Monitor::Solo(solo) = monitor {
            // Everyone else falls silent now; they come back when the
            // monitor is let go and their own notes next arrive.
            for part in 0..PARTS {
                if part != solo {
                    self.synth.note_off_all_channel(part as i32, false);
                    self.parts.sounded[part] = [SILENT; 128];
                }
            }
        }
    }
}
