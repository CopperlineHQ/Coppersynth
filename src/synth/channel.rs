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
    pitch_bend_range: i16,
    coarse_tune: i16,
    fine_tune: i16,

    pitch_bend: f32,

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
            portamento_source: None,
            mono_mode: false,
            channel_pressure: 0,
            poly_pressure: [0; 128],
            pending_bank: None,
            reverb_send: 0,
            chorus_send: 0,
            rpn: 0,
            pitch_bend_range: 0,
            coarse_tune: 0,
            fine_tune: 0,
            pitch_bend: 0_f32,
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
        self.portamento_source = None;
        self.mono_mode = false;
        self.channel_pressure = 0;
        self.poly_pressure = [0; 128];
        self.pending_bank = None;

        self.reverb_send = 40;
        self.chorus_send = 0;

        self.rpn = -1;
        self.pitch_bend_range = 2 << 7;
        self.coarse_tune = 0;
        self.fine_tune = 8192;

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

    pub(crate) fn set_nrpn_coarse(&mut self, _value: i32) {
        self.last_data_type = DataType::Nrpn;
    }

    pub(crate) fn set_nrpn_fine(&mut self, _value: i32) {
        self.last_data_type = DataType::Nrpn;
    }

    pub(crate) fn data_entry_coarse(&mut self, value: i32) {
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
        (50_f32 / 16383_f32) * self.modulation as f32
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
        (1_f32 / 127_f32) * self.reverb_send as f32
    }

    pub(crate) fn get_chorus_send(&self) -> f32 {
        (1_f32 / 127_f32) * self.chorus_send as f32
    }

    pub(crate) fn get_pitch_bend_range(&self) -> f32 {
        (self.pitch_bend_range >> 7) as f32 + 0.01_f32 * (self.pitch_bend_range & 0x7F) as f32
    }

    pub(crate) fn get_tune(&self) -> f32 {
        self.coarse_tune as f32 + (1_f32 / 8192_f32) * (self.fine_tune - 8192) as f32
    }

    pub(crate) fn get_pitch_bend(&self) -> f32 {
        self.get_pitch_bend_range() * self.pitch_bend
    }
}
