#![allow(dead_code)]

use std::cmp;
use std::collections::HashMap;
use std::sync::Arc;

use crate::synth::array_math::ArrayMath;
use crate::synth::channel::Channel;
use crate::synth::chorus::{Chorus, ChorusType};
use crate::synth::error::SynthesizerError;
use crate::synth::region_pair::RegionPair;
use crate::synth::reverb::Reverb;
use crate::synth::soundfont::SoundFont;
use crate::synth::soundfont_math::SoundFontMath;
use crate::synth::synthesizer_settings::SynthesizerSettings;
use crate::synth::voice_collection::VoiceCollection;

/// An instance of the SoundFont synthesizer.
#[derive(Debug)]
#[non_exhaustive]
pub struct Synthesizer {
    pub(crate) sound_font: Arc<SoundFont>,
    pub(crate) sample_rate: i32,
    pub(crate) block_size: usize,
    pub(crate) maximum_polyphony: usize,

    preset_lookup: HashMap<i32, usize>,
    default_preset: usize,

    channels: Vec<Channel>,

    voices: VoiceCollection,

    block_left: Vec<f32>,
    block_right: Vec<f32>,

    inverse_block_size: f32,

    block_read: usize,

    master_volume: f32,
    /// The unit's REVERB/CHORUS "of ALL" levels: gains on the effect
    /// returns, 1.0 at the factory setting.
    master_reverb_gain: f32,
    master_chorus_gain: f32,
    chorus_type: ChorusType,
    /// The reverb character 0-7, Room 1 to Panning Delay.
    reverb_type: u8,

    effects: Option<Effects>,
}

impl Synthesizer {
    /// The number of channels.
    pub const CHANNEL_COUNT: usize = 16;
    /// The floor for effect feeds; see [`Synthesizer::write_block_above`].
    const EFFECT_NON_AUDIBLE: f32 = 1.0e-5_f32;
    /// The reverb return, calibrated so a full send is unmistakably a
    /// hall: banks' CC91 modulators top out near 0.4 of the spec's send
    /// range, and Freeverb's 0.015 input gain leaves that a whisper.
    const REVERB_RETURN: f32 = 4.0_f32;
    /// The chorus return, for the same reason: the unit modulates
    /// correctly but sat too quiet in the mix to be heard as more than
    /// a faint comb.
    const CHORUS_RETURN: f32 = 2.0_f32;
    /// The percussion channel.
    pub const PERCUSSION_CHANNEL: usize = 9;

    /// Initializes a new synthesizer using a specified SoundFont and settings.
    ///
    /// # Arguments
    ///
    /// * `sound_font` - The SoundFont instance.
    /// * `settings` - The settings for synthesis.
    pub fn new(
        sound_font: &Arc<SoundFont>,
        settings: &SynthesizerSettings,
    ) -> Result<Self, SynthesizerError> {
        settings.validate()?;

        let mut preset_lookup: HashMap<i32, usize> = HashMap::new();

        let mut min_preset_id = i32::MAX;
        let mut default_preset: usize = 0;
        for i in 0..sound_font.presets.len() {
            let preset = &sound_font.presets[i];

            // The preset ID is Int32, where the upper 16 bits represent the bank number
            // and the lower 16 bits represent the patch number.
            // This ID is used to search for presets by the combination of bank number
            // and patch number.
            let preset_id = (preset.bank_number << 16) | preset.patch_number;
            preset_lookup.insert(preset_id, i);

            // The preset with the minimum ID number will be default.
            // If the SoundFont is GM compatible, the piano will be chosen.
            if preset_id < min_preset_id {
                default_preset = i;
                min_preset_id = preset_id;
            }
        }

        let mut channels: Vec<Channel> = Vec::new();
        for i in 0..Synthesizer::CHANNEL_COUNT {
            channels.push(Channel::new(i == Synthesizer::PERCUSSION_CHANNEL));
        }

        let voices = VoiceCollection::new(settings);

        let block_left: Vec<f32> = vec![0_f32; settings.block_size];
        let block_right: Vec<f32> = vec![0_f32; settings.block_size];

        let inverse_block_size = 1_f32 / settings.block_size as f32;

        let block_read = settings.block_size;

        let master_volume = 0.5_f32;

        let effects = if settings.enable_reverb_and_chorus {
            Some(Effects::new(settings))
        } else {
            None
        };

        Ok(Self {
            sound_font: Arc::clone(sound_font),
            sample_rate: settings.sample_rate,
            block_size: settings.block_size,
            maximum_polyphony: settings.maximum_polyphony,
            preset_lookup,
            default_preset,
            channels,
            voices,
            block_left,
            block_right,
            inverse_block_size,
            block_read,
            master_volume,
            master_reverb_gain: 1.0_f32,
            master_chorus_gain: 1.0_f32,
            chorus_type: ChorusType::Chorus3,
            reverb_type: 4,
            effects,
        })
    }

