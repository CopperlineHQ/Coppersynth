#![allow(dead_code)]

use std::io::Read;

use crate::{binary_reader::BinaryReader, error::SoundFontError};

/// One SFModList entry: a routing from a real-time source (note-on
/// velocity, key number, a MIDI CC, pitch wheel...) to a destination
/// generator, scaled by `amount` and optionally by a second source.
///
/// The SoundFont spec models everything expressive a bank does at play
/// time this way, including overriding its own defaults: a file modulator
/// with the same source/destination/transform as a default supersedes it,
/// and an amount of zero is how a bank switches a default off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct Modulator {
    /// sfModSrcOper: the source enumerator, encoding index, CC flag,
    /// direction, polarity and curve type. Decoded by [`ModSource`].
    pub(crate) source: u16,
    /// sfModDestOper: the generator this modulator writes to.
    pub(crate) destination: u16,
    /// sfModAmount: the full-scale contribution, in the destination
    /// generator's own units.
    pub(crate) amount: i16,
    /// sfModAmtSrcOper: a second source scaling `amount`, or zero.
    pub(crate) amount_source: u16,
    /// sfModTransOper: 0 = linear, 2 = absolute value.
    pub(crate) transform: u16,
}

impl Modulator {
    fn new<R: Read>(reader: &mut R) -> Result<Self, SoundFontError> {
        let source = BinaryReader::read_u16(reader)?;
        let destination = BinaryReader::read_u16(reader)?;
        let amount = BinaryReader::read_i16(reader)?;
        let amount_source = BinaryReader::read_u16(reader)?;
        let transform = BinaryReader::read_u16(reader)?;

        Ok(Self {
            source,
            destination,
            amount,
            amount_source,
            transform,
        })
    }

    pub(crate) fn read_from_chunk<R: Read>(
        reader: &mut R,
        size: usize,
    ) -> Result<Vec<Modulator>, SoundFontError> {
        if size == 0 || !size.is_multiple_of(10) {
            return Err(SoundFontError::InvalidModulatorList);
        }

        let count = size / 10 - 1;

        let mut modulators: Vec<Modulator> = Vec::new();
        for _i in 0..count {
            modulators.push(Modulator::new(reader)?);
        }

        // The last one is the terminator.
        Modulator::new(reader)?;

        Ok(modulators)
    }

    /// Whether `other` names the same routing: per spec, two modulators are
    /// identical when source, destination, amount-source and transform all
    /// match -- the amount is the value, not part of the identity. A file
    /// modulator identical to a default supersedes it.
    /// The MIDI controller this modulator's primary source reads, if it
    /// reads one: bit 7 of the source operator is the CC flag, the low
    /// seven bits the controller number.
    pub(crate) fn source_cc(&self) -> Option<u8> {
        (self.source & 0x80 != 0).then_some((self.source & 0x7F) as u8)
    }

    pub(crate) fn same_routing(&self, other: &Modulator) -> bool {
        self.source == other.source
            && self.destination == other.destination
            && self.amount_source == other.amount_source
            && self.transform == other.transform
    }
}

/// The decoded halves of a source enumerator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModSource {
    /// The controller index: a general-controller number, or a MIDI CC.
    pub(crate) index: u8,
    /// Whether `index` names a MIDI CC rather than a general controller.
    pub(crate) is_cc: bool,
    /// Max-to-min rather than min-to-max.
    pub(crate) descending: bool,
    /// Bipolar (-1..1) rather than unipolar (0..1).
    pub(crate) bipolar: bool,
    /// 0 = linear, 1 = concave, 2 = convex, 3 = switch.
    pub(crate) curve: u8,
}

/// General-controller indices the spec defines for a non-CC source.
pub(crate) mod general_controller {
    pub const NONE: u8 = 0;
    pub const NOTE_ON_VELOCITY: u8 = 2;
    pub const NOTE_ON_KEY: u8 = 3;
    pub const POLY_PRESSURE: u8 = 10;
    pub const CHANNEL_PRESSURE: u8 = 13;
    pub const PITCH_WHEEL: u8 = 14;
    pub const PITCH_WHEEL_SENSITIVITY: u8 = 16;
}

impl ModSource {
    pub(crate) fn from_operator(source: u16) -> Self {
        Self {
            index: (source & 0x7F) as u8,
            is_cc: source & 0x80 != 0,
            descending: source & 0x100 != 0,
            bipolar: source & 0x200 != 0,
            curve: ((source >> 10) & 0x3F) as u8,
        }
    }

