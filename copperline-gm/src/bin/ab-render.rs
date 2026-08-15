//! Render a MIDI file through the forked rustysynth, for A/B listening
//! against a FluidSynth rendering of the same file.
//!
//! This is the fidelity rig: GeneralUser GS leans on SF2 modulators the
//! vanilla core does not apply, and paired WAVs are how that gap is
//! measured -- first by ear, later as the oracle for the modulator work.
//!
//!     ab-render SOUNDFONT.sf2 IN.mid OUT.wav
//!
//! Output is 16-bit stereo at 44100 Hz, the rate Copperline mixes at.

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Arc;

use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};

const SAMPLE_RATE: i32 = 44_100;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: ab-render SOUNDFONT.sf2 IN.mid OUT.wav");
        std::process::exit(2);
    }

    let sound_font = Arc::new(SoundFont::new(&mut File::open(&args[1]).expect("soundfont")).expect("parse soundfont"));
    let midi = Arc::new(MidiFile::new(&mut File::open(&args[2]).expect("midi file")).expect("parse midi"));

    let settings = SynthesizerSettings::new(SAMPLE_RATE);
    let synthesizer = Synthesizer::new(&sound_font, &settings).expect("synthesizer");
    let mut sequencer = MidiFileSequencer::new(synthesizer);
    sequencer.play(&midi, false);

    // The whole song plus a second of tail for releases and reverb.
    let frames = ((midi.get_length() + 1.0) * SAMPLE_RATE as f64) as usize;
    let mut left = vec![0f32; frames];
    let mut right = vec![0f32; frames];
    sequencer.render(&mut left, &mut right);

    write_wav_16(&args[3], &left, &right).expect("write wav");
    eprintln!(
        "{}: {:.1}s, peak {:.3}",
        args[3],
        frames as f64 / SAMPLE_RATE as f64,
        left.iter()
            .chain(right.iter())
            .fold(0f32, |a, s| a.max(s.abs()))
    );
}

/// Minimal 16-bit PCM stereo WAV writer; no dependency needed for a rig.
fn write_wav_16(path: &str, left: &[f32], right: &[f32]) -> std::io::Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    let data_len = (left.len() * 4) as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&2u16.to_le_bytes())?; // stereo
    w.write_all(&(SAMPLE_RATE as u32).to_le_bytes())?;
    w.write_all(&(SAMPLE_RATE as u32 * 4).to_le_bytes())?;
    w.write_all(&4u16.to_le_bytes())?; // block align
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for (l, r) in left.iter().zip(right) {
        for s in [l, r] {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            w.write_all(&v.to_le_bytes())?;
        }
    }
    Ok(())
}
