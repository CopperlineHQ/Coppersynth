#![allow(dead_code)]

#[derive(Debug, PartialEq, Eq)]
enum DataType {
    None,
    Rpn,
    Nrpn,
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct Channel {
    pub(crate) is_percussion_channel: bool,

    bank_number: i32,
    patch_number: i32,

    modulation: i16,
    volume: i16,
    pan: i16,
    expression: i16,
    hold_pedal: bool,
    sostenuto_pedal: bool,
    soft_pedal: bool,
    portamento_pedal: bool,
    /// CC5, 0-127; 0 is the fastest glide.
    portamento_time: u8,
    /// The channel's most recent note-on, where a portamento glide
    /// starts from when the pedal is down.
    last_key: Option<u8>,
    /// CC84's armed source key, spent by the next note-on.
    portamento_source: Option<u8>,
    /// Mode 4 (mono, M=1) against the power-on Mode 3.
    mono_mode: bool,
    /// Channel pressure, received and offered to bank modulators; the
    /// unit itself routes it nowhere by default.
    channel_pressure: u8,
    /// Polyphonic key pressure per key, likewise.
    poly_pressure: [u8; 128],
    /// CC0 latches here and lands on the program change, as the unit
    /// suspends bank select until then.
    pending_bank: Option<i32>,

    reverb_send: u8,
    chorus_send: u8,

    rpn: i16,
    nrpn: i16,
    /// The GS tone-modify offsets, -50..+50 about zero, applied to the
    /// notes that start after them (program changes and CC121 leave
    /// them standing, as the manual specifies).
    vib_rate_off: i32,
    vib_depth_off: i32,
    vib_delay_off: i32,
    cutoff_off: i32,
    resonance_off: i32,
    eg_attack_off: i32,
    eg_decay_off: i32,
    eg_release_off: i32,
    /// The drum-instrument overrides, keyed by note; only a percussion
    /// part routes anything into them.
    drum_pitch: [i8; 128],
    drum_level: [Option<u8>; 128],
    drum_pan: [Option<u8>; 128],
    drum_reverb: [Option<u8>; 128],
    drum_chorus: [Option<u8>; 128],
    pitch_bend_range: i16,
    coarse_tune: i16,
    fine_tune: i16,

    pitch_bend: f32,

    /// The unit's master tune in semitones, shared by every channel;
    /// the synthesizer stamps it here so a voice's pitch math reads
    /// one place.
    master_tune: f32,
    /// The playable window (Key Range L/H): notes outside it are not
    /// sounded on this part.
    key_range_lo: u8,
    key_range_hi: u8,
    /// The velocity curve (Velocity Sens Depth/Offset), 64/64 neutral.
    velo_depth: u8,
    velo_offset: u8,
    /// Modulation depth: how far the mod wheel's full travel reaches,
    /// with the GS factory 10 as the neutral scale.
    mod_depth: u8,

    /// Every controller's last raw value, for the modulator sources. The
    /// named fields above stay authoritative for the synthesizer's own
    /// paths; this table answers for the arbitrary CCs a bank is free to
    /// route from.
    cc: [u8; 128],

    last_data_type: DataType,
}

impl Channel {
    pub(crate) fn new(is_percussion_channel: bool) -> Self {
        let mut channel = Self {
            is_percussion_channel,
            bank_number: 0,
            patch_number: 0,
            modulation: 0,
            volume: 0,
            pan: 0,
            expression: 0,
            hold_pedal: false,
            sostenuto_pedal: false,
            soft_pedal: false,
            portamento_pedal: false,
            portamento_time: 0,
            last_key: None,
            portamento_source: None,
            mono_mode: false,
            channel_pressure: 0,
            poly_pressure: [0; 128],
            pending_bank: None,
            reverb_send: 0,
            chorus_send: 0,
            rpn: 0,
            nrpn: 0,
            vib_rate_off: 0,
            vib_depth_off: 0,
            vib_delay_off: 0,
            cutoff_off: 0,
            resonance_off: 0,
            eg_attack_off: 0,
            eg_decay_off: 0,
            eg_release_off: 0,
            drum_pitch: [0; 128],
            drum_level: [None; 128],
            drum_pan: [None; 128],
            drum_reverb: [None; 128],
            drum_chorus: [None; 128],
            pitch_bend_range: 0,
            coarse_tune: 0,
            fine_tune: 0,
            pitch_bend: 0_f32,
            master_tune: 0_f32,
            key_range_lo: 0,
            key_range_hi: 127,
            velo_depth: 64,
            velo_offset: 64,
            mod_depth: 10,
            cc: [0; 128],
            last_data_type: DataType::None,
        };

        channel.reset();

        channel
    }