    /// Map a raw 0..1 controller value through this source's direction,
    /// curve and polarity.
    ///
    /// The curves are FluidSynth's continuous forms -- GeneralUser GS is
    /// voiced against FluidSynth, so its tables are the behaviour banks
    /// actually expect: concave(v) = -(40/96)*log10(1-v), convex its
    /// mirror, both clamped to 0..1. Bipolar output mirrors the curve
    /// around the centre into -1..1.
    pub(crate) fn map(&self, raw: f32) -> f32 {
        let v = raw.clamp(0.0, 1.0);
        let v = if self.descending { 1.0 - v } else { v };

        fn concave(v: f32) -> f32 {
            if v >= 1.0 {
                1.0
            } else {
                (-(40.0 / 96.0) * (1.0 - v).log10()).clamp(0.0, 1.0)
            }
        }
        fn convex(v: f32) -> f32 {
            if v <= 0.0 {
                0.0
            } else {
                (1.0 + (40.0 / 96.0) * v.log10()).clamp(0.0, 1.0)
            }
        }
        let unipolar = |v: f32| match self.curve {
            1 => concave(v),
            2 => convex(v),
            3 => {
                if v >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            _ => v,
        };

        if self.bipolar {
            // The curve is applied to each half, mirrored in sign, so a
            // bipolar concave source bends symmetrically about the centre.
            if v >= 0.5 {
                unipolar(2.0 * v - 1.0)
            } else {
                -unipolar(1.0 - 2.0 * v)
            }
        } else {
            unipolar(v)
        }
    }
}

/// The raw 0..1 value of a modulator source, before the curve.
///
/// Velocity and key are fixed per voice; everything else is read live so
/// CC-driven modulators follow the controller. Sources the spec defines
/// but games never use (link, unsupported general controllers) read as
/// zero, which silences the modulators built on them rather than
/// misdriving a destination.
pub(crate) fn source_raw(
    source: ModSource,
    channel: &crate::channel::Channel,
    key: i32,
    velocity: i32,
) -> f32 {
    if source.is_cc {
        return channel.get_cc(source.index) as f32 / 127.0;
    }
    match source.index {
        general_controller::NONE => 1.0,
        general_controller::NOTE_ON_VELOCITY => velocity as f32 / 127.0,
        general_controller::NOTE_ON_KEY => key as f32 / 127.0,
        general_controller::PITCH_WHEEL => channel.get_pitch_bend_raw(),
        general_controller::PITCH_WHEEL_SENSITIVITY => channel.get_pitch_bend_range() / 127.0,
        _ => 0.0,
    }
}

impl Modulator {
    /// This modulator's contribution, in its destination generator's own
    /// units.
    pub(crate) fn contribution(
        &self,
        channel: &crate::channel::Channel,
        key: i32,
        velocity: i32,
    ) -> f32 {
        let primary = ModSource::from_operator(self.source);
        let mut value = source_raw(primary, channel, key, velocity);
        value = primary.map(value);
        if self.amount_source != 0 {
            let secondary = ModSource::from_operator(self.amount_source);
            value *= secondary.map(source_raw(secondary, channel, key, velocity));
        }
        let out = value * self.amount as f32;
        if self.transform == 2 {
            out.abs()
        } else {
            out
        }
    }

    /// A zone's effective list: the global zone's modulators with the
    /// local zone's applied over them, a local modulator superseding a
    /// global one with the same routing rather than stacking with it.
    pub(crate) fn merge(global: &[Modulator], local: &[Modulator]) -> Vec<Modulator> {
        let mut merged: Vec<Modulator> = Vec::with_capacity(global.len() + local.len());
        for g in global {
            if !local.iter().any(|l| l.same_routing(g)) {
                merged.push(*g);
            }
        }
        merged.extend_from_slice(local);
        merged
    }

