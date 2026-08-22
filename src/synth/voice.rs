#![allow(dead_code)]

use std::f32::consts;

use crate::synth::bi_quad_filter::BiQuadFilter;
use crate::synth::channel::Channel;
use crate::synth::generator_type::GeneratorType;
use crate::synth::lfo::Lfo;
use crate::synth::modulation_envelope::ModulationEnvelope;
use crate::synth::modulator::{Modulator, DEFAULT_VEL_TO_ATTENUATION};
use crate::synth::oscillator::Oscillator;
use crate::synth::region_ex::RegionEx;
use crate::synth::region_pair::RegionPair;

/// The soft pedal's attenuation on notes struck under it (about -4 dB).
const SOFT_PEDAL_GAIN: f32 = 0.63;
use crate::synth::soundfont_math::SoundFontMath;
use crate::synth::synthesizer_settings::SynthesizerSettings;
use crate::synth::volume_envelope::VolumeEnvelope;

#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
enum VoiceState {
    Playing = 0,
    ReleaseRequested = 1,
    Released = 2,
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct Voice {
    vol_env: VolumeEnvelope,
    mod_env: ModulationEnvelope,

    vib_lfo: Lfo,
    mod_lfo: Lfo,

    oscillator: Oscillator,
    filter: BiQuadFilter,

    block: Vec<f32>,

    // A sudden change in the mix gain will cause pop noise.
    // To avoid this, we save the mix gain of the previous block,
    // and smooth out the gain if the gap between the current and previous gain is too large.
    // The actual smoothing process is done in the WriteBlock method of the Synthesizer class.
    pub(crate) previous_mix_gain_left: f32,
    pub(crate) previous_mix_gain_right: f32,
    pub(crate) current_mix_gain_left: f32,
    pub(crate) current_mix_gain_right: f32,

    pub(crate) previous_reverb_send: f32,
    /// Caught by the sostenuto pedal: release waits for the pedal even
    /// after the key (or the hold pedal) lets go.
    sostenuto_held: bool,
    /// Struck under the soft pedal: the note keeps its softened voice
    /// for its whole life, as a softened hammer would.
    soft_struck: bool,
    /// A drum note's per-note pitch offset (NRPN 18 rr), in semitones.
    drum_pitch: f32,
    /// The portamento glide still to travel, in semitones, decaying
    /// toward zero block by block.
    glide_offset: f32,
    /// The per-block decay the synthesizer derived from CC5.
    glide_decay: f32,
    pub(crate) previous_chorus_send: f32,
    pub(crate) current_reverb_send: f32,
    pub(crate) current_chorus_send: f32,

    exclusive_class: i32,
    // The note's identity, as the bytes that named it. The pitch maths
    // works in f32 and the envelope scaling in i32; each widens where
    // it computes rather than carrying a wider identity around.
    channel: u8,
    key: u8,
    velocity: u8,

    note_gain: f32,

    cutoff: f32,
    resonance: f32,

    vib_lfo_to_pitch: f32,
    mod_lfo_to_pitch: f32,
    mod_env_to_pitch: f32,

    mod_lfo_to_cutoff: i32,
    mod_env_to_cutoff: i32,
    dynamic_cutoff: bool,

    mod_lfo_to_volume: f32,
    dynamic_volume: bool,

    instrument_pan: f32,
    instrument_reverb: f32,
    instrument_chorus: f32,

    // Some instruments require fast cutoff change, which can cause pop noise.
    // This is used to smooth out the cutoff frequency.
    smoothed_cutoff: f32,

    // --- SF2 modulators ------------------------------------------------
    //
    // The bank's own routings, instrument-level and preset-level combined
    // (supersession within each level was resolved when the regions were
    // built; across levels the spec sums them). Contributions whose
    // sources are fixed at note-on are folded into the base values in
    // start(); the rest are re-evaluated every block, which is also where
    // the synthesizer already re-reads its channel state.
    dynamic_mods: Vec<Modulator>,
    /// The filter's base position in cents, static contributions included.
    base_cutoff_cents: f32,
    /// Static modulator attenuation, in centibels.
    static_atten_cb: f32,
    /// The hard-coded velocity curve is replaced when the bank supersedes
    /// the spec's default velocity-to-attenuation routing.
    vel_atten_superseded: bool,
    /// A bank modulator drives this send, so the synthesizer's own
    /// channel-CC scaling must stand aside for it.
    reverb_from_mods: bool,
    chorus_from_mods: bool,
    /// Live modulator sums, refreshed per block: attenuation cB, filter
    /// cents, reverb and chorus in 0.1% units, pan in 0.1% units.
    dyn_atten_cb: f32,
    dyn_cutoff_cents: f32,
    dyn_reverb: f32,
    dyn_chorus: f32,
    dyn_pan: f32,

    voice_state: VoiceState,
    /// Time elapsed in samples
    voice_length: usize,
    min_voice_length: usize,
}

impl Voice {
    pub(crate) fn new(settings: &SynthesizerSettings) -> Self {
        Self {
            vol_env: VolumeEnvelope::new(settings),
            mod_env: ModulationEnvelope::new(settings),
            vib_lfo: Lfo::new(settings),
            mod_lfo: Lfo::new(settings),
            oscillator: Oscillator::new(settings),
            filter: BiQuadFilter::new(settings),
            block: vec![0_f32; settings.block_size],
            previous_mix_gain_left: 0_f32,
            previous_mix_gain_right: 0_f32,
            current_mix_gain_left: 0_f32,
            current_mix_gain_right: 0_f32,
            previous_reverb_send: 0_f32,
            sostenuto_held: false,
            soft_struck: false,
            drum_pitch: 0_f32,
            glide_offset: 0_f32,
            glide_decay: 1_f32,
            previous_chorus_send: 0_f32,
            current_reverb_send: 0_f32,
            current_chorus_send: 0_f32,
            exclusive_class: 0,
            channel: 0,
            key: 0,
            velocity: 0,
            note_gain: 0_f32,
            cutoff: 0_f32,
            resonance: 0_f32,
            vib_lfo_to_pitch: 0_f32,
            mod_lfo_to_pitch: 0_f32,
            mod_env_to_pitch: 0_f32,
            mod_lfo_to_cutoff: 0,
            mod_env_to_cutoff: 0,
            dynamic_cutoff: false,
            mod_lfo_to_volume: 0_f32,
            dynamic_volume: false,
            instrument_pan: 0_f32,
            instrument_reverb: 0_f32,
            instrument_chorus: 0_f32,
            smoothed_cutoff: 0_f32,
            dynamic_mods: Vec::new(),
            base_cutoff_cents: 0_f32,
            static_atten_cb: 0_f32,
            vel_atten_superseded: false,
            reverb_from_mods: false,
            chorus_from_mods: false,
            dyn_atten_cb: 0_f32,
            dyn_cutoff_cents: 0_f32,
            dyn_reverb: 0_f32,
            dyn_chorus: 0_f32,
            dyn_pan: 0_f32,
            voice_state: VoiceState::Playing,
            voice_length: 0,
            min_voice_length: (settings.sample_rate / 500) as usize,
        }
    }

