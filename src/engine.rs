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

use crate::synth::{SoundFont, Synthesizer, SynthesizerSettings};

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

/// The demo songs baked into the library, by title. The panel's demo
/// mode plays them; nothing else knows they exist. "Railgun Rain" is
/// by Ivan Stanton (northivanastan), public domain; "Title Screen" has
/// no known artist and its licence asks nothing.
pub const DEMO_SONGS: [&str; 2] = ["Railgun Rain", "Title Screen"];
const DEMO_DATA: [&[u8]; 2] = [
    include_bytes!("../demo/railgun-rain.mid"),
    include_bytes!("../demo/title-screen.mid"),
];

/// A demo song in flight: its events flattened to seconds, and how far
/// the render clock has carried it.
struct Demo {
    events: Vec<(f64, u8, u8, u8)>,
    at: usize,
    clock: f64,
}

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
    /// Voice Reserve, as the unit stores and shows it. Our well has
    /// more voices than the hardware's, so nothing starves and the
    /// number never has to act; it is kept because the unit keeps it.
    voice_rsv: [u8; PARTS],
    /// Per-part reception switches: bank select, and NRPN.
    rx_bank: [bool; PARTS],
    rx_nrpn: [bool; PARTS],
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
            // The GS factory reserve: six for the drum part, two each
            // for the melodic fifteen.
            voice_rsv: std::array::from_fn(|p| if p == DRUM_PART { 6 } else { 2 }),
            rx_bank: [true; PARTS],
            rx_nrpn: [true; PARTS],
        }
    }
}

pub struct Engine {
    synth: Synthesizer,
    translator: Mt32Translator,
    parts: Parts,
    /// The drum kits the soundfont actually carries, sorted: the panel
    /// steps these as a list, unlike melodic programs, whose numbers
    /// must never shift.
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
    demo: Option<Demo>,
    /// The system functions, as the unit's own menu groups them: kept
    /// across a GS reset, stored regardless of the Back Up switch.
    rx_inst_chg: bool,
    rx_sysex: bool,
    rx_gs_reset: bool,
    backup: bool,
    /// The bar display style 1-8 and the peak-hold style 0 (off) to 3.
    display_type: u8,
    peak_hold: u8,
    /// Master tune in tenths of a hertz, 4153..=4662 around A=440.0.
    master_tune_tenths: u16,
    /// Whether the byte stream is inside a sysex message, for the Rx
    /// SysEx switch to skip one whole.
    in_sysex: bool,
    /// Whether MIDI IN is ignored wholesale -- demo mode closes the
    /// door, as the hardware does, even between songs.
    wire_closed: bool,
    /// The off-line watch: whether active sensing has been seen, and
    /// how many frames have rendered since the last byte arrived.
    sensing: bool,
    silent_frames: u64,
}

impl Engine {
    /// Open a soundfont and build the engine at `sample_rate`.
    pub fn open(soundfont: &Path, sample_rate: u32, mode: Mt32Mode) -> Result<Self, String> {
        let mut file =
            File::open(soundfont).map_err(|e| format!("opening {}: {e}", soundfont.display()))?;
        Self::open_reader(&mut file, sample_rate, mode)
            .map_err(|e| format!("{}: {e}", soundfont.display()))
    }

