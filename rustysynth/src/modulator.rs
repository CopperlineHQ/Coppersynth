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
#[derive(Clone, Copy, PartialEq, Eq)]
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
        if size == 0 || size % 10 != 0 {
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
    pub(crate) fn same_routing(&self, other: &Modulator) -> bool {
        self.source == other.source
            && self.destination == other.destination
            && self.amount_source == other.amount_source
            && self.transform == other.transform
    }
}

/// The decoded halves of a source enumerator.
#[derive(Clone, Copy, PartialEq, Eq)]
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
}

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
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/GeneralUser-GS.sf2");
        let Ok(mut file) = std::fs::File::open(path) else {
            return;
        };
        let font = crate::SoundFont::new(&mut file).expect("GeneralUser parses");
        assert!(font.get_presets().len() > 200);
    }
}