    pub(crate) fn reset(&mut self) {
        self.bank_number = if self.is_percussion_channel { 128 } else { 0 };
        self.patch_number = 0;

        self.modulation = 0;
        self.volume = 100 << 7;
        self.pan = 64 << 7;
        self.expression = 127 << 7;
        self.hold_pedal = false;
        self.sostenuto_pedal = false;
        self.soft_pedal = false;
        self.portamento_pedal = false;
        self.portamento_time = 0;
        self.last_key = None;
        self.portamento_source = None;
        self.mono_mode = false;
        self.channel_pressure = 0;
        self.poly_pressure = [0; 128];
        self.pending_bank = None;

        self.reverb_send = 40;
        self.chorus_send = 0;

        self.rpn = -1;
        self.nrpn = -1;
        self.vib_rate_off = 0;
        self.vib_depth_off = 0;
        self.vib_delay_off = 0;
        self.cutoff_off = 0;
        self.resonance_off = 0;
        self.eg_attack_off = 0;
        self.eg_decay_off = 0;
        self.eg_release_off = 0;
        self.drum_pitch = [0; 128];
        self.drum_level = [None; 128];
        self.drum_pan = [None; 128];
        self.drum_reverb = [None; 128];
        self.drum_chorus = [None; 128];
        self.pitch_bend_range = 2 << 7;
        self.coarse_tune = 0;
        self.fine_tune = 8192;
        self.key_range_lo = 0;
        self.key_range_hi = 127;
        self.velo_depth = 64;
        self.velo_offset = 64;
        self.mod_depth = 10;

        self.pitch_bend = 0_f32;

        // The raw table mirrors the named defaults above, so a modulator
        // reading CC7 before any controller arrives sees the same state
        // the synthesizer's own volume path uses.
        self.cc = [0; 128];
        self.cc[7] = 100;
        self.cc[10] = 64;
        self.cc[11] = 127;
        self.cc[91] = 40;
    }

    pub(crate) fn reset_all_controllers(&mut self) {
        self.modulation = 0;
        self.expression = 127 << 7;
        self.hold_pedal = false;
        self.sostenuto_pedal = false;
        self.soft_pedal = false;
        self.portamento_pedal = false;
        self.portamento_source = None;
        self.channel_pressure = 0;
        self.poly_pressure = [0; 128];

        self.rpn = -1;

        self.pitch_bend = 0_f32;

        self.cc[1] = 0;
        self.cc[11] = 127;
        self.cc[64] = 0;
    }

    pub(crate) fn set_bank(&mut self, value: i32) {
        // The unit suspends bank select until the program change that
        // completes it; a bank sent on its own must not retarget notes
        // already choosing presets. (Drum parts ignore it outright.)
        if self.is_percussion_channel {
            return;
        }
        self.pending_bank = Some(value);
    }

    pub(crate) fn set_patch(&mut self, value: i32) {
        if let Some(bank) = self.pending_bank.take() {
            self.bank_number = bank;
        }
        self.patch_number = value;
    }

    pub(crate) fn set_modulation_coarse(&mut self, value: i32) {
        self.modulation = (self.modulation & 0x7F) | (value << 7) as i16;
    }

    pub(crate) fn set_modulation_fine(&mut self, value: i32) {
        self.modulation = (((self.modulation as i32) & 0xFF80) | value) as i16;
    }

    pub(crate) fn set_volume_coarse(&mut self, value: i32) {
        self.volume = (self.volume & 0x7F) | (value << 7) as i16;
    }

    pub(crate) fn set_volume_fine(&mut self, value: i32) {
        self.volume = (((self.volume as i32) & 0xFF80) | value) as i16;
    }

    pub(crate) fn set_pan_coarse(&mut self, value: i32) {
        self.pan = (self.pan & 0x7F) | (value << 7) as i16;
    }

    pub(crate) fn set_pan_fine(&mut self, value: i32) {
        self.pan = (((self.pan as i32) & 0xFF80) | value) as i16;
    }