    /// Processes a MIDI message.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel to which the message will be sent.
    /// * `command` - The type of the message.
    /// * `data1` - The first data part of the message.
    /// * `data2` - The second data part of the message.
    pub fn process_midi_message(&mut self, channel: i32, command: i32, data1: i32, data2: i32) {
        if !(0 <= channel && channel < self.channels.len() as i32) {
            return;
        }

        let channel_info = &mut self.channels[channel as usize];

        match command {
            0x80 => self.note_off(channel, data1),       // Note Off
            0x90 => self.note_on(channel, data1, data2), // Note On
            0xB0 => {
                // The raw table first: modulators may route from any CC,
                // named or not.
                channel_info.set_cc(data1, data2);
                match data1 // Controller
                {
                0x00 => channel_info.set_bank(data2), // Bank Selection
                0x01 => channel_info.set_modulation_coarse(data2), // Modulation Coarse
                0x21 => channel_info.set_modulation_fine(data2), // Modulation Fine
                0x06 => channel_info.data_entry_coarse(data2), // Data Entry Coarse
                0x26 => channel_info.data_entry_fine(data2), // Data Entry Fine
                0x07 => channel_info.set_volume_coarse(data2), // Channel Volume Coarse
                0x27 => channel_info.set_volume_fine(data2), // Channel Volume Fine
                0x0A => channel_info.set_pan_coarse(data2), // Pan Coarse
                0x2A => channel_info.set_pan_fine(data2), // Pan Fine
                0x0B => channel_info.set_expression_coarse(data2), // Expression Coarse
                0x2B => channel_info.set_expression_fine(data2), // Expression Fine
                0x05 => channel_info.set_portamento_time(data2), // Portamento Time
                0x40 => channel_info.set_hold_pedal(data2), // Hold Pedal
                0x41 => channel_info.set_portamento_pedal(data2), // Portamento
                0x42 => { // Sostenuto
                    channel_info.set_sostenuto_pedal(data2);
                    self.apply_sostenuto(channel, data2 >= 64);
                }
                0x43 => channel_info.set_soft_pedal(data2), // Soft Pedal
                0x54 => channel_info.set_portamento_source(data2), // Portamento Control
                0x5B => channel_info.set_reverb_send(data2), // Reverb Send
                0x5D => channel_info.set_chorus_send(data2), // Chorus Send
                0x63 => channel_info.set_nrpn_coarse(data2), // NRPN Coarse
                0x62 => channel_info.set_nrpn_fine(data2), // NRPN Fine
                0x65 => channel_info.set_rpn_coarse(data2), // RPN Coarse
                0x64 => channel_info.set_rpn_fine(data2), // RPN Fine
                0x78 => self.note_off_all_channel(channel, true), // All Sound Off
                0x79 => self.reset_all_controllers_channel(channel), // Reset All Controllers
                0x7B => self.note_off_all_channel(channel, false), // All Note Off
                // Omni off/on are recognized only as all-notes-off; the
                // mode does not change, exactly as the unit reads them.
                0x7C | 0x7D => self.note_off_all_channel(channel, false),
                0x7E => { // Mono (M = 1 whatever the count asks)
                    channel_info.set_mono_mode(true);
                    self.note_off_all_channel(channel, true);
                }
                0x7F => { // Poly
                    channel_info.set_mono_mode(false);
                    self.note_off_all_channel(channel, true);
                }
                _ => (),
                }
            }
            // Both pressures are received and kept for bank modulators to
            // route; the unit itself sends them nowhere by default.
            0xA0 => channel_info.set_poly_pressure(data1, data2), // Poly Pressure
            0xC0 => channel_info.set_patch(data1),                // Program Change
            0xD0 => channel_info.set_channel_pressure(data1),     // Channel Pressure
            0xE0 => channel_info.set_pitch_bend(data1, data2),    // Pitch Bend
            _ => (),
        }
    }