    /// The contribution of a modulator whose sources are all fixed at
    /// note-on. Sources that would need live channel state read as zero,
    /// so this is only meaningful for modulators [`is_static`] accepts.
    pub(crate) fn static_contribution(&self, key: i32, velocity: i32) -> f32 {
        let fixed = |op: u16| {
            let m = ModSource::from_operator(op);
            let raw = if m.is_cc {
                0.0
            } else {
                match m.index {
                    general_controller::NONE => 1.0,
                    general_controller::NOTE_ON_VELOCITY => velocity as f32 / 127.0,
                    general_controller::NOTE_ON_KEY => key as f32 / 127.0,
                    _ => 0.0,
                }
            };
            m.map(raw)
        };
        let mut value = fixed(self.source);
        if self.amount_source != 0 {
            value *= fixed(self.amount_source);
        }
        let out = value * self.amount as f32;
        if self.transform == 2 {
            out.abs()
        } else {
            out
        }
    }

    /// Whether every source this modulator reads is fixed for the life of
    /// a voice (velocity, key, or the constant), so its contribution can
    /// be computed once at note-on.
    pub(crate) fn is_static(&self) -> bool {
        let fixed = |s: u16| {
            let m = ModSource::from_operator(s);
            !m.is_cc
                && matches!(
                    m.index,
                    general_controller::NONE
                        | general_controller::NOTE_ON_VELOCITY
                        | general_controller::NOTE_ON_KEY
                )
        };
        fixed(self.source) && (self.amount_source == 0 || fixed(self.amount_source))
    }
}

/// The spec's default velocity-to-attenuation routing (source 0x0502:
/// velocity, concave, max-to-min). The synthesizer hard-codes its own
/// velocity curve; a bank modulator with this routing supersedes it, which
/// is how GeneralUser GS replaces the curve zone by zone.
pub(crate) const DEFAULT_VEL_TO_ATTENUATION: Modulator = Modulator {
    source: 0x0502,
    destination: 48,
    amount: 960,
    amount_source: 0,
    transform: 0,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Two records and the mandatory terminator, little-endian. The first
    /// is the spec's default velocity-to-attenuation modulator (source
    /// 0x0502: velocity, concave, max-to-min), the second is GeneralUser's
    /// Synth Bass 2 velocity-to-cutoff with its negative amount.
    #[test]
    fn records_parse_with_signed_amounts() {
        let bytes: Vec<u8> = [
            (0x0502u16, 48u16, 960i16, 0u16, 0u16),
            (0x0102, 8, -8500, 0, 0),
            (0, 0, 0, 0, 0),
        ]
        .iter()
        .flat_map(|&(s, d, a, r#as, t)| {
            let mut v = Vec::new();
            v.extend(s.to_le_bytes());
            v.extend(d.to_le_bytes());
            v.extend(a.to_le_bytes());
            v.extend(r#as.to_le_bytes());
            v.extend(t.to_le_bytes());
            v
        })
        .collect();

        let mods = Modulator::read_from_chunk(&mut bytes.as_slice(), bytes.len()).unwrap();
        assert_eq!(mods.len(), 2, "the terminator is not a modulator");
        assert_eq!(mods[0].destination, 48);
        assert_eq!(mods[0].amount, 960);
        assert_eq!(mods[1].destination, 8);
        assert_eq!(mods[1].amount, -8500, "amounts are signed");

        let src = ModSource::from_operator(mods[0].source);
        assert!(!src.is_cc);
        assert_eq!(src.index, general_controller::NOTE_ON_VELOCITY);
        assert!(src.descending);
        assert_eq!(src.curve, 1, "0x0502 is the concave velocity curve");

        // Identity ignores the amount: an amount-0 file modulator with the
        // same routing must supersede (switch off) a default.
        let off = Modulator {
            amount: 0,
            ..mods[0]
        };
        assert!(off.same_routing(&mods[0]));
        assert!(!mods[0].same_routing(&mods[1]));
    }

    #[test]
    fn a_chunk_that_is_not_whole_records_is_refused() {
        let bytes = vec![0u8; 15];
        assert!(Modulator::read_from_chunk(&mut bytes.as_slice(), 15).is_err());
    }

    /// The whole of GeneralUser GS through the real parsing path. Reading
    /// pmod/imod mis-sized would desynchronise every chunk after them, so
    /// a clean parse is the framing proof. Skips without the local asset,
    /// like the ignored suites elsewhere.
    #[test]
    fn generaluser_parses_through_the_modulator_path() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/GeneralUser-GS.sf2");
        let Ok(mut file) = std::fs::File::open(path) else {
            return;
        };
        let font = crate::SoundFont::new(&mut file).expect("GeneralUser parses");
        assert!(font.get_presets().len() > 200);
    }
}