    pub(crate) fn set_expression_coarse(&mut self, value: i32) {
        self.expression = (self.expression & 0x7F) | (value << 7) as i16;
    }

    pub(crate) fn set_expression_fine(&mut self, value: i32) {
        self.expression = (((self.expression as i32) & 0xFF80) | value) as i16;
    }

    pub(crate) fn set_hold_pedal(&mut self, value: i32) {
        self.hold_pedal = value >= 64;
    }

    pub(crate) fn set_sostenuto_pedal(&mut self, value: i32) {
        self.sostenuto_pedal = value >= 64;
    }

    pub(crate) fn get_sostenuto_pedal(&self) -> bool {
        self.sostenuto_pedal
    }

    pub(crate) fn set_soft_pedal(&mut self, value: i32) {
        self.soft_pedal = value >= 64;
    }

    pub(crate) fn get_soft_pedal(&self) -> bool {
        self.soft_pedal
    }

    pub(crate) fn set_portamento_pedal(&mut self, value: i32) {
        self.portamento_pedal = value >= 64;
    }

    pub(crate) fn get_portamento_pedal(&self) -> bool {
        self.portamento_pedal
    }

    pub(crate) fn set_portamento_time(&mut self, value: i32) {
        self.portamento_time = value as u8;
    }

    pub(crate) fn get_portamento_time(&self) -> u8 {
        self.portamento_time
    }

    pub(crate) fn set_last_key(&mut self, key: i32) {
        self.last_key = Some(key as u8);
    }

    pub(crate) fn get_last_key(&self) -> Option<u8> {
        self.last_key
    }

    pub(crate) fn set_portamento_source(&mut self, key: i32) {
        self.portamento_source = Some(key as u8);
    }

    pub(crate) fn take_portamento_source(&mut self) -> Option<u8> {
        self.portamento_source.take()
    }

    pub(crate) fn set_mono_mode(&mut self, mono: bool) {
        self.mono_mode = mono;
    }

    pub(crate) fn get_mono_mode(&self) -> bool {
        self.mono_mode
    }

    pub(crate) fn set_channel_pressure(&mut self, value: i32) {
        self.channel_pressure = value as u8;
    }

    pub(crate) fn get_channel_pressure(&self) -> u8 {
        self.channel_pressure
    }

    pub(crate) fn set_poly_pressure(&mut self, key: i32, value: i32) {
        if (0..128).contains(&key) {
            self.poly_pressure[key as usize] = value as u8;
        }
    }

    pub(crate) fn get_poly_pressure(&self, key: i32) -> u8 {
        self.poly_pressure.get(key as usize).copied().unwrap_or(0)
    }

    pub(crate) fn set_reverb_send(&mut self, value: i32) {
        self.reverb_send = value as u8;
    }

    pub(crate) fn set_chorus_send(&mut self, value: i32) {
        self.chorus_send = value as u8;
    }

    pub(crate) fn set_rpn_coarse(&mut self, value: i32) {
        self.rpn = (self.rpn & 0x7F) | (value << 7) as i16;
        self.last_data_type = DataType::Rpn;
    }

    pub(crate) fn set_rpn_fine(&mut self, value: i32) {
        self.rpn = (((self.rpn as i32) & 0xFF80) | value) as i16;
        self.last_data_type = DataType::Rpn;
    }

    pub(crate) fn set_nrpn_coarse(&mut self, value: i32) {
        self.nrpn = (self.nrpn & 0x7F) | (value << 7) as i16;
        self.last_data_type = DataType::Nrpn;
    }

    pub(crate) fn set_nrpn_fine(&mut self, value: i32) {
        self.nrpn = (((self.nrpn as i32) & 0xFF80) | value) as i16;
        self.last_data_type = DataType::Nrpn;
    }

