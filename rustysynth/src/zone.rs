use crate::error::SoundFontError;
use crate::generator::Generator;
use crate::modulator::Modulator;
use crate::zone_info::ZoneInfo;

#[non_exhaustive]
pub(crate) struct Zone {
    pub(crate) generators: Vec<Generator>,
    pub(crate) modulators: Vec<Modulator>,
}

impl Zone {
    pub(crate) fn empty() -> Self {
        Self {
            generators: Vec::new(),
            modulators: Vec::new(),
        }
    }

    fn new(info: &ZoneInfo, generators: &[Generator], modulators: &[Modulator]) -> Self {
        let mut segment: Vec<Generator> = Vec::new();

        for i in 0..info.generator_count {
            segment.push(generators[(info.generator_index + i) as usize]);
        }

        // The indices were bookkept before the records were read at all;
        // now they select this zone's slice of the modulator list.
        let mut mod_segment: Vec<Modulator> = Vec::new();

        for i in 0..info.modulator_count {
            mod_segment.push(modulators[(info.modulator_index + i) as usize]);
        }

        Self {
            generators: segment,
            modulators: mod_segment,
        }
    }

    pub(crate) fn create(
        infos: &[ZoneInfo],
        generators: &[Generator],
        modulators: &[Modulator],
    ) -> Result<Vec<Zone>, SoundFontError> {
        if infos.len() <= 1 {
            return Err(SoundFontError::ZoneNotFound);
        }

        // The last one is the terminator.
        let count = infos.len() - 1;

        let mut zones: Vec<Zone> = Vec::new();
        for info in infos.iter().take(count) {
            zones.push(Zone::new(info, generators, modulators));
        }

        Ok(zones)
    }
}
