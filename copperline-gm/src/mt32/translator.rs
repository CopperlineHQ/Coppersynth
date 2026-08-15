//! The MT-32 -> GM byte-stream machine.
//!
//! Bytes arrive one at a time off an emulated serial line, exactly as a
//! real MT-32 receives them; translated messages come out ready for a GM
//! synthesizer. In between sits a model of the MT-32 the game thinks it
//! is talking to: its patch memory, its custom timbre names, its rhythm
//! setup, its part-to-channel assignment. Sysex is always consumed --
//! nothing downstream understands it -- and what it *changes* is
//! remembered, so a game that re-points patch 5 at a harp and then
//! selects patch 5 gets a harp.
//!
//! What cannot be carried over is a custom timbre's actual sound: those
//! are synthesis parameters for hardware we are not emulating. The name
//! the game uploads is matched against Sierra's own MT-32 -> GM choices
//! first, then against the preset timbre names, which catches the common
//! "modified preset" case ("FrHorn1MS2" lands on the French horn). A
//! name matching nothing falls back to a square lead: audibly synthetic,
//! deliberately so.

use super::tables;

/// How the translator decides whether to translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mt32Mode {
    /// Pass everything through untouched (sysex is still consumed).
    Off,
    /// Translate from the first byte.
    On,
    /// Pass through until MT-32 sysex identifies the traffic (Roland
    /// model ID 0x16), then translate; a GM reset switches back off.
    Auto,
}

/// What the machine emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A channel message for the synthesizer, post-translation.
    Midi {
        command: u8,
        channel: u8,
        data1: u8,
        data2: u8,
    },
    /// Text a game wrote to the MT-32's display, for the OSD.
    Display(String),
    /// Universal sysex master volume, 0..=127.
    MasterVolume(u8),
    /// Auto mode decided: true once MT-32 traffic is identified.
    Translating(bool),
}

/// One patch-memory entry, as far as translation cares.
#[derive(Debug, Clone, Copy)]
struct Patch {
    /// 0 = preset group a, 1 = group b, 2 = memory (custom), 3 = rhythm.
    timbre_group: u8,
    timbre_number: u8,
    /// Stored 0..=48, meaning -24..=24 semitones.
    key_shift: u8,
    /// Bender range in semitones, 0..=24.
    bender_range: u8,
}

impl Patch {
    /// The power-on state: patch i selects preset timbre i (group a for
    /// the first 64, group b for the rest), no shift, bend range 12.
    fn default_for(i: usize) -> Self {
        Self {
            timbre_group: if i < 64 { 0 } else { 1 },
            timbre_number: (i % 64) as u8,
            key_shift: 24,
            bender_range: 12,
        }
    }
}

const SYSEX_LIMIT: usize = 2048;
/// 0xFF in `emitted_key` marks "no note sounding".
const NO_KEY: u8 = 0xFF;

pub struct Mt32Translator {
    mode: Mt32Mode,
    active: bool,

    // --- incremental MIDI parser ---------------------------------------
    status: u8,
    data: [u8; 2],
    have: usize,
    sysex: Option<Vec<u8>>,

    // --- the modelled MT-32 --------------------------------------------
    patches: [Patch; 128],
    /// Timbre-memory names, 10 columns each, as uploaded.
    timbre_names: [[u8; 10]; 64],
    /// Rhythm setup: key 24..=87 -> assigned timbre value, or None where
    /// the game has not written one (identity translation applies).
    rhythm_assign: [Option<u8>; 64],
    /// MIDI channel of parts 1..=8 and the rhythm part (0-based, 0xFF =
    /// off), from the system area; defaults are channels 2-9 and 10.
    part_channel: [u8; 9],

    // --- per-channel translation state ---------------------------------
    /// The GM program last emitted per channel.
    gm_program: [u8; 16],
    /// Semitone shift applied to notes per channel.
    key_shift: [i16; 16],
    /// Velocity adjustment per channel, from the Sierra custom tables.
    velocity_adjust: [i16; 16],
    /// Bender range last sent per channel (0xFF = never).
    sent_bend_range: [u8; 16],
    /// Where each sounding note was emitted, so its note-off lands on the
    /// same key whatever the shift has become since.
    emitted_key: [[u8; 128]; 16],

    events: Vec<Event>,
}