    /// The GS NRPN set, routed on the data-entry MSB (the LSB is
    /// ignored, as the unit ignores it). Tone modifies are relative,
    /// -50..+50 about 40H; the drum set is per-note, absolute except
    /// the relative pitch. Program changes and CC121 leave these
    /// standing; only a reset clears them.
    fn nrpn_data_entry(&mut self, value: i32) {
        let msb = (self.nrpn >> 7) & 0x7F;
        let lsb = self.nrpn & 0x7F;
        let relative = (value - 64).clamp(-50, 50);
        match (msb, lsb) {
            (0x01, 0x08) => self.vib_rate_off = relative,
            (0x01, 0x09) => self.vib_depth_off = relative,
            (0x01, 0x0A) => self.vib_delay_off = relative,
            (0x01, 0x20) => self.cutoff_off = relative,
            (0x01, 0x21) => self.resonance_off = relative,
            (0x01, 0x63) => self.eg_attack_off = relative,
            (0x01, 0x64) => self.eg_decay_off = relative,
            (0x01, 0x66) => self.eg_release_off = relative,
            (0x18, key) if self.is_percussion_channel => {
                self.drum_pitch[key as usize] = (value - 64).clamp(-64, 63) as i8;
            }
            (0x1A, key) if self.is_percussion_channel => {
                self.drum_level[key as usize] = Some(value as u8);
            }
            (0x1C, key) if self.is_percussion_channel => {
                self.drum_pan[key as usize] = Some(value as u8);
            }
            (0x1D, key) if self.is_percussion_channel => {
                self.drum_reverb[key as usize] = Some(value as u8);
            }
            (0x1E, key) if self.is_percussion_channel => {
                self.drum_chorus[key as usize] = Some(value as u8);
            }
            _ => {}
        }
    }

    /// A GS NRPN's stored value back in wire terms, for the fascia's
    /// editor (relative parameters read 64 at neutral).
    pub(crate) fn nrpn_wire_value(&self, msb: u8, lsb: u8) -> u8 {
        let rel = |off: i32| (off + 64).clamp(0, 127) as u8;
        match (msb, lsb) {
            (0x01, 0x08) => rel(self.vib_rate_off),
            (0x01, 0x09) => rel(self.vib_depth_off),
            (0x01, 0x0A) => rel(self.vib_delay_off),
            (0x01, 0x20) => rel(self.cutoff_off),
            (0x01, 0x21) => rel(self.resonance_off),
            (0x01, 0x63) => rel(self.eg_attack_off),
            (0x01, 0x64) => rel(self.eg_decay_off),
            (0x01, 0x66) => rel(self.eg_release_off),
            _ => 64,
        }
    }

    /// Vibrato rate as a frequency factor (about +/-4x at the ends).
    pub(crate) fn nrpn_vib_rate_factor(&self) -> f32 {
        (2_f32).powf(self.vib_rate_off as f32 / 25.0)
    }

    /// Extra vibrato depth in cents.
    pub(crate) fn nrpn_vib_depth_cents(&self) -> f32 {
        self.vib_depth_off as f32 * 6.0
    }

    /// Vibrato delay as a duration factor (positive waits longer).
    pub(crate) fn nrpn_vib_delay_factor(&self) -> f32 {
        (2_f32).powf(self.vib_delay_off as f32 / 25.0)
    }

    /// TVF cutoff offset in cents.
    pub(crate) fn nrpn_cutoff_cents(&self) -> f32 {
        self.cutoff_off as f32 * 48.0
    }

    /// TVF resonance offset in centibels.
    pub(crate) fn nrpn_resonance_cb(&self) -> f32 {
        self.resonance_off as f32 * 6.0
    }

    /// Envelope time factors: attack, decay, release (about half to
    /// double across the range).
    pub(crate) fn nrpn_eg_factors(&self) -> (f32, f32, f32) {
        let f = |off: i32| (2_f32).powf(off as f32 / 50.0);
        (
            f(self.eg_attack_off),
            f(self.eg_decay_off),
            f(self.eg_release_off),
        )
    }

    /// A drum note's pitch offset in semitones.
    pub(crate) fn drum_pitch_semitones(&self, key: i32) -> f32 {
        if !self.is_percussion_channel {
            return 0_f32;
        }
        self.drum_pitch
            .get(key as usize)
            .map(|&p| p as f32)
            .unwrap_or(0_f32)
    }

    /// A drum note's TVA level as a gain (GM's squared curve).
    pub(crate) fn drum_level_gain(&self, key: i32) -> f32 {
        if !self.is_percussion_channel {
            return 1_f32;
        }
        match self.drum_level.get(key as usize).copied().flatten() {
            Some(level) => {
                let v = level as f32 / 127.0;
                v * v
            }
            None => 1_f32,
        }
    }