    pub(crate) fn set_sostenuto_held(&mut self, held: bool) {
        self.sostenuto_held = held;
    }

    /// Begin this voice gliding in from another key (portamento, or a
    /// Portamento Control message ahead of the note).
    pub(crate) fn glide_from(&mut self, source_key: u8, decay_per_block: f32) {
        // A signed distance in semitones: gliding down is negative.
        self.glide_offset = f32::from(source_key) - f32::from(self.key);
        self.glide_decay = decay_per_block;
    }

    /// Portamento Control onto a sounding voice: re-tune to the new
    /// key, gliding from wherever the pitch is now, without
    /// re-triggering -- the manual's legato case.
    pub(crate) fn retune_to(&mut self, new_key: u8, decay_per_block: f32) {
        self.glide_offset += f32::from(self.key) - f32::from(new_key);
        self.key = new_key;
        self.glide_decay = decay_per_block;
    }

    /// Still held down: neither the key nor a pedal has asked for the
    /// release yet.
    pub(crate) fn is_sounding(&self) -> bool {
        self.voice_state == VoiceState::Playing
    }

    pub(crate) fn start(
        &mut self,
        region: &RegionPair,
        channel_info: &Channel,
        channel: u8,
        key: u8,
        velocity: u8,
    ) {
        self.exclusive_class = region.get_exclusive_class();
        self.channel = channel;
        self.key = key;
        self.velocity = velocity;
        // A voice from the pool must not inherit the last note's pedals.
        self.sostenuto_held = false;
        self.soft_struck = false;
        self.glide_offset = 0_f32;
        self.glide_decay = 1_f32;

        // The bank's modulators for this note: the instrument level, which
        // superseded the defaults where it names their routing, and the
        // preset level, which sums on top per spec. Static contributions
        // (velocity/key sources) are folded into the base values here;
        // live ones are re-evaluated each block.
        self.vel_atten_superseded = region
            .instrument
            .modulators
            .iter()
            .any(|m| m.same_routing(&DEFAULT_VEL_TO_ATTENUATION));
        self.reverb_from_mods = false;
        self.chorus_from_mods = false;
        self.static_atten_cb = 0_f32;
        self.dynamic_mods.clear();
        let mut static_cutoff_cents = 0_f32;
        let mut static_q_cb = 0_f32;
        let mut static_pan = 0_f32;
        let mut static_reverb = 0_f32;
        let mut static_chorus = 0_f32;
        let mut static_mod_lfo_pitch = 0_f32;
        let mut static_vib_lfo_pitch = 0_f32;
        let mut static_mod_env_pitch = 0_f32;
        let mut static_mod_lfo_cutoff = 0_f32;
        let mut static_mod_env_cutoff = 0_f32;
        let mut static_mod_lfo_volume = 0_f32;
        let mut start_offset = 0_f32;
        for m in region
            .instrument
            .modulators
            .iter()
            .chain(region.preset.modulators.iter())
        {
            // Only a modulator that routes the send's own controller
            // takes it over from the channel scaling; a bank adding
            // reverb from velocity or a constant must not disconnect
            // the player's CC91.
            match m.destination {
                GeneratorType::REVERB_EFFECTS_SEND if m.source_cc() == Some(91) => {
                    self.reverb_from_mods = true
                }
                GeneratorType::CHORUS_EFFECTS_SEND if m.source_cc() == Some(93) => {
                    self.chorus_from_mods = true
                }
                _ => {}
            }
            if m.is_static() {
                let value = m.static_contribution(key, velocity);
                match m.destination {
                    GeneratorType::START_ADDRESS_OFFSET => start_offset += value,
                    GeneratorType::START_ADDRESS_COARSE_OFFSET => start_offset += 32768_f32 * value,
                    GeneratorType::INITIAL_ATTENUATION => self.static_atten_cb += value,
                    GeneratorType::INITIAL_FILTER_CUTOFF_FREQUENCY => static_cutoff_cents += value,
                    GeneratorType::INITIAL_FILTER_Q => static_q_cb += value,
                    GeneratorType::PAN => static_pan += value,
                    GeneratorType::REVERB_EFFECTS_SEND => static_reverb += value,
                    GeneratorType::CHORUS_EFFECTS_SEND => static_chorus += value,
                    GeneratorType::MODULATION_LFO_TO_PITCH => static_mod_lfo_pitch += value,
                    GeneratorType::VIBRATO_LFO_TO_PITCH => static_vib_lfo_pitch += value,
                    GeneratorType::MODULATION_ENVELOPE_TO_PITCH => static_mod_env_pitch += value,
                    GeneratorType::MODULATION_LFO_TO_FILTER_CUTOFF_FREQUENCY => {
                        static_mod_lfo_cutoff += value
                    }
                    GeneratorType::MODULATION_ENVELOPE_TO_FILTER_CUTOFF_FREQUENCY => {
                        static_mod_env_cutoff += value
                    }
                    GeneratorType::MODULATION_LFO_TO_VOLUME => static_mod_lfo_volume += value,
                    // Destinations outside the supported set contribute
                    // nothing rather than misdriving something else.
                    _ => {}
                }
            } else {
                self.dynamic_mods.push(*m);
            }
        }
        self.dyn_atten_cb = 0_f32;
        self.dyn_cutoff_cents = 0_f32;
        self.dyn_reverb = 0_f32;
        self.dyn_chorus = 0_f32;
        self.dyn_pan = 0_f32;

        if velocity > 0 {
            // According to the Polyphone's implementation, the initial attenuation should be reduced to 40%.
            // I'm not sure why, but this indeed improves the loudness variability.
            let sample_attenuation = 0.4_f32 * region.get_initial_attenuation();
            let filter_attenuation = 0.5_f32 * region.get_initial_filter_q();
            // A bank that supersedes the default velocity curve supplies
            // its own through the attenuation modulators; applying the
            // hard-coded one as well would count velocity twice.
            let velocity_decibels = if self.vel_atten_superseded {
                0_f32
            } else {
                2_f32 * SoundFontMath::linear_to_decibels(velocity as f32 / 127_f32)
            };
            let decibels = velocity_decibels - sample_attenuation - filter_attenuation;
            self.note_gain = SoundFontMath::decibels_to_linear(decibels);
        } else {
            self.note_gain = 0_f32;
        }

        // The channel's GS tone modifies ride the same static
        // accumulators the bank's modulators use, folded in before the
        // filter and vibrato derive from them.
        static_cutoff_cents += channel_info.nrpn_cutoff_cents();
        static_q_cb += channel_info.nrpn_resonance_cb();
        static_vib_lfo_pitch += channel_info.nrpn_vib_depth_cents();

        // The filter's base is kept in cents so modulator contributions,
        // which arrive in cents, compose with the LFO and envelope paths.
        self.base_cutoff_cents = region.gen_sum(GeneratorType::INITIAL_FILTER_CUTOFF_FREQUENCY)
            as f32
            + static_cutoff_cents;
        self.cutoff = SoundFontMath::cents_to_hertz(self.base_cutoff_cents);
        self.resonance = SoundFontMath::decibels_to_linear(
            region.get_initial_filter_q() + 0.1_f32 * static_q_cb,
        );

        self.vib_lfo_to_pitch =
            0.01_f32 * (region.get_vibrato_lfo_to_pitch() as f32 + static_vib_lfo_pitch);
        self.mod_lfo_to_pitch =
            0.01_f32 * (region.get_modulation_lfo_to_pitch() as f32 + static_mod_lfo_pitch);
        self.mod_env_to_pitch =
            0.01_f32 * (region.get_modulation_envelope_to_pitch() as f32 + static_mod_env_pitch);

        self.mod_lfo_to_cutoff = (region.get_modulation_lfo_to_filter_cutoff_frequency() as f32
            + static_mod_lfo_cutoff) as i32;
        self.mod_env_to_cutoff = (region.get_modulation_envelope_to_filter_cutoff_frequency()
            as f32
            + static_mod_env_cutoff) as i32;
        self.dynamic_cutoff = self.mod_lfo_to_cutoff != 0
            || self.mod_env_to_cutoff != 0
            || !self.dynamic_mods.is_empty();

        self.mod_lfo_to_volume =
            region.get_modulation_lfo_to_volume() + 0.1_f32 * static_mod_lfo_volume;
        self.dynamic_volume = self.mod_lfo_to_volume > 0.05_f32;

        // A drum note's own place and pitch, when its part set them.
        self.drum_pitch = channel_info.drum_pitch_semitones(key);
        self.note_gain *= channel_info.drum_level_gain(key);
        if let Some(pan) = channel_info.drum_pan_override(key, velocity) {
            static_pan = 10.0 * (pan - region.get_pan());
        }

        self.instrument_pan =
            SoundFontMath::clamp(region.get_pan() + 0.1_f32 * static_pan, -50_f32, 50_f32);
        self.instrument_reverb =
            0.01_f32 * (region.get_reverb_effects_send() + 0.1_f32 * static_reverb);
        self.instrument_chorus =
            0.01_f32 * (region.get_chorus_effects_send() + 0.1_f32 * static_chorus);

        let eg = channel_info.nrpn_eg_factors();
        RegionEx::start_volume_envelope(&mut self.vol_env, region, key, velocity, eg);
        RegionEx::start_modulation_envelope(&mut self.mod_env, region, key, velocity);
        RegionEx::start_vibrato(
            &mut self.vib_lfo,
            region,
            key,
            velocity,
            channel_info.nrpn_vib_rate_factor(),
            channel_info.nrpn_vib_delay_factor(),
        );
        RegionEx::start_modulation(&mut self.mod_lfo, region, key, velocity);
        RegionEx::start_oscillator(
            &mut self.oscillator,
            region,
            SoundFontMath::clamp(start_offset, -1e7_f32, 1e7_f32) as i32,
        );
        self.filter.clear_buffer();
        self.filter.set_low_pass_filter(self.cutoff, self.resonance);

        self.smoothed_cutoff = self.cutoff;

        self.voice_state = VoiceState::Playing;
        self.voice_length = 0;
    }

