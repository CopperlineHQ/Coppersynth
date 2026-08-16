#![allow(dead_code)]

use std::io::Read;

use crate::synth::binary_reader::BinaryReader;
use crate::synth::error::SoundFontError;
use crate::synth::four_cc::FourCC;
use crate::synth::generator_type::GeneratorType;
use crate::synth::instrument::Instrument;
use crate::synth::preset::Preset;
use crate::synth::sample_header::SampleHeader;
use crate::synth::soundfont_info::SoundFontInfo;
use crate::synth::soundfont_parameters::SoundFontParameters;
use crate::synth::soundfont_sampledata::SoundFontSampleData;
use crate::synth::LoopMode;

/// Reperesents a SoundFont.
#[derive(Debug)]
#[non_exhaustive]
pub struct SoundFont {
    pub(crate) info: SoundFontInfo,
    pub(crate) bits_per_sample: i32,
    pub(crate) wave_data: Vec<i16>,
    pub(crate) sample_headers: Vec<SampleHeader>,
    pub(crate) presets: Vec<Preset>,
    pub(crate) instruments: Vec<Instrument>,
    repaired_regions: usize,
    dropped_regions: usize,
}

impl SoundFont {
    /// Loads a SoundFont from the stream.
    ///
    /// # Arguments
    ///
    /// * `reader` - The data stream used to load the SoundFont.
    pub fn new<R: Read>(reader: &mut R) -> Result<Self, SoundFontError> {
        let chunk_id = BinaryReader::read_four_cc(reader)?;
        if chunk_id != b"RIFF" {
            return Err(SoundFontError::RiffChunkNotFound);
        }

        let _size = BinaryReader::read_i32(reader);

        let form_type = BinaryReader::read_four_cc(reader)?;
        if form_type != b"sfbk" {
            return Err(SoundFontError::InvalidRiffChunkType {
                expected: FourCC::from_bytes(*b"sfbk"),
                actual: form_type,
            });
        }

        let info = SoundFontInfo::new(reader)?;
        let sample_data = SoundFontSampleData::new(reader)?;
        let parameters = SoundFontParameters::new(reader)?;

        let mut sound_font = Self {
            info,
            bits_per_sample: sample_data.bits_per_sample,
            wave_data: sample_data.wave_data,
            sample_headers: parameters.sample_headers,
            presets: parameters.presets,
            instruments: parameters.instruments,
            repaired_regions: 0,
            dropped_regions: 0,
        };

        sound_font.repair();

        Ok(sound_font)
    }

    /// Broken banks are the rule out in the world: rips carry regions
    /// with loop points past the data or inside out, and the checks that
    /// once rejected the whole font for one bad zone (issues #22/#33,
    /// PR #51 upstream) threw working instruments away with it. Instead:
    /// a region whose loop alone is broken plays through unlooped, a
    /// region whose sample bounds are nonsense is dropped, and the rest
    /// of the font stands.
    fn repair(&mut self) {
        let len = self.wave_data.len();
        let mut repaired = 0usize;
        let mut dropped = 0usize;
        for instrument in &mut self.instruments {
            instrument.regions.retain_mut(|region| {
                let start = region.get_sample_start();
                let end = region.get_sample_end();
                if start < 0 || end as usize >= len || end <= start {
                    dropped += 1;
                    return false;
                }
                let start_loop = region.get_sample_start_loop();
                let end_loop = region.get_sample_end_loop();
                let looped = region.get_sample_modes() != LoopMode::NoLoop;
                let loop_broken = start_loop < 0
                    || end_loop as usize >= len
                    || end_loop < start_loop
                    || (looped && start_loop >= end_loop);
                if loop_broken {
                    // The sample itself checked out, so keep it and
                    // defuse the loop: no loop mode, offsets cleared,
                    // and the loop points pinned inside the sample so
                    // nothing downstream can index past the data.
                    region.gs[GeneratorType::SAMPLE_MODES as usize] = 0;
                    region.gs[GeneratorType::START_LOOP_ADDRESS_OFFSET as usize] = 0;
                    region.gs[GeneratorType::END_LOOP_ADDRESS_OFFSET as usize] = 0;
                    region.gs[GeneratorType::START_LOOP_ADDRESS_COARSE_OFFSET as usize] = 0;
                    region.gs[GeneratorType::END_LOOP_ADDRESS_COARSE_OFFSET as usize] = 0;
                    region.sample_start_loop = region.sample_start;
                    region.sample_end_loop = region.sample_end;
                    repaired += 1;
                }
                true
            });
        }
        self.repaired_regions = repaired;
        self.dropped_regions = dropped;
    }

    /// How many regions the load had to mend (loops defused) and how
    /// many it had to drop (sample bounds beyond saving), so a host can
    /// tell the user their bank arrived bruised.
    pub fn get_repairs(&self) -> (usize, usize) {
        (self.repaired_regions, self.dropped_regions)
    }

    /// Gets the information of the SoundFont.
    pub fn get_info(&self) -> &SoundFontInfo {
        &self.info
    }

    /// Gets the bits per sample of the sample data.
    pub fn get_bits_per_sample(&self) -> i32 {
        self.bits_per_sample
    }

    /// Gets the sample data.
    pub fn get_wave_data(&self) -> &[i16] {
        &self.wave_data[..]
    }

    /// Gets the samples of the SoundFont.
    pub fn get_sample_headers(&self) -> &[SampleHeader] {
        &self.sample_headers[..]
    }

    /// Gets the presets of the SoundFont.
    pub fn get_presets(&self) -> &[Preset] {
        &self.presets[..]
    }

    /// Gets the instruments of the SoundFont.
    pub fn get_instruments(&self) -> &[Instrument] {
        &self.instruments[..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{fs::File, path::PathBuf};

    fn samples_dir_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples")
    }

    #[test]
    fn test_load_reject_sf3() {
        let path = samples_dir_path().join("dummy.sf3");
        let mut file = File::open(&path).unwrap();
        assert!(matches!(
            SoundFont::new(&mut file),
            Err(SoundFontError::UnsupportedSampleFormat)
        ));
    }

    // smpl sub-chunk exists, but is zero-length.
    #[test]
    fn test_load_empty_samples() {
        let path = samples_dir_path().join("test_empty_samples.sf2");
        let mut file = File::open(&path).unwrap();
        assert!(matches!(
            SoundFont::new(&mut file),
            Err(SoundFontError::SampleDataNotFound)
        ));
    }
}