    /// The per-block decay of a portamento glide at the channel's CC5
    /// time. 0 is instant, as the unit reads it; upward the glide
    /// closes on a time constant that grows to a couple of seconds --
    /// an approximation of the hardware's curve.
    fn portamento_decay(&self, time: u8) -> f32 {
        if time == 0 {
            return 0_f32;
        }
        let t = time as f32 / 127.0;
        let tau = 0.005 + 2.5 * t * t;
        (-(self.block_size as f32) / (self.sample_rate as f32 * tau)).exp()
    }

    /// The sostenuto pedal going down catches the notes sounding at
    /// that moment; coming up it lets go of them all. Notes played
    /// while it is down are never caught.
    fn apply_sostenuto(&mut self, channel: i32, down: bool) {
        for voice in self.voices.get_active_voices().iter_mut() {
            if voice.channel() == channel {
                voice.set_sostenuto_held(down);
            }
        }
    }

    /// Stops a note.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel of the note.
    /// * `key` - The key of the note.
    pub fn note_off(&mut self, channel: i32, key: i32) {
        if !(0 <= channel && channel < self.channels.len() as i32) {
            return;
        }

        for voice in self.voices.get_active_voices().iter_mut() {
            if voice.channel() == channel && voice.key() == key {
                voice.end();
            }
        }
    }

