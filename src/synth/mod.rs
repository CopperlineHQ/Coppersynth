//! The synthesis core: Coppersynth's derivative of Nobuaki Tanaka's
//! RustySynth (itself a port of his MeltySynth), folded in as a module.
//! What the derivation added: SF2 modulators, master effect returns,
//! tolerant soundfont loading, and the introspection the front panel
//! reads. Provenance and licence: LICENSE-rustysynth.txt at the root.

mod error;

mod array_math;
mod binary_reader;
mod four_cc;
mod read_counter;

mod generator;
mod generator_type;
mod instrument;
mod instrument_info;
mod instrument_region;
mod loop_mode;
mod preset;
mod preset_info;
mod preset_region;
mod sample_header;
mod soundfont;
mod soundfont_info;
mod soundfont_math;
mod soundfont_parameters;
mod soundfont_sampledata;
mod soundfont_version;
mod zone;
mod zone_info;

mod bi_quad_filter;
mod channel;
mod envelope_stage;
mod lfo;
mod modulation_envelope;
mod oscillator;
mod region_ex;
mod region_pair;
mod synthesizer;
mod synthesizer_settings;
mod voice;
mod voice_collection;
mod volume_envelope;

mod midifile;
mod midifile_looptype;
mod modulator;

mod chorus;
mod reverb;

pub use self::chorus::ChorusType;
pub use self::error::MidiFileError;
pub use self::error::SoundFontError;
pub use self::error::SynthesizerError;
pub use self::instrument::Instrument;
pub use self::instrument_region::InstrumentRegion;
pub use self::loop_mode::LoopMode;
pub use self::midifile::MidiFile;
pub use self::midifile_looptype::MidiFileLoopType;
pub use self::preset::Preset;
pub use self::preset_region::PresetRegion;
pub use self::sample_header::SampleHeader;
pub use self::soundfont::SoundFont;
pub use self::soundfont_info::SoundFontInfo;
pub use self::soundfont_version::SoundFontVersion;
pub use self::synthesizer::Synthesizer;
pub use self::synthesizer_settings::SynthesizerSettings;