    /// A drum note's pan override in the instrument's -50..+50 units.
    /// 0 on the wire means random placement; a deterministic scatter
    /// derived from the note keeps renders reproducible.
    pub(crate) fn drum_pan_override(&self, key: i32, velocity: i32) -> Option<f32> {
        if !self.is_percussion_channel {
            return None;
        }
        let raw = self.drum_pan.get(key as usize).copied().flatten()?;
        if raw == 0 {
            let hash = (key as u32)
                .wrapping_mul(2654435761)
                .wrapping_add(velocity as u32)
                .wrapping_mul(40503);
            return Some((hash >> 16) as f32 % 101.0 - 50.0);
        }
        Some((raw as f32 - 64.0).clamp(-50.0, 50.0) * (50.0 / 63.0))
    }

    /// The part reverb send scaled by a drum note's multiplicand.
    pub(crate) fn get_reverb_send_for(&self, key: i32) -> f32 {
        let mult = if self.is_percussion_channel {
            self.drum_reverb
                .get(key as usize)
                .copied()
                .flatten()
                .map(|v| v as f32 / 127.0)
                .unwrap_or(1_f32)
        } else {
            1_f32
        };
        self.get_reverb_send() * mult
    }

    /// The part chorus send scaled by a drum note's multiplicand.
    pub(crate) fn get_chorus_send_for(&self, key: i32) -> f32 {
        let mult = if self.is_percussion_channel {
            self.drum_chorus
                .get(key as usize)
                .copied()
                .flatten()
                .map(|v| v as f32 / 127.0)
                .unwrap_or(1_f32)
        } else {
            1_f32
        };
        self.get_chorus_send() * mult
    }

    pub(crate) fn data_entry_coarse(&mut self, value: i32) {
        if self.last_data_type == DataType::Nrpn {
            self.nrpn_data_entry(value);
            return;
        }
        if self.last_data_type != DataType::Rpn {
            return;
        }

        if self.rpn == 0 {
            self.pitch_bend_range = (self.pitch_bend_range & 0x7F) | (value << 7) as i16;
        } else if self.rpn == 1 {
            self.fine_tune = (self.fine_tune & 0x7F) | (value << 7) as i16;
        } else if self.rpn == 2 {
            self.coarse_tune = (value - 64) as i16;
        }
    }

    pub(crate) fn data_entry_fine(&mut self, value: i32) {
        if self.last_data_type != DataType::Rpn {
            return;
        }

        if self.rpn == 0 {
            self.pitch_bend_range = (((self.pitch_bend_range as i32) & 0xFF80) | value) as i16;
        } else if self.rpn == 1 {
            self.fine_tune = (((self.fine_tune as i32) & 0xFF80) | value) as i16;
        }
    }

    pub(crate) fn set_pitch_bend(&mut self, value1: i32, value2: i32) {
        self.pitch_bend = (1_f32 / 8192_f32) * ((value1 | (value2 << 7)) - 8192) as f32;
    }

    pub(crate) fn get_bank_number(&self) -> i32 {
        self.bank_number
    }

    pub(crate) fn get_patch_number(&self) -> i32 {
        self.patch_number
    }

    /// Record a controller's raw value for the modulator sources. Called
    /// for every incoming CC, named or not, before the switch that feeds
    /// the named fields.
    pub(crate) fn set_cc(&mut self, controller: i32, value: i32) {
        if (0..128).contains(&controller) {
            self.cc[controller as usize] = value as u8;
        }
    }

    pub(crate) fn get_cc(&self, controller: u8) -> u8 {
        self.cc[(controller & 0x7F) as usize]
    }

    /// The wheel's own position as 0..1 with 0.5 centred, for the
    /// modulator sources -- [`get_pitch_bend`] bakes the range in, which
    /// is the synthesizer's business, not a source's. `pitch_bend` is
    /// kept as -1..1.
    pub(crate) fn get_pitch_bend_raw(&self) -> f32 {
        0.5_f32 * self.pitch_bend + 0.5_f32
    }

    pub(crate) fn get_modulation(&self) -> f32 {
        // The wheel's full travel spans 50 cents at the GS factory
        // depth of 10, and Mod Depth rescales that reach -- capped
        // where a vibrato becomes a siren.
        let reach = (50_f32 * self.mod_depth as f32 / 10.0).min(600.0);
        (reach / 16383_f32) * self.modulation as f32
    }