    /// Open the bank Coppersynth carries inside itself: GeneralUser GS,
    /// fetched from its repository when the library was built and
    /// zipped in with its licence. This is the synth with no
    /// configuration at all.
    pub fn open_bundled(sample_rate: u32, mode: Mt32Mode) -> Result<Self, String> {
        static BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bundled-bank.zip"));
        if BUNDLE.is_empty() {
            return Err("built without the bundled soundfont (offline build)".to_string());
        }
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(BUNDLE))
            .map_err(|e| format!("bundled bank: {e}"))?;
        let entry = (0..archive.len())
            .find(|&i| {
                archive
                    .by_index(i)
                    .is_ok_and(|f| f.name().to_ascii_lowercase().ends_with(".sf2"))
            })
            .ok_or_else(|| "bundled bank: no .sf2 inside".to_string())?;
        let mut reader = archive
            .by_index(entry)
            .map_err(|e| format!("bundled bank: {e}"))?;
        Self::open_reader(&mut reader, sample_rate, mode)
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
            demo: None,
            rx_inst_chg: true,
            rx_sysex: true,
            rx_gs_reset: true,
            backup: true,
            display_type: 1,
            peak_hold: 1,
            master_tune_tenths: 4400,
            in_sysex: false,
            wire_closed: false,
            sensing: false,
            silent_frames: 0,
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
        // A unit in its demo mode ignores MIDI IN, as the hardware
        // does -- between songs as much as during one.
        if self.wire_closed || self.demo.is_some() {
            return;
        }
        self.silent_frames = 0;
        // Active sensing arms the off-line watch and carries nothing
        // else; being real-time, it never enters the framing below.
        if byte == 0xFE {
            self.sensing = true;
            return;
        }
        // The Rx SysEx switch: off, exclusives fall on the floor whole,
        // and everything else plays on.
        if byte == 0xF0 {
            self.in_sysex = true;
        }
        if self.in_sysex {
            let ends = byte == 0xF7;
            if ends {
                self.in_sysex = false;
            }
            if !self.rx_sysex {
                return;
            }
            let _ = ends;
        }
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
                Event::GsReset => {
                    if self.rx_gs_reset {
                        self.gs_reset();
                    }
                }
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
                // no longer reaches it -- and with Rx Inst Chg off, no
                // part changes instrument from the wire at all.
                0xC0 if !self.rx_inst_chg => {}
                0xC0 if self.parts.locks[part] & LOCK_PROGRAM != 0 => {}
                0xB0 => {
                    // The part's own reception switches: bank select
                    // and NRPN fall on the floor when turned away.
                    if matches!(data1, 0 | 32) && !self.parts.rx_bank[part] {
                        continue;
                    }
                    if matches!(data1, 98 | 99) && !self.parts.rx_nrpn[part] {
                        continue;
                    }
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
        if self.synth.is_percussion(part as i32) {
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
        // The off-line watch: a stream that kept itself alive with
        // active sensing and then stopped means the line is gone --
        // the player force-quit, the cable pulled -- and the unit
        // releases everything rather than holding the last chord
        // forever. 420 ms, as the standard prescribes.
        if self.sensing {
            self.silent_frames += n as u64;
            if self.silent_frames > self.sample_rate as u64 * 42 / 100 {
                self.midi_off_line();
            }
        }
        self.pump_demo(n);
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
        // Translating, the part wears the MT-32 timbre's name -- unless
        // the panel has taken the program over, in which case the glass
        // must name what is actually loaded, or the edit looks ignored.
        let panel_owns_program =
            self.parts.locks.get(part).copied().unwrap_or(0) & LOCK_PROGRAM != 0;
        let name = if panel_owns_program {
            self.preset_name(bank, patch)
        } else {
            self.translator
                .channel_name(part)
                .map(str::to_string)
                .unwrap_or_else(|| self.preset_name(bank, patch))
        };
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
            drums: self.synth.is_percussion(part as i32),
        }
    }

    /// What the soundfont calls the preset on `bank`/`patch`, or
    /// "Empty" for a slot the font never filled: the numbering never
    /// shifts, the hole is simply named. (Playback still falls back to
    /// the font's default sound -- a synth must play something.)
    fn preset_name(&self, bank: i32, patch: i32) -> String {
        self.synth
            .get_sound_font()
            .get_presets()
            .iter()
            .find(|p| p.get_bank_number() == bank && p.get_patch_number() == patch)
            .map(|p| p.get_name().trim().to_string())
            .unwrap_or_else(|| "Empty".to_string())
    }