impl Mt32Translator {
    pub fn new(mode: Mt32Mode) -> Self {
        let mut t = Self {
            mode,
            active: mode == Mt32Mode::On,
            status: 0,
            data: [0; 2],
            have: 0,
            sysex: None,
            patches: std::array::from_fn(Patch::default_for),
            timbre_names: [[b' '; 10]; 64],
            rhythm_assign: [None; 64],
            part_channel: [1, 2, 3, 4, 5, 6, 7, 8, 9],
            gm_program: [0; 16],
            key_shift: [0; 16],
            velocity_adjust: [0; 16],
            sent_bend_range: [0xFF; 16],
            emitted_key: [[NO_KEY; 128]; 16],
            events: Vec::new(),
        };
        if t.active {
            t.on_activated();
        }
        t
    }

    pub fn is_translating(&self) -> bool {
        self.active
    }

    /// Feed one byte off the wire; drain what it produced.
    pub fn push(&mut self, byte: u8) -> std::vec::Drain<'_, Event> {
        self.accept(byte);
        self.events.drain(..)
    }

    fn accept(&mut self, byte: u8) {
        // Real-time bytes may interleave anything, sysex included.
        if byte >= 0xF8 {
            return;
        }
        if byte == 0xF0 {
            self.sysex = Some(Vec::new());
            return;
        }
        if byte == 0xF7 {
            if let Some(body) = self.sysex.take() {
                self.on_sysex(&body);
            }
            return;
        }
        if let Some(body) = self.sysex.as_mut() {
            if byte < 0x80 {
                if body.len() < SYSEX_LIMIT {
                    body.push(byte);
                }
                return;
            }
            // A status byte inside sysex ends it (missing F7).
            let body = self.sysex.take().unwrap();
            self.on_sysex(&body);
        }
        if byte >= 0x80 {
            if byte >= 0xF1 {
                // System common we do not model; clears running status.
                self.status = 0;
                self.have = 0;
                return;
            }
            self.status = byte;
            self.have = 0;
            return;
        }
        if self.status == 0 {
            return;
        }
        self.data[self.have] = byte;
        self.have += 1;
        let need = match self.status & 0xF0 {
            0xC0 | 0xD0 => 1,
            _ => 2,
        };
        if self.have == need {
            self.have = 0;
            let (d1, d2) = (self.data[0], self.data[1]);
            self.on_message(self.status, d1, d2);
        }
    }

    fn emit(&mut self, command: u8, channel: u8, data1: u8, data2: u8) {
        self.events.push(Event::Midi {
            command,
            channel,
            data1: data1 & 0x7F,
            data2: data2 & 0x7F,
        });
    }

    fn on_message(&mut self, status: u8, d1: u8, d2: u8) {
        let command = status & 0xF0;
        let channel = status & 0x0F;
        if !self.active {
            self.emit(command, channel, d1, d2);
            return;
        }
        let rhythm = channel == self.part_channel[8];
        match command {
            0x90 | 0x80 => self.on_note(command, channel, d1, d2, rhythm),
            0xC0 => self.on_program(channel, d1),
            0xB0 => match d1 {
                // The MT-32 pans the opposite way to the MIDI spec.
                0x0A => {
                    let flipped = (0x80u16 - d2 as u16).min(0x7F) as u8;
                    self.emit(command, channel, d1, flipped);
                }
                _ => self.emit(command, channel, d1, d2),
            },
            _ => self.emit(command, channel, d1, d2),
        }
    }

    fn on_note(&mut self, command: u8, channel: u8, key: u8, velocity: u8, rhythm: bool) {
        let is_off = command == 0x80 || (command == 0x90 && velocity == 0);
        if is_off {
            let emitted = self.emitted_key[channel as usize][key as usize];
            if emitted != NO_KEY {
                self.emitted_key[channel as usize][key as usize] = NO_KEY;
                self.emit(command, channel, emitted, velocity);
            }
            return;
        }
        let mapped = if rhythm {
            match self.rhythm_key(key) {
                Some(k) => k,
                // A key with nothing sensible behind it stays silent
                // rather than firing a random drum.
                None => return,
            }
        } else {
            let shifted = key as i16 + self.key_shift[channel as usize];
            if !(0..=127).contains(&shifted) {
                return;
            }
            shifted as u8
        };
        let velocity = (velocity as i16 + self.velocity_adjust[channel as usize]).clamp(1, 127);
        self.emitted_key[channel as usize][key as usize] = mapped;
        self.emit(0x90, channel, mapped, velocity as u8);
    }

    fn rhythm_key(&self, key: u8) -> Option<u8> {
        if let Some(assigned) = (24..=87)
            .contains(&key)
            .then(|| self.rhythm_assign[(key - 24) as usize])
            .flatten()
        {
            // The game re-assigned this key. Values 64..=93 name the
            // preset rhythm timbres, whose default keys the GM map keeps,
            // so route to that timbre's home key.
            if (64..=93).contains(&assigned) {
                return tables::rhythm_timbre_home_key(assigned - 64);
            }
            // A custom timbre on a drum key has no GM meaning; identity
            // is the least wrong answer inside the kit.
            return tables::rhythm_key_to_gm(key);
        }
        tables::rhythm_key_to_gm(key)
    }

    fn on_program(&mut self, channel: u8, patch: u8) {
        let ch = channel as usize;
        let p = self.patches[patch as usize];
        let (gm, extra_shift, vel_adjust) = self.resolve_patch(&p);
        self.gm_program[ch] = gm;
        self.key_shift[ch] = p.key_shift as i16 - 24 + extra_shift as i16;
        self.velocity_adjust[ch] = vel_adjust as i16;
        self.emit(0xC0, channel, gm, 0);
        self.ensure_bend_range(channel, p.bender_range);
    }

    /// The GM rendering of a patch: program, extra key shift, velocity
    /// adjustment (the latter two from Sierra's tables for customs).
    fn resolve_patch(&self, p: &Patch) -> (u8, i8, i8) {
        match p.timbre_group {
            0 => (tables::PATCH_TO_GM[(p.timbre_number & 63) as usize], 0, 0),
            1 => (
                tables::PATCH_TO_GM[64 + (p.timbre_number & 63) as usize],
                0,
                0,
            ),
            2 => {
                let name = &self.timbre_names[(p.timbre_number & 63) as usize];
                tables::match_custom_name(name)
            }
            // Group 3 puts a rhythm timbre on a melodic part; GM has no
            // equivalent, so a percussive stand-in does the least harm.
            _ => (117, 0, 0),
        }
    }

    fn ensure_bend_range(&mut self, channel: u8, semitones: u8) {
        if self.sent_bend_range[channel as usize] == semitones {
            return;
        }
        self.sent_bend_range[channel as usize] = semitones;
        // RPN 0: pitch bend sensitivity.
        self.emit(0xB0, channel, 0x65, 0);
        self.emit(0xB0, channel, 0x64, 0);
        self.emit(0xB0, channel, 0x06, semitones);
    }

    /// Activation: the moment the stream is known to be MT-32 traffic,
    /// give every part the MT-32's resting state -- bend range 12 --
    /// before any of its notes arrive.
    fn on_activated(&mut self) {
        for part in 0..8 {
            let channel = self.part_channel[part];
            if channel < 16 {
                self.ensure_bend_range(channel, 12);
            }
        }
        self.events.push(Event::Translating(true));
    }

    // --- sysex ----------------------------------------------------------

    fn on_sysex(&mut self, body: &[u8]) {
        // Universal master volume: 7F dev 04 01 ll mm.
        if body.len() >= 6 && body[0] == 0x7F && body[2] == 0x04 && body[3] == 0x01 {
            self.events.push(Event::MasterVolume(body[5] & 0x7F));
            return;
        }
        // Universal GM reset: 7E dev 09 01|02|03.
        if body.len() >= 4 && body[0] == 0x7E && body[2] == 0x09 {
            if self.mode == Mt32Mode::Auto && self.active {
                self.active = false;
                self.events.push(Event::Translating(false));
            }
            return;
        }
        // Roland DT1 to an MT-32: 41 dev 16 12 aa bb cc data.. sum.
        if body.len() >= 8 && body[0] == 0x41 && body[2] == 0x16 && body[3] == 0x12 {
            if self.mode == Mt32Mode::Auto && !self.active {
                self.active = true;
                self.on_activated();
            }
            if !self.active {
                return;
            }
            let addr = ((body[4] as u32) << 14) | ((body[5] as u32) << 7) | body[6] as u32;
            let data = &body[7..body.len() - 1];
            let sum: u32 = body[4..].iter().map(|&b| b as u32).sum();
            if sum % 128 != 0 {
                // A corrupt write is dropped whole; guessing at half a
                // patch table would be worse than missing it.
                return;
            }
            self.apply_write(addr, data);
        }
    }

    /// Walk a DT1 write across the modelled memory, byte by byte, so a
    /// block write spanning entries lands exactly as it would in the
    /// device.
    fn apply_write(&mut self, addr: u32, data: &[u8]) {
        const PATCH_TEMP: u32 = 0x03 << 14;
        const RHYTHM_SETUP: u32 = (0x03 << 14) | (0x01 << 7) | 0x10;
        const PATCH_MEMORY: u32 = 0x05 << 14;
        const TIMBRE_MEMORY: u32 = 0x08 << 14;
        const SYSTEM: u32 = 0x10 << 14;
        const DISPLAY: u32 = 0x20 << 14;

        if addr >= DISPLAY && addr < DISPLAY + 0x80 {
            let text: String = data
                .iter()
                .map(|&b| {
                    if (0x20..0x7F).contains(&b) {
                        b as char
                    } else {
                        ' '
                    }
                })
                .collect();
            let text = text.trim().to_string();
            if !text.is_empty() {
                self.events.push(Event::Display(text));
            }
            return;
        }

        for (i, &value) in data.iter().enumerate() {
            let at = addr + i as u32;
            match at {
                a if a >= TIMBRE_MEMORY && a < TIMBRE_MEMORY + 64 * 256 => {
                    let offset = a - TIMBRE_MEMORY;
                    let (slot, byte) = ((offset / 256) as usize, offset % 256);
                    if byte < 10 {
                        self.timbre_names[slot][byte as usize] = value;
                    }
                }
                a if a >= PATCH_MEMORY && a < PATCH_MEMORY + 128 * 8 => {
                    let offset = a - PATCH_MEMORY;
                    let (slot, byte) = ((offset / 8) as usize, offset % 8);
                    let p = &mut self.patches[slot];
                    match byte {
                        0 => p.timbre_group = value & 3,
                        1 => p.timbre_number = value & 63,
                        2 => p.key_shift = value.min(48),
                        4 => p.bender_range = value.min(24),
                        _ => {}
                    }
                }
                a if a >= RHYTHM_SETUP && a < RHYTHM_SETUP + 64 * 4 => {
                    let offset = a - RHYTHM_SETUP;
                    let (key, byte) = ((offset / 4) as usize, offset % 4);
                    if byte == 0 {
                        self.rhythm_assign[key] = Some(value);
                    }
                }
                a if a >= PATCH_TEMP && a < PATCH_TEMP + 8 * 16 => {
                    // A game writing a part's live patch skips patch
                    // memory entirely; retarget the part on the spot.
                    let offset = a - PATCH_TEMP;
                    let (part, byte) = ((offset / 16) as usize, offset % 16);
                    let channel = self.part_channel[part];
                    if channel >= 16 {
                        continue;
                    }
                    let slot = 127 - part; // scratch entries, per part
                    let p = &mut self.patches[slot];
                    match byte {
                        0 => p.timbre_group = value & 3,
                        1 => p.timbre_number = value & 63,
                        2 => p.key_shift = value.min(48),
                        4 => p.bender_range = value.min(24),
                        _ => {}
                    }
                    if byte == 1 {
                        self.on_program_from_temp(channel, slot);
                    }
                }
                a if a >= SYSTEM && a < SYSTEM + 0x20 => {
                    let offset = a - SYSTEM;
                    if (0x0D..=0x15).contains(&offset) {
                        let part = (offset - 0x0D) as usize;
                        // 0..=15 are channels; 16 means off.
                        self.part_channel[part] = if value < 16 { value } else { 0xFF };
                    }
                    if offset == 0x16 {
                        self.events.push(Event::MasterVolume(value.min(100)));
                    }
                }
                _ => {}
            }
        }
    }

    fn on_program_from_temp(&mut self, channel: u8, slot: usize) {
        let p = self.patches[slot];
        let ch = channel as usize;
        let (gm, extra_shift, vel_adjust) = self.resolve_patch(&p);
        self.gm_program[ch] = gm;
        self.key_shift[ch] = p.key_shift as i16 - 24 + extra_shift as i16;
        self.velocity_adjust[ch] = vel_adjust as i16;
        self.emit(0xC0, channel, gm, 0);
        self.ensure_bend_range(channel, p.bender_range);
    }
}