    pub(crate) fn get_volume(&self) -> f32 {
        (1_f32 / 16383_f32) * self.volume as f32
    }

    pub(crate) fn get_pan(&self) -> f32 {
        (100_f32 / 16383_f32) * self.pan as f32 - 50_f32
    }

    pub(crate) fn get_expression(&self) -> f32 {
        (1_f32 / 16383_f32) * self.expression as f32
    }

    pub(crate) fn get_hold_pedal(&self) -> bool {
        self.hold_pedal
    }

    pub(crate) fn get_reverb_send(&self) -> f32 {
        // The send eases in and tops out shy of drenched: tuned by ear
        // over several passes, the curve steepening each time so the
        // shipping default of 40 sits further back while full send
        // stays where it was. A room should sit behind the band, not
        // swallow it.
        0.742 * ((1_f32 / 127_f32) * self.reverb_send as f32).powf(1.832)
    }

    pub(crate) fn get_chorus_send(&self) -> f32 {
        (1_f32 / 127_f32) * self.chorus_send as f32
    }

    pub(crate) fn get_pitch_bend_range(&self) -> f32 {
        (self.pitch_bend_range >> 7) as f32 + 0.01_f32 * (self.pitch_bend_range & 0x7F) as f32
    }

    pub(crate) fn get_tune(&self) -> f32 {
        self.master_tune
            + self.coarse_tune as f32
            + (1_f32 / 8192_f32) * (self.fine_tune - 8192) as f32
    }

    pub(crate) fn set_master_tune(&mut self, semitones: f32) {
        self.master_tune = semitones;
    }

    pub(crate) fn bend_range_semitones(&self) -> i8 {
        (self.pitch_bend_range >> 7) as i8
    }

    pub(crate) fn set_bend_range_semitones(&mut self, semitones: i8) {
        self.pitch_bend_range = (semitones as i16) << 7;
    }

    /// Fine tune as the panel's -12..=+12: the RPN's semitone either
    /// way, in twelve steps.
    pub(crate) fn fine_tune_display(&self) -> i8 {
        let twelfths = (self.fine_tune as i32 - 8192) * 12;
        let rounding = if twelfths >= 0 { 4096 } else { -4096 };
        ((twelfths + rounding) / 8192) as i8
    }

    pub(crate) fn set_fine_tune_display(&mut self, value: i8) {
        self.fine_tune = (8192 + (value as i32) * 8192 / 12).clamp(0, 16383) as i16;
    }

    pub(crate) fn mono(&self) -> bool {
        self.mono_mode
    }

    pub(crate) fn set_percussion(&mut self, on: bool) {
        self.is_percussion_channel = on;
        self.pending_bank = None;
        self.bank_number = if on { 128 } else { 0 };
        self.patch_number = 0;
    }

    pub(crate) fn key_range(&self) -> (u8, u8) {
        (self.key_range_lo, self.key_range_hi)
    }

    pub(crate) fn set_key_range(&mut self, lo: u8, hi: u8) {
        self.key_range_lo = lo.min(127);
        self.key_range_hi = hi.min(127);
    }

    pub(crate) fn velo_sens(&self) -> (u8, u8) {
        (self.velo_depth, self.velo_offset)
    }

    pub(crate) fn set_velo_sens(&mut self, depth: u8, offset: u8) {
        self.velo_depth = depth.min(127);
        self.velo_offset = offset.min(127);
    }

    /// The received velocity through the part's sensitivity curve:
    /// depth tilts the slope about the centre, offset lifts or sinks
    /// the floor, 64/64 passing the wire through untouched.
    pub(crate) fn shaped_velocity(&self, velocity: i32) -> i32 {
        let shaped =
            velocity as f32 * self.velo_depth as f32 / 64.0 + (self.velo_offset as f32 - 64.0);
        (shaped as i32).clamp(1, 127)
    }

    pub(crate) fn mod_depth(&self) -> u8 {
        self.mod_depth
    }

    pub(crate) fn set_mod_depth(&mut self, depth: u8) {
        self.mod_depth = depth.min(127);
    }

    pub(crate) fn get_pitch_bend(&self) -> f32 {
        self.get_pitch_bend_range() * self.pitch_bend
    }
}