    /// Starts a note.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel of the note.
    /// * `key` - The key of the note.
    /// * `velocity` - The velocity of the note.
    pub fn note_on(&mut self, channel: i32, key: i32, velocity: i32) {
        if velocity == 0 {
            self.note_off(channel, key);
            return;
        }

        if !(0 <= channel && channel < self.channels.len() as i32) {
            return;
        }

        // The part's playable window and velocity curve stand at the
        // front door: a note outside Key Range L/H never sounds, and
        // one inside arrives at the shaped velocity.
        let (lo, hi) = self.channels[channel as usize].key_range();
        if !(lo as i32..=hi as i32).contains(&key) {
            return;
        }
        let velocity = self.channels[channel as usize].shaped_velocity(velocity);

        // Portamento first: a Portamento Control source armed ahead of
        // this note re-tunes a sounding voice of that key in place --
        // legato, no new voice, exactly the manual's example. Armed
        // with no such voice, or with the pedal down, the new voice
        // glides in from the source instead.
        let decay = self.portamento_decay(self.channels[channel as usize].get_portamento_time());
        let mut glide_source: Option<i32> = None;
        if let Some(src) = self.channels[channel as usize].take_portamento_source() {
            let mut retuned = false;
            for voice in self.voices.get_active_voices().iter_mut() {
                if voice.channel() == channel && voice.key() == src as i32 && voice.is_sounding() {
                    voice.retune_to(key, decay);
                    retuned = true;
                    break;
                }
            }
            if retuned {
                self.channels[channel as usize].set_last_key(key);
                return;
            }
            glide_source = Some(src as i32);
        } else if self.channels[channel as usize].get_portamento_pedal() {
            glide_source = self.channels[channel as usize]
                .get_last_key()
                .map(i32::from);
        }

        // Mode 4: one voice at a time -- the note that arrives releases
        // whatever the channel was sounding.
        if self.channels[channel as usize].get_mono_mode() {
            for voice in self.voices.get_active_voices().iter_mut() {
                if voice.channel() == channel {
                    voice.end();
                }
            }
        }

        let channel_info = &self.channels[channel as usize];

        let preset_id = (channel_info.get_bank_number() << 16) | channel_info.get_patch_number();

        let mut preset = self.default_preset;
        match self.preset_lookup.get(&preset_id) {
            Some(value) => preset = *value,
            None => {
                // Try fallback to the GM sound set.
                // Normally, the given patch number + the bank number 0 will work.
                // For drums (bank number >= 128), it seems to be better to select the standard set (128:0).
                let gm_preset_id = if channel_info.get_bank_number() < 128 {
                    channel_info.get_patch_number()
                } else {
                    128 << 16
                };

                // If no corresponding preset was found. Use the default one...
                if let Some(value) = self.preset_lookup.get(&gm_preset_id) {
                    preset = *value
                }
            }
        }

        let preset = &self.sound_font.presets[preset];
        for preset_region in preset.regions.iter() {
            if preset_region.contains(key, velocity) {
                let instrument = &self.sound_font.instruments[preset_region.instrument];
                for instrument_region in instrument.regions.iter() {
                    if instrument_region.contains(key, velocity) {
                        let region_pair = RegionPair::new(preset_region, instrument_region);

                        if let Some(value) = self.voices.request_new(instrument_region, channel) {
                            value.start(
                                &region_pair,
                                &self.channels[channel as usize],
                                channel,
                                key,
                                velocity,
                            );
                            if let Some(src) = glide_source {
                                value.glide_from(src, decay);
                            }
                        }
                    }
                }
            }
        }
        self.channels[channel as usize].set_last_key(key);
    }

    /// Stops all the notes in the specified channel.
    ///
    /// # Arguments
    ///
    /// * `immediate` - If `true`, notes will stop immediately without the release sound.
    pub fn note_off_all(&mut self, immediate: bool) {
        if immediate {
            self.voices.clear();
        } else {
            for voice in self.voices.get_active_voices().iter_mut() {
                voice.end();
            }
        }
    }

    /// Stops all the notes in the specified channel.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel in which the notes will be stopped.
    /// * `immediate` - If `true`, notes will stop immediately without the release sound.
    pub fn note_off_all_channel(&mut self, channel: i32, immediate: bool) {
        if immediate {
            for voice in self.voices.get_active_voices().iter_mut() {
                if voice.channel() == channel {
                    voice.kill();
                }
            }
        } else {
            for voice in self.voices.get_active_voices().iter_mut() {
                if voice.channel() == channel {
                    voice.end();
                }
            }
        }
    }

    /// Resets all the controllers.
    pub fn reset_all_controllers(&mut self) {
        for channel in &mut self.channels {
            channel.reset_all_controllers();
        }
    }

    /// Resets all the controllers of the specified channel.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel to be reset.
    pub fn reset_all_controllers_channel(&mut self, channel: i32) {
        if !(0 <= channel && channel < self.channels.len() as i32) {
            return;
        }

        self.channels[channel as usize].reset_all_controllers();
    }

    /// Resets the synthesizer.
    pub fn reset(&mut self) {
        self.voices.clear();

        for channel in &mut self.channels {
            channel.reset();
        }

        if let Some(effects) = self.effects.as_mut() {
            effects.reverb.mute();
            effects.chorus.mute();
        }

        self.block_read = self.block_size;
    }