    pub(crate) fn end(&mut self) {
        if self.voice_state == VoiceState::Playing {
            self.voice_state = VoiceState::ReleaseRequested;
        }
    }

    pub(crate) fn kill(&mut self) {
        self.note_gain = 0_f32;
    }

    pub(crate) fn process(&mut self, data: &[i16], channels: &[Channel]) -> bool {
        if self.note_gain < SoundFontMath::NON_AUDIBLE {
            return false;
        }

        let channel_info = &channels[usize::from(self.channel)];

        self.update_dynamic_mods(channel_info);

        self.release_if_necessary(channel_info);

        if !self.vol_env.process(self.block.len()) {
            return false;
        }

        self.mod_env.process(self.block.len());
        self.vib_lfo.process();
        self.mod_lfo.process();

        let vib_pitch_change = (0.01_f32 * channel_info.get_modulation() + self.vib_lfo_to_pitch)
            * self.vib_lfo.get_value();
        let mod_pitch_change = self.mod_lfo_to_pitch * self.mod_lfo.get_value()
            + self.mod_env_to_pitch * self.mod_env.get_value();
        let channel_pitch_change = channel_info.get_tune() + channel_info.get_pitch_bend();
        let pitch = f32::from(self.key)
            + self.drum_pitch
            + self.glide_offset
            + vib_pitch_change
            + mod_pitch_change
            + channel_pitch_change;
        // The glide closes on the note exponentially; close enough is
        // arrived.
        self.glide_offset *= self.glide_decay;
        if self.glide_offset.abs() < 0.005 {
            self.glide_offset = 0_f32;
        }
        if !self.oscillator.process(data, &mut self.block[..], pitch) {
            return false;
        }

        if self.dynamic_cutoff {
            let cents = self.mod_lfo_to_cutoff as f32 * self.mod_lfo.get_value()
                + self.mod_env_to_cutoff as f32 * self.mod_env.get_value()
                + self.dyn_cutoff_cents;
            let factor = SoundFontMath::cents_to_multiplying_factor(cents);
            let new_cutoff = factor * self.cutoff;

            // The cutoff change is limited within x0.5 and x2 to reduce pop noise.
            let lower_limit = 0.5_f32 * self.smoothed_cutoff;
            let upper_limit = 2_f32 * self.smoothed_cutoff;
            self.smoothed_cutoff = SoundFontMath::clamp(new_cutoff, lower_limit, upper_limit);

            self.filter
                .set_low_pass_filter(self.smoothed_cutoff, self.resonance);
        }
        self.filter.process(&mut self.block[..]);

        self.previous_mix_gain_left = self.current_mix_gain_left;
        self.previous_mix_gain_right = self.current_mix_gain_right;
        self.previous_reverb_send = self.current_reverb_send;
        self.previous_chorus_send = self.current_chorus_send;

        if self.voice_length == 0 && channel_info.get_soft_pedal() {
            self.soft_struck = true;
        }

        // According to the GM spec, the following value should be squared.
        let ve = channel_info.get_volume() * channel_info.get_expression();
        let channel_gain = ve * ve;

        let mut mix_gain = self.note_gain * channel_gain * self.vol_env.get_value();
        if self.soft_struck {
            // The soft pedal's quieter, rounder hammer: about -4 dB and
            // a gently closed filter on notes struck while it is down.
            mix_gain *= SOFT_PEDAL_GAIN;
        }
        let mod_atten_cb = self.static_atten_cb + self.dyn_atten_cb;
        if mod_atten_cb != 0_f32 {
            // Attenuation modulators land in centibels and are scaled by
            // the same 0.4 the generator's own attenuation gets, so a bank
            // that supersedes the velocity curve keeps its loudness
            // relationships.
            mix_gain *= SoundFontMath::decibels_to_linear(-0.1_f32 * 0.4_f32 * mod_atten_cb);
        }
        if self.dynamic_volume {
            let decibels = self.mod_lfo_to_volume * self.mod_lfo.get_value();
            mix_gain *= SoundFontMath::decibels_to_linear(decibels);
        }

        let angle = (consts::PI / 200_f32)
            * (channel_info.get_pan() + self.instrument_pan + 0.1_f32 * self.dyn_pan + 50_f32);
        if angle <= 0_f32 {
            self.current_mix_gain_left = mix_gain;
            self.current_mix_gain_right = 0_f32;
        } else if angle >= SoundFontMath::HALF_PI {
            self.current_mix_gain_left = 0_f32;
            self.current_mix_gain_right = mix_gain;
        } else {
            self.current_mix_gain_left = mix_gain * angle.cos();
            self.current_mix_gain_right = mix_gain * angle.sin();
        }

        // A bank that routes CC91/CC93 itself has taken over the send
        // entirely: its curve alone decides, without the channel scaling
        // (the controller would apply twice) and without the instrument's
        // own base send (the player's zero must mean zero).
        self.current_reverb_send = if self.reverb_from_mods {
            SoundFontMath::clamp(0.001_f32 * self.dyn_reverb, 0_f32, 1_f32)
        } else {
            SoundFontMath::clamp(
                channel_info.get_reverb_send_for(self.key) + self.instrument_reverb,
                0_f32,
                1_f32,
            )
        };
        self.current_chorus_send = if self.chorus_from_mods {
            SoundFontMath::clamp(0.001_f32 * self.dyn_chorus, 0_f32, 1_f32)
        } else {
            SoundFontMath::clamp(
                channel_info.get_chorus_send_for(self.key) + self.instrument_chorus,
                0_f32,
                1_f32,
            )
        };

        if self.voice_length == 0 {
            self.previous_mix_gain_left = self.current_mix_gain_left;
            self.previous_mix_gain_right = self.current_mix_gain_right;
            self.previous_reverb_send = self.current_reverb_send;
            self.previous_chorus_send = self.current_chorus_send;
        }

        self.voice_length += self.block.len();

        true
    }