    /// The next drum kit the panel's arrows land on: the neighbour in
    /// the soundfont's own kit list. Kits are a list by nature; melodic
    /// programs are numbered slots and step plainly so their numbers
    /// stay put. `None` when the font carries no kits at all.
    pub fn neighbour_kit(&self, part: usize, step: i32) -> Option<u8> {
        let list = &self.drum_programs;
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

    /// The banks the font offers for the part's current instrument,
    /// walking from the current bank in `step`'s direction with
    /// wraparound; `None` on a drum part (kits ignore banks) or when
    /// the font has nothing to walk.
    pub fn neighbour_variation(&self, part: usize, step: i32) -> Option<u8> {
        let (bank, patch) = self.synth.channel_bank_patch(part)?;
        if bank >= 128 {
            return None;
        }
        let mut banks: Vec<u8> = self
            .synth
            .get_sound_font()
            .get_presets()
            .iter()
            .filter(|p| p.get_patch_number() == patch && p.get_bank_number() < 128)
            .map(|p| p.get_bank_number() as u8)
            .collect();
        banks.sort_unstable();
        banks.dedup();
        if banks.is_empty() {
            return None;
        }
        let len = banks.len() as i32;
        let at = match banks.binary_search(&(bank as u8)) {
            Ok(i) => i as i32 + step,
            Err(i) if step > 0 => i as i32,
            Err(i) => i as i32 - 1,
        };
        Some(banks[at.rem_euclid(len) as usize])
    }

    /// Put the part on a variation bank of its current instrument, the
    /// wire's own way: bank select completed by the program change.
    pub fn set_part_variation(&mut self, part: usize, bank: u8) {
        let (_, patch) = self.synth.channel_bank_patch(part).unwrap_or((0, 0));
        self.synth
            .process_midi_message(part as i32, 0xB0, 0, bank as i32);
        self.synth.process_midi_message(part as i32, 0xC0, patch, 0);
    }

    /// The part's current bank, 0-127, for the variation display.
    pub fn part_bank(&self, part: usize) -> u8 {
        let (bank, _) = self.synth.channel_bank_patch(part).unwrap_or((0, 0));
        (bank & 0x7F) as u8
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
    /// The chorus character in force, for the fascia.
    pub fn chorus_type(&self) -> crate::synth::ChorusType {
        self.synth.chorus_type()
    }

    /// Swap the chorus character (the fascia's Chorus Type edit).
    pub fn set_chorus_type(&mut self, chorus_type: crate::synth::ChorusType) {
        self.synth.set_chorus_type(chorus_type);
    }

    pub fn bank_name(&self) -> &str {
        self.synth
            .get_sound_font()
            .get_info()
            .get_bank_name()
            .trim()
    }

    /// How many regions the load mended and dropped putting a bruised
    /// bank right, for the host's log.
    pub fn bank_repairs(&self) -> (usize, usize) {
        self.synth.get_sound_font().get_repairs()
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

    /// A raw controller value as the part last received it, for the
    /// fascia's part-parameter editor.
    pub fn part_cc_value(&self, part: usize, controller: u8) -> u8 {
        self.synth.channel_cc(part, controller).unwrap_or(0)
    }

    /// A GS NRPN's current value in wire terms (relative parameters
    /// read 64 at neutral), for the same editor.
    pub fn part_nrpn_wire(&self, part: usize, msb: u8, lsb: u8) -> u8 {
        self.synth.channel_nrpn_wire(part as i32, msb, lsb)
    }

    /// Send one controller to a part from the fascia, exactly as the
    /// wire would.
    pub fn send_part_cc(&mut self, part: usize, controller: u8, value: u8) {
        self.part_cc(part, controller, value);
    }

    /// Send a GS NRPN to a part from the fascia: select, data entry,
    /// then the null the manual recommends.
    pub fn send_part_nrpn(&mut self, part: usize, msb: u8, lsb: u8, value: u8) {
        self.part_cc(part, 0x63, msb);
        self.part_cc(part, 0x62, lsb);
        self.part_cc(part, 0x06, value);
        self.part_cc(part, 0x65, 0x7F);
        self.part_cc(part, 0x64, 0x7F);
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

    // --- the demo player -------------------------------------------------

    /// Start demo song `song` from its beginning.
    pub fn demo_play(&mut self, song: usize) {
        self.demo_stop();
        let Some(data) = DEMO_DATA.get(song) else {
            return;
        };
        let Ok(file) = crate::synth::MidiFile::new(&mut std::io::Cursor::new(*data)) else {
            return;
        };
        self.demo = Some(Demo {
            events: file.events().collect(),
            at: 0,
            clock: 0.0,
        });
    }

    /// Stop the demo and let the room ring down.
    pub fn demo_stop(&mut self) {
        if self.demo.take().is_some() {
            self.all_notes_off();
        }
    }

    /// Whether a started song has run out of events.
    pub fn demo_finished(&self) -> bool {
        self.demo.as_ref().is_some_and(|d| d.at >= d.events.len())
    }

    /// Carry the demo forward by one rendered block, its due events
    /// going through the same part layer the wire uses.
    fn pump_demo(&mut self, frames: usize) {
        if self.demo.is_none() {
            return;
        }
        let step = frames as f64 / self.sample_rate as f64;
        if let Some(demo) = &mut self.demo {
            demo.clock += step;
        }
        loop {
            let due = match &self.demo {
                Some(d) if d.at < d.events.len() && d.events[d.at].0 <= d.clock => {
                    Some(d.events[d.at])
                }
                _ => None,
            };
            let Some((_, status, data1, data2)) = due else {
                break;
            };
            if let Some(demo) = &mut self.demo {
                demo.at += 1;
            }
            self.deliver(status & 0xF0, status & 0x0F, data1, data2);
        }
    }

    /// Everything off at once, releases respected.
    pub fn all_notes_off(&mut self) {
        self.synth.note_off_all(false);
        for sounded in &mut self.parts.sounded {
            *sounded = [SILENT; 128];
        }
    }

    /// The line went away under a stream that was keeping itself
    /// alive: everything sounding is released. Also the host's word
    /// for a machine reset -- the source power-cycling is the line
    /// dropping, whether or not it sensed. Quietly: the glass carrying
    /// an error through every ordinary boot would say less, not more.
    pub fn midi_off_line(&mut self) {
        self.sensing = false;
        self.in_sysex = false;
        self.all_notes_off();
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

    /// Close or open MIDI IN wholesale: demo mode's door.
    pub fn set_wire_closed(&mut self, closed: bool) {
        self.wire_closed = closed;
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

    // --- the system functions --------------------------------------------

    /// Master tune in tenths of a hertz around A4: 4153..=4662, 4400
    /// standard pitch.
    pub fn master_tune_tenths(&self) -> u16 {
        self.master_tune_tenths
    }

    pub fn set_master_tune_tenths(&mut self, tenths: u16) {
        self.master_tune_tenths = tenths.clamp(4153, 4662);
        let hz = self.master_tune_tenths as f32 / 10.0;
        self.synth.set_master_tune(12.0 * (hz / 440.0).log2());
    }

    /// The reverb character 0-7, Room1 to Panning Delay.
    pub fn reverb_type(&self) -> u8 {
        self.synth.reverb_type()
    }

    pub fn set_reverb_type(&mut self, reverb_type: u8) {
        self.synth.set_reverb_type(reverb_type);
    }

    pub fn rx_inst_chg(&self) -> bool {
        self.rx_inst_chg
    }

    pub fn set_rx_inst_chg(&mut self, on: bool) {
        self.rx_inst_chg = on;
    }

    pub fn rx_sysex(&self) -> bool {
        self.rx_sysex
    }

    pub fn set_rx_sysex(&mut self, on: bool) {
        self.rx_sysex = on;
    }

    pub fn rx_gs_reset(&self) -> bool {
        self.rx_gs_reset
    }

    pub fn set_rx_gs_reset(&mut self, on: bool) {
        self.rx_gs_reset = on;
    }

    /// The Back Up switch: whether the saved state is restored at the
    /// next power-on, or the unit wakes to the GS basic setting.
    pub fn backup(&self) -> bool {
        self.backup
    }

    pub fn set_backup(&mut self, on: bool) {
        self.backup = on;
    }

    /// The bar display style, 1-8.
    pub fn display_type(&self) -> u8 {
        self.display_type
    }

    pub fn set_display_type(&mut self, display_type: u8) {
        self.display_type = display_type.clamp(1, 8);
    }

    /// The peak-hold style: 0 off, 1 falls, 2 winks out, 3 floats up.
    pub fn peak_hold(&self) -> u8 {
        self.peak_hold
    }

    pub fn set_peak_hold(&mut self, peak_hold: u8) {
        self.peak_hold = peak_hold.min(3);
    }

    // --- part functions ---------------------------------------------------

    /// Whether `part` is a drum part.
    pub fn part_drums(&self, part: usize) -> bool {
        self.synth.is_percussion(part as i32)
    }

    /// Part Mode: make `part` a drum part on the font's first kit, or a
    /// normal part back on Piano 1.
    pub fn set_part_drums(&mut self, part: usize, drums: bool) {
        self.synth.set_percussion(part as i32, drums);
        if drums {
            let kit = self.drum_programs.first().copied().unwrap_or(0);
            self.synth
                .process_midi_message(part as i32, 0xC0, kit as i32, 0);
        }
    }

    /// Bend Range in semitones, -24..=+24 -- negative bends the other
    /// way, as the unit's own panel allows.
    pub fn part_bend_range(&self, part: usize) -> i8 {
        self.synth.channel_bend_range(part as i32)
    }

    pub fn set_part_bend_range(&mut self, part: usize, semitones: i8) {
        self.synth
            .set_channel_bend_range(part as i32, semitones.clamp(-24, 24));
    }

    /// Fine Tune as the unit shows it, -12..=+12: the full RPN fine
    /// range of a semitone either way, in the panel's twelve steps.
    pub fn part_fine_tune(&self, part: usize) -> i8 {
        self.synth.channel_fine_tune_display(part as i32)
    }

    pub fn set_part_fine_tune(&mut self, part: usize, value: i8) {
        self.synth
            .set_channel_fine_tune_display(part as i32, value.clamp(-12, 12));
    }

    /// Voice Reserve, kept and shown as the unit keeps it. This well
    /// runs deeper than the hardware's, so nothing ever starves and
    /// the number never has to act.
    pub fn part_voice_reserve(&self, part: usize) -> u8 {
        self.parts.voice_rsv.get(part).copied().unwrap_or(2)
    }

    pub fn set_part_voice_reserve(&mut self, part: usize, voices: u8) {
        if let Some(slot) = self.parts.voice_rsv.get_mut(part) {
            *slot = voices.min(28);
        }
    }

    /// The part's bank-select reception switch.
    pub fn part_rx_bank(&self, part: usize) -> bool {
        self.parts.rx_bank.get(part).copied().unwrap_or(true)
    }

    pub fn set_part_rx_bank(&mut self, part: usize, on: bool) {
        if let Some(slot) = self.parts.rx_bank.get_mut(part) {
            *slot = on;
        }
    }

    /// The part's NRPN reception switch.
    pub fn part_rx_nrpn(&self, part: usize) -> bool {
        self.parts.rx_nrpn.get(part).copied().unwrap_or(true)
    }

    pub fn set_part_rx_nrpn(&mut self, part: usize, on: bool) {
        if let Some(slot) = self.parts.rx_nrpn.get_mut(part) {
            *slot = on;
        }
    }

    /// Key Range L/H as note numbers.
    pub fn part_key_range(&self, part: usize) -> (u8, u8) {
        self.synth.channel_key_range(part as i32)
    }

    pub fn set_part_key_range(&mut self, part: usize, lo: u8, hi: u8) {
        self.synth.set_channel_key_range(part as i32, lo, hi);
    }

    /// Velocity Sens (depth, offset), 64/64 neutral.
    pub fn part_velo_sens(&self, part: usize) -> (u8, u8) {
        self.synth.channel_velo_sens(part as i32)
    }

    pub fn set_part_velo_sens(&mut self, part: usize, depth: u8, offset: u8) {
        self.synth.set_channel_velo_sens(part as i32, depth, offset);
    }

    /// M/P Mode: whether the part plays one note at a time.
    pub fn part_mono(&self, part: usize) -> bool {
        self.synth.channel_mono(part as i32)
    }

    pub fn set_part_mono(&mut self, part: usize, mono: bool) {
        self.synth.set_channel_mono(part as i32, mono);
    }

    /// Modulation Depth, the GS factory 10 neutral.
    pub fn part_mod_depth(&self, part: usize) -> u8 {
        self.synth.channel_mod_depth(part as i32)
    }

    pub fn set_part_mod_depth(&mut self, part: usize, depth: u8) {
        self.synth.set_channel_mod_depth(part as i32, depth);
    }

    // --- resets and the saved state --------------------------------------

    /// The GS basic setting: every part and all-parts value back to the
    /// factory, the system functions untouched -- the unit's Init GS,
    /// and what an all-reset message on the wire performs.
    pub fn gs_reset(&mut self) {
        self.synth.reset();
        for part in 0..PARTS {
            self.synth.set_percussion(part as i32, part == DRUM_PART);
        }
        self.parts = Parts::new();
        self.cm64_selected = false;
        self.set_master_volume_cc(127);
        self.master_pan = 64;
        self.master_shift = 0;
        self.set_master_reverb(64);
        self.set_master_chorus(64);
        self.synth
            .set_chorus_type(crate::synth::ChorusType::Chorus3);
        self.synth.set_reverb_type(4);
        self.set_master_tune_tenths(4400);
    }

    /// The factory preset: the GS basic setting and the system
    /// functions too -- the unit's Init All (the host puts the built-in
    /// bank back around it).
    pub fn factory_reset(&mut self) {
        self.gs_reset();
        self.device_id = 17;
        self.rx_inst_chg = true;
        self.rx_sysex = true;
        self.rx_gs_reset = true;
        self.backup = true;
        self.display_type = 1;
        self.peak_hold = 1;
    }

    /// The battery-backed memory, as bytes for the host to keep: the
    /// system functions and, for the Back Up switch to honour, the
    /// whole current state of every part.
    pub fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(512);
        out.extend_from_slice(b"CSYN");
        out.push(2);
        // System functions, honoured regardless of Back Up.
        out.push(self.device_id);
        out.push(self.rx_inst_chg as u8);
        out.push(self.rx_sysex as u8);
        out.push(self.rx_gs_reset as u8);
        out.push(self.backup as u8);
        out.push(self.display_type);
        out.push(self.peak_hold);
        // The all-parts settings.
        out.push(self.master_volume_cc);
        out.push(self.master_pan);
        out.push(self.master_shift as u8);
        out.push(self.master_reverb_cc);
        out.push(self.master_chorus_cc);
        out.push(self.synth.chorus_type().index());
        out.push(self.synth.reverb_type());
        out.extend_from_slice(&self.master_tune_tenths.to_le_bytes());
        // Every part in full.
        for part in 0..PARTS {
            let (bank, patch) = self.synth.channel_bank_patch(part).unwrap_or((0, 0));
            let cc = |controller| self.synth.channel_cc(part, controller).unwrap_or(0);
            let (lo, hi) = self.part_key_range(part);
            let (depth, offset) = self.part_velo_sens(part);
            out.push(self.parts.rx_channel[part].map_or(0xFF, |c| c));
            out.push(self.parts.mute[part] as u8);
            out.push(self.parts.key_shift[part] as u8);
            out.push(self.parts.level_cap[part]);
            out.push(self.parts.locks[part]);
            out.push(self.part_drums(part) as u8);
            out.push((bank & 0xFF) as u8);
            out.push((patch & 0x7F) as u8);
            for controller in [10u8, 91, 93, 1, 5, 11, 65, 66, 67] {
                out.push(cc(controller));
            }
            out.push(self.part_bend_range(part) as u8);
            out.push(lo);
            out.push(hi);
            out.push(depth);
            out.push(offset);
            out.push(self.part_mod_depth(part));
            out.push(self.synth.channel_mono(part as i32) as u8);
            for (msb, lsb) in [
                (0x01u8, 0x08u8),
                (0x01, 0x09),
                (0x01, 0x0A),
                (0x01, 0x20),
                (0x01, 0x21),
                (0x01, 0x63),
                (0x01, 0x64),
                (0x01, 0x66),
            ] {
                out.push(self.part_nrpn_wire(part, msb, lsb));
            }
            out.push(self.part_voice_reserve(part));
            out.push(self.part_fine_tune(part) as u8);
            out.push(self.part_rx_bank(part) as u8);
            out.push(self.part_rx_nrpn(part) as u8);
        }
        out
    }

    /// Wake up on the battery-backed memory: the system functions come
    /// back always; the parts come back when the Back Up switch in the
    /// saved bytes says so, and wake to the GS basic setting when it
    /// says off. Unknown or damaged bytes leave the unit as it stands.
    pub fn load_state(&mut self, bytes: &[u8]) {
        const SYSTEM: usize = 5 + 7;
        const MASTERS: usize = 9;
        const PART: usize = 6 + 2 + 9 + 7 + 8 + 4;
        if bytes.len() < SYSTEM + MASTERS + PARTS * PART || &bytes[..4] != b"CSYN" || bytes[4] != 2
        {
            return;
        }
        let b = &bytes[5..];
        self.device_id = b[0].clamp(1, 32);
        self.rx_inst_chg = b[1] != 0;
        self.rx_sysex = b[2] != 0;
        self.rx_gs_reset = b[3] != 0;
        self.backup = b[4] != 0;
        self.set_display_type(b[5]);
        self.set_peak_hold(b[6]);
        if !self.backup {
            self.gs_reset();
            return;
        }
        let b = &b[7..];
        self.set_master_volume_cc(b[0].min(127));
        self.master_pan = b[1].min(127);
        self.master_shift = (b[2] as i8).clamp(-24, 24);
        self.set_master_reverb(b[3].min(127));
        self.set_master_chorus(b[4].min(127));
        self.synth
            .set_chorus_type(crate::synth::ChorusType::from_index(b[5]));
        self.synth.set_reverb_type(b[6]);
        self.set_master_tune_tenths(u16::from_le_bytes([b[7], b[8]]));
        let mut b = &b[9..];
        for part in 0..PARTS {
            let p = &b[..PART];
            self.parts.rx_channel[part] = (p[0] != 0xFF).then_some(p[0].min(15));
            self.parts.mute[part] = p[1] != 0;
            self.parts.key_shift[part] = (p[2] as i8).clamp(-24, 24);
            self.parts.level_cap[part] = p[3].min(127);
            self.parts.locks[part] = p[4];
            let drums = p[5] != 0;
            self.synth.set_percussion(part as i32, drums);
            self.synth
                .process_midi_message(part as i32, 0xB0, 0, p[6].min(127) as i32);
            self.synth
                .process_midi_message(part as i32, 0xC0, p[7] as i32, 0);
            for (i, controller) in [10u8, 91, 93, 1, 5, 11, 65, 66, 67].iter().enumerate() {
                self.synth.process_midi_message(
                    part as i32,
                    0xB0,
                    *controller as i32,
                    p[8 + i].min(127) as i32,
                );
            }
            self.set_part_bend_range(part, p[17] as i8);
            self.set_part_key_range(part, p[18], p[19]);
            self.set_part_velo_sens(part, p[20], p[21]);
            self.set_part_mod_depth(part, p[22]);
            self.synth.set_channel_mono(part as i32, p[23] != 0);
            self.set_part_voice_reserve(part, p[32]);
            self.set_part_fine_tune(part, p[33] as i8);
            self.set_part_rx_bank(part, p[34] != 0);
            self.set_part_rx_nrpn(part, p[35] != 0);
            for (i, (msb, lsb)) in [
                (0x01u8, 0x08u8),
                (0x01, 0x09),
                (0x01, 0x0A),
                (0x01, 0x20),
                (0x01, 0x21),
                (0x01, 0x63),
                (0x01, 0x64),
                (0x01, 0x66),
            ]
            .iter()
            .enumerate()
            {
                self.send_part_nrpn(part, *msb, *lsb, p[24 + i].min(127));
            }
            b = &b[PART..];
        }
        // The level caps take effect through the usual path.
        for part in 0..PARTS {
            let cap = self.parts.level_cap[part];
            self.set_part_level(part, cap);
        }
    }
}