    /// Renders the waveform.
    ///
    /// # Arguments
    ///
    /// * `left` - The buffer of the left channel to store the rendered waveform.
    /// * `right` - The buffer of the right channel to store the rendered waveform.
    ///
    /// # Remarks
    ///
    /// The output buffers for the left and right must be the same length.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        if left.len() != right.len() {
            panic!("The output buffers for the left and right must be the same length.");
        }

        let left_length = left.len();

        let mut wrote = 0;
        while wrote < left_length {
            if self.block_read == self.block_size {
                self.render_block();
                self.block_read = 0;
            }

            let src_rem = self.block_size - self.block_read;
            let dst_rem = left_length - wrote;
            let rem = cmp::min(src_rem, dst_rem);

            for t in 0..rem {
                left[wrote + t] = self.block_left[self.block_read + t];
                right[wrote + t] = self.block_right[self.block_read + t];
            }

            self.block_read += rem;
            wrote += rem;
        }
    }

    fn render_block(&mut self) {
        self.voices
            .process(&self.sound_font.wave_data, &self.channels);

        self.block_left.fill(0_f32);
        self.block_right.fill(0_f32);
        for voice in self.voices.get_active_voices().iter_mut() {
            let previous_gain_left = self.master_volume * voice.previous_mix_gain_left;
            let current_gain_left = self.master_volume * voice.current_mix_gain_left;
            Synthesizer::write_block(
                previous_gain_left,
                current_gain_left,
                voice.block(),
                &mut self.block_left[..],
                self.inverse_block_size,
            );
            let previous_gain_right = self.master_volume * voice.previous_mix_gain_right;
            let current_gain_right = self.master_volume * voice.current_mix_gain_right;
            Synthesizer::write_block(
                previous_gain_right,
                current_gain_right,
                voice.block(),
                &mut self.block_right[..],
                self.inverse_block_size,
            );
        }

        if let Some(effects) = self.effects.as_mut() {
            let chorus = &mut effects.chorus;
            let chorus_input_left = &mut effects.chorus_input_left[..];
            let chorus_input_right = &mut effects.chorus_input_right[..];
            let chorus_output_left = &mut effects.chorus_output_left[..];
            let chorus_output_right = &mut effects.chorus_output_right[..];
            chorus_input_left.fill(0_f32);
            chorus_input_right.fill(0_f32);
            for voice in self.voices.get_active_voices().iter_mut() {
                let previous_gain_left = voice.previous_chorus_send * voice.previous_mix_gain_left;
                let current_gain_left = voice.current_chorus_send * voice.current_mix_gain_left;
                Synthesizer::write_block_above(
                    previous_gain_left,
                    current_gain_left,
                    voice.block(),
                    chorus_input_left,
                    self.inverse_block_size,
                    Synthesizer::EFFECT_NON_AUDIBLE,
                );
                let previous_gain_right =
                    voice.previous_chorus_send * voice.previous_mix_gain_right;
                let current_gain_right = voice.current_chorus_send * voice.current_mix_gain_right;
                Synthesizer::write_block_above(
                    previous_gain_right,
                    current_gain_right,
                    voice.block(),
                    chorus_input_right,
                    self.inverse_block_size,
                    Synthesizer::EFFECT_NON_AUDIBLE,
                );
            }
            chorus.process(
                chorus_input_left,
                chorus_input_right,
                chorus_output_left,
                chorus_output_right,
            );
            let chorus_gain =
                self.master_volume * self.master_chorus_gain * Synthesizer::CHORUS_RETURN;
            ArrayMath::multiply_add(chorus_gain, chorus_output_left, &mut self.block_left[..]);
            ArrayMath::multiply_add(chorus_gain, chorus_output_right, &mut self.block_right[..]);

            let reverb = &mut effects.reverb;
            let reverb_input = &mut effects.reverb_input[..];
            let reverb_output_left = &mut effects.reverb_output_left[..];
            let reverb_output_right = &mut effects.reverb_output_right[..];
            reverb_input.fill(0_f32);
            for voice in self.voices.get_active_voices().iter_mut() {
                let previous_gain = reverb.get_input_gain()
                    * voice.previous_reverb_send
                    * (voice.previous_mix_gain_left + voice.previous_mix_gain_right);
                let current_gain = reverb.get_input_gain()
                    * voice.current_reverb_send
                    * (voice.current_mix_gain_left + voice.current_mix_gain_right);
                Synthesizer::write_block_above(
                    previous_gain,
                    current_gain,
                    voice.block(),
                    &mut reverb_input[..],
                    self.inverse_block_size,
                    Synthesizer::EFFECT_NON_AUDIBLE,
                );
            }

            reverb.process(reverb_input, reverb_output_left, reverb_output_right);
            let reverb_gain = self.master_volume
                * self.master_reverb_gain
                * Synthesizer::REVERB_RETURN
                * reverb.get_output_gain();
            ArrayMath::multiply_add(reverb_gain, reverb_output_left, &mut self.block_left[..]);
            ArrayMath::multiply_add(reverb_gain, reverb_output_right, &mut self.block_right[..]);
        }
    }

    fn write_block(
        previous_gain: f32,
        current_gain: f32,
        source: &[f32],
        destination: &mut [f32],
        inverse_block_size: f32,
    ) {
        Synthesizer::write_block_above(
            previous_gain,
            current_gain,
            source,
            destination,
            inverse_block_size,
            SoundFontMath::NON_AUDIBLE,
        );
    }

    /// The audibility floor is the caller's: the dry mix skips anything
    /// under `NON_AUDIBLE`, but an effect feed is pre-amplification --
    /// the reverb's fixed input gain alone is 0.015, so a feed the dry
    /// floor calls silent still rings the combs audibly.
    fn write_block_above(
        previous_gain: f32,
        current_gain: f32,
        source: &[f32],
        destination: &mut [f32],
        inverse_block_size: f32,
        floor: f32,
    ) {
        if SoundFontMath::max(previous_gain, current_gain) < floor {
            return;
        }

        if (current_gain - previous_gain).abs() < 1.0E-3_f32 {
            ArrayMath::multiply_add(current_gain, source, destination);
        } else {
            let step = inverse_block_size * (current_gain - previous_gain);
            ArrayMath::multiply_add_slope(previous_gain, step, source, destination);
        }
    }

    /// Gets the SoundFont used as the audio source.
    pub fn get_sound_font(&self) -> &SoundFont {
        &self.sound_font
    }

    /// Gets the sample rate for synthesis.
    pub fn get_sample_rate(&self) -> i32 {
        self.sample_rate
    }

    /// Gets the block size for rendering waveform.
    pub fn get_block_size(&self) -> usize {
        self.block_size
    }

    /// Gets the number of maximum polyphony.
    pub fn get_maximum_polyphony(&self) -> usize {
        self.maximum_polyphony
    }

    /// Gets the value indicating whether reverb and chorus are enabled.
    pub fn get_enable_reverb_and_chorus(&self) -> bool {
        self.effects.is_some()
    }

    /// Gets the bank and patch numbers channel `channel` last selected.
    pub fn channel_bank_patch(&self, channel: usize) -> Option<(i32, i32)> {
        let channel_info = self.channels.get(channel)?;
        Some((
            channel_info.get_bank_number(),
            channel_info.get_patch_number(),
        ))
    }

    /// Gets the raw value of controller `controller` on `channel`.
    pub fn channel_cc(&self, channel: usize, controller: u8) -> Option<u8> {
        Some(self.channels.get(channel)?.get_cc(controller))
    }

    /// Gets the peak amplitude of the voices sounding on each channel:
    /// the largest per-voice mix gain, which already carries the volume
    /// envelope, the note's velocity and the channel's own levels.
    pub fn channel_activity(&self) -> [f32; Synthesizer::CHANNEL_COUNT] {
        let mut peak = [0_f32; Synthesizer::CHANNEL_COUNT];
        for voice in self.voices.active_voices() {
            let gain = voice
                .current_mix_gain_left
                .max(voice.current_mix_gain_right);
            let channel = voice.channel() as usize;
            if let Some(entry) = peak.get_mut(channel) {
                *entry = entry.max(gain);
            }
        }
        peak
    }

    /// Gets the number of voices currently sounding.
    pub fn active_voice_count(&self) -> usize {
        self.voices.active_voices().len()
    }

    /// Gets the master volume.
    pub fn get_master_volume(&self) -> f32 {
        self.master_volume
    }

    /// Sets the master volume.
    ///
    /// # Arguments
    ///
    /// * `value` - The new value of the master volume.
    pub fn set_master_volume(&mut self, value: f32) {
        self.master_volume = value;
    }

    /// Gets the gain applied to the reverb return.
    pub fn get_master_reverb_gain(&self) -> f32 {
        self.master_reverb_gain
    }

    /// Sets the gain applied to the reverb return.
    pub fn set_master_reverb_gain(&mut self, value: f32) {
        self.master_reverb_gain = value;
    }

    /// Gets the gain applied to the chorus return.
    pub fn get_master_chorus_gain(&self) -> f32 {
        self.master_chorus_gain
    }

    /// A channel's GS NRPN value in wire terms (64 = neutral for the
    /// relative parameters).
    pub fn channel_nrpn_wire(&self, channel: i32, msb: u8, lsb: u8) -> u8 {
        self.channels
            .get(channel as usize)
            .map(|c| c.nrpn_wire_value(msb, lsb))
            .unwrap_or(64)
    }

    /// The chorus character in force.
    pub fn chorus_type(&self) -> ChorusType {
        self.chorus_type
    }

    /// Swap the chorus for another character. The unit is rebuilt
    /// silent (its delay line starts empty), which is what the
    /// hardware's macro switch does too.
    pub fn set_chorus_type(&mut self, chorus_type: ChorusType) {
        if chorus_type == self.chorus_type {
            return;
        }
        self.chorus_type = chorus_type;
        if let Some(effects) = self.effects.as_mut() {
            effects.chorus = Chorus::of_type(self.sample_rate, chorus_type);
        }
    }

    /// Sets the gain applied to the chorus return.
    pub fn set_master_chorus_gain(&mut self, value: f32) {
        self.master_chorus_gain = value;
    }

    /// The reverb character in force, 0-7.
    pub fn reverb_type(&self) -> u8 {
        self.reverb_type
    }

    /// Swap the reverb for another character. The rack is rebuilt
    /// silent, exactly as the chorus's macro switch is.
    pub fn set_reverb_type(&mut self, reverb_type: u8) {
        let reverb_type = reverb_type.min(7);
        if reverb_type == self.reverb_type {
            return;
        }
        self.reverb_type = reverb_type;
        if let Some(effects) = self.effects.as_mut() {
            effects.reverb = Reverb::of_type(self.sample_rate, reverb_type);
        }
    }

    /// The unit's master tune, applied to every channel in one go.
    pub fn set_master_tune(&mut self, semitones: f32) {
        for channel in self.channels.iter_mut() {
            channel.set_master_tune(semitones);
        }
    }

    /// Whether `channel` is a drum part.
    pub fn is_percussion(&self, channel: i32) -> bool {
        self.channels
            .get(channel as usize)
            .is_some_and(|c| c.is_percussion_channel)
    }

    /// Make `channel` a drum part or a normal one. The change stops its
    /// notes and puts the part on the mode's first instrument, exactly
    /// as the unit's Part Mode switch does.
    pub fn set_percussion(&mut self, channel: i32, on: bool) {
        if !(0 <= channel && channel < self.channels.len() as i32) {
            return;
        }
        if self.channels[channel as usize].is_percussion_channel == on {
            return;
        }
        self.note_off_all_channel(channel, true);
        self.channels[channel as usize].set_percussion(on);
    }

    /// The part's playable window.
    pub fn channel_key_range(&self, channel: i32) -> (u8, u8) {
        self.channels
            .get(channel as usize)
            .map(|c| c.key_range())
            .unwrap_or((0, 127))
    }

    pub fn set_channel_key_range(&mut self, channel: i32, lo: u8, hi: u8) {
        if let Some(c) = self.channels.get_mut(channel as usize) {
            c.set_key_range(lo, hi);
        }
    }

    /// The part's velocity curve (depth, offset).
    pub fn channel_velo_sens(&self, channel: i32) -> (u8, u8) {
        self.channels
            .get(channel as usize)
            .map(|c| c.velo_sens())
            .unwrap_or((64, 64))
    }

    pub fn set_channel_velo_sens(&mut self, channel: i32, depth: u8, offset: u8) {
        if let Some(c) = self.channels.get_mut(channel as usize) {
            c.set_velo_sens(depth, offset);
        }
    }

    /// The part's bend range in semitones, signed.
    pub fn channel_bend_range(&self, channel: i32) -> i8 {
        self.channels
            .get(channel as usize)
            .map(|c| c.bend_range_semitones())
            .unwrap_or(2)
    }

    pub fn set_channel_bend_range(&mut self, channel: i32, semitones: i8) {
        if let Some(c) = self.channels.get_mut(channel as usize) {
            c.set_bend_range_semitones(semitones);
        }
    }

    /// The part's fine tune, the panel's -12..=+12.
    pub fn channel_fine_tune_display(&self, channel: i32) -> i8 {
        self.channels
            .get(channel as usize)
            .map(|c| c.fine_tune_display())
            .unwrap_or(0)
    }

    pub fn set_channel_fine_tune_display(&mut self, channel: i32, value: i8) {
        if let Some(c) = self.channels.get_mut(channel as usize) {
            c.set_fine_tune_display(value);
        }
    }

    /// Whether the part plays one note at a time.
    pub fn channel_mono(&self, channel: i32) -> bool {
        self.channels
            .get(channel as usize)
            .is_some_and(|c| c.mono())
    }

    pub fn set_channel_mono(&mut self, channel: i32, mono: bool) {
        if let Some(c) = self.channels.get_mut(channel as usize) {
            c.set_mono_mode(mono);
        }
    }

    /// The part's modulation depth.
    pub fn channel_mod_depth(&self, channel: i32) -> u8 {
        self.channels
            .get(channel as usize)
            .map(|c| c.mod_depth())
            .unwrap_or(10)
    }

    pub fn set_channel_mod_depth(&mut self, channel: i32, depth: u8) {
        if let Some(c) = self.channels.get_mut(channel as usize) {
            c.set_mod_depth(depth);
        }
    }
}

#[derive(Debug)]
struct Effects {
    reverb: Reverb,
    reverb_input: Vec<f32>,
    reverb_output_left: Vec<f32>,
    reverb_output_right: Vec<f32>,

    chorus: Chorus,
    chorus_input_left: Vec<f32>,
    chorus_input_right: Vec<f32>,
    chorus_output_left: Vec<f32>,
    chorus_output_right: Vec<f32>,
}

impl Effects {
    fn new(settings: &SynthesizerSettings) -> Effects {
        Self {
            reverb: Reverb::of_type(settings.sample_rate, 4),
            reverb_input: vec![0_f32; settings.block_size],
            reverb_output_left: vec![0_f32; settings.block_size],
            reverb_output_right: vec![0_f32; settings.block_size],
            // The unit wakes in Chorus 2 -- the hardware's own default
            // is Chorus 3, but 2's quicker sweep carries further.
            chorus: Chorus::of_type(settings.sample_rate, ChorusType::Chorus3),
            chorus_input_left: vec![0_f32; settings.block_size],
            chorus_input_right: vec![0_f32; settings.block_size],
            chorus_output_left: vec![0_f32; settings.block_size],
            chorus_output_right: vec![0_f32; settings.block_size],
        }
    }
}