    /// Re-evaluate the modulators whose sources are live. Runs once per
    /// block, alongside the channel reads the block already does; with no
    /// dynamic modulators it is a handful of stores.
    fn update_dynamic_mods(&mut self, channel_info: &Channel) {
        self.dyn_atten_cb = 0_f32;
        self.dyn_cutoff_cents = 0_f32;
        self.dyn_reverb = 0_f32;
        self.dyn_chorus = 0_f32;
        self.dyn_pan = 0_f32;
        if self.dynamic_mods.is_empty() {
            return;
        }
        for i in 0..self.dynamic_mods.len() {
            let m = self.dynamic_mods[i];
            let value = m.contribution(channel_info, self.key, self.velocity);
            match m.destination {
                GeneratorType::INITIAL_ATTENUATION => self.dyn_atten_cb += value,
                GeneratorType::INITIAL_FILTER_CUTOFF_FREQUENCY => self.dyn_cutoff_cents += value,
                GeneratorType::REVERB_EFFECTS_SEND => self.dyn_reverb += value,
                GeneratorType::CHORUS_EFFECTS_SEND => self.dyn_chorus += value,
                GeneratorType::PAN => self.dyn_pan += value,
                _ => {}
            }
        }
    }

    fn release_if_necessary(&mut self, channel_info: &Channel) {
        if self.voice_length < self.min_voice_length {
            return;
        }

        if self.voice_state == VoiceState::ReleaseRequested
            && !channel_info.get_hold_pedal()
            && !self.sostenuto_held
        {
            self.vol_env.release();
            self.mod_env.release();
            self.oscillator.release();

            self.voice_state = VoiceState::Released;
        }
    }

    pub(crate) fn block(&self) -> &Vec<f32> {
        &self.block
    }

    pub(crate) fn voice_length(&self) -> usize {
        self.voice_length
    }

    pub(crate) fn exclusive_class(&self) -> i32 {
        self.exclusive_class
    }

    pub(crate) fn channel(&self) -> u8 {
        self.channel
    }

    pub(crate) fn key(&self) -> u8 {
        self.key
    }

    pub(crate) fn priority(&self) -> f32 {
        if self.note_gain < SoundFontMath::NON_AUDIBLE {
            0_f32
        } else {
            self.vol_env.get_priority()
        }
    }
}
