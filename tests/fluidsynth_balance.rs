//! How this unit's program-to-program balance compares with the
//! reference player's, on the same bank, note for note.
//!
//! The question it exists to answer is whether a change to the gain
//! path moves this synthesiser towards FluidSynth or away from it --
//! which is not a thing that can be judged by reading either one's
//! source, as [`ATTENUATION_WEIGHT`](../src/synth/voice.rs) records.
//!
//! It needs FluidSynth on the path (`brew install fluidsynth`) and is
//! ignored by default, since a test suite should not depend on another
//! synthesiser being installed. Both sides render dry, one note per
//! program at a fixed velocity, and only the *balance* is compared --
//! each series is centred on its own mean, because the absolute levels
//! depend on FluidSynth's `-g` and on this unit's own calibration, and
//! neither of those says anything about whether a voice is right.
//!
//! Each program is rendered **on its own**, one invocation apiece. The
//! first version of this laid all 128 out in one-second slots in a
//! single file, which is quicker and quietly wrong: a release runs on
//! past its slot, so every window held the tail of the program before
//! it, and a program whose own voice is quiet or short measured as
//! whatever preceded it. It reported five programs as far out that are
//! not out at all -- Gunshot carrying Applause's seven-second tail,
//! Helicopter carrying Telephone 1's, Sitar carrying Star Theme's nine.
//! One note, one render, no neighbours.
//!
//! ```text
//! cargo test --test fluidsynth_balance -- --ignored --nocapture
//! ```
use coppersynth::engine::{Engine, Mt32Mode};
use std::process::Command;

const BANK: &str = "assets/GeneralUser-GS.sf2";
const NOTE: u8 = 60;
const VELOCITY: u8 = 100;
const RATE: usize = 44_100;

/// One second of one note, dry, as this unit renders it.
fn ours(program: u8) -> Option<f32> {
    let mut e = Engine::open_bundled(RATE as u32, Mt32Mode::Off).ok()?;
    e.set_part_reverb(0, 0);
    e.set_part_chorus(0, 0);
    for b in [0xC0, program, 0x90, NOTE, VELOCITY] {
        e.write_byte(b);
    }
    let mut out = vec![(0.0f32, 0.0f32); RATE];
    e.render(&mut out);
    Some(rms(out.iter().flat_map(|&(l, r)| [l, r])))
}

fn rms(samples: impl Iterator<Item = f32>) -> f32 {
    let (sum, n) = samples.fold((0.0f64, 0usize), |(s, n), x| {
        (s + f64::from(x) * f64::from(x), n + 1)
    });
    if n == 0 {
        return 0.0;
    }
    (sum / n as f64).sqrt() as f32
}

/// One program, one note, held for the whole second and never released
/// -- the same thing [`ours`] renders, so the two sides are the same
/// experiment.
fn write_one_note_midi(path: &std::path::Path, program: u8) -> std::io::Result<()> {
    fn vlq(mut n: u32) -> Vec<u8> {
        let mut out = vec![(n & 0x7F) as u8];
        n >>= 7;
        while n != 0 {
            out.push(((n & 0x7F) | 0x80) as u8);
            n >>= 7;
        }
        out.reverse();
        out
    }
    // 480 ticks a quarter at 120 bpm: a second is 960 ticks.
    let mut ev: Vec<u8> = Vec::new();
    ev.extend(vlq(0));
    ev.extend([0xC0, program]);
    ev.extend(vlq(0));
    ev.extend([0x90, NOTE, VELOCITY]);
    ev.extend(vlq(960));
    ev.extend([0xFF, 0x2F, 0x00]);
    let mut out: Vec<u8> = Vec::new();
    out.extend(b"MThd");
    out.extend(6u32.to_be_bytes());
    out.extend([0u8, 0, 0, 1, 1, 224]); // format 0, one track, 480 ticks
    out.extend(b"MTrk");
    out.extend((ev.len() as u32).to_be_bytes());
    out.extend(&ev);
    std::fs::write(path, out)
}

/// The same note, as FluidSynth renders it, on its own.
fn theirs(dir: &std::path::Path, program: u8) -> Option<f32> {
    let midi = dir.join(format!("one_{program}.mid"));
    let wav = dir.join(format!("one_{program}.wav"));
    write_one_note_midi(&midi, program).ok()?;
    let out = Command::new("fluidsynth")
        .args(["-ni", "-F"])
        .arg(&wav)
        .args(["-r", "44100", "-R", "0", "-C", "0", "-g", "1.0", BANK])
        .arg(&midi)
        .output()
        .ok()?;
    if !out.status.success() || !wav.exists() {
        return None;
    }
    let raw = std::fs::read(&wav).ok()?;
    // Skip the RIFF header to the data chunk, then read 16-bit stereo.
    let at = raw.windows(4).position(|w| w == b"data")? + 8;
    let frames: Vec<f32> = raw[at..]
        .chunks_exact(2)
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
        .take(RATE * 2)
        .collect();
    Some(rms(frames.into_iter()))
}

#[test]
#[ignore = "needs fluidsynth on the path"]
fn the_balance_matches_the_reference_player() {
    let dir = std::env::temp_dir().join("coppersynth-balance");
    let _ = std::fs::create_dir_all(&dir);
    let Some(first) = theirs(&dir, 0) else {
        println!("fluidsynth not available, or it would not render; nothing compared");
        return;
    };
    let reference: Vec<f32> = std::iter::once(first)
        .chain((1u8..128).map(|p| theirs(&dir, p).unwrap_or(0.0)))
        .collect();
    let Some(first) = ours(0) else {
        println!("no bundled bank in this build; nothing compared");
        return;
    };
    let mine: Vec<f32> = std::iter::once(first)
        .chain((1u8..128).map(|p| ours(p).unwrap_or(0.0)))
        .collect();

    let live: Vec<usize> = (0..128)
        .filter(|&i| reference[i] > 1e-6 && mine[i] > 1e-9)
        .collect();
    let db = |x: f32| 20.0 * x.log10();
    let centred = |v: &[f32]| {
        let vals: Vec<f32> = live.iter().map(|&i| db(v[i])).collect();
        let mean = vals.iter().sum::<f32>() / vals.len() as f32;
        vals.into_iter().map(|x| x - mean).collect::<Vec<_>>()
    };
    let (r, m) = (centred(&reference), centred(&mine));
    let errs: Vec<f32> = r.iter().zip(&m).map(|(a, b)| b - a).collect();
    let rms_err = (errs.iter().map(|e| e * e).sum::<f32>() / errs.len() as f32).sqrt();
    let within = errs.iter().filter(|e| e.abs() <= 2.0).count();
    let worst = errs
        .iter()
        .cloned()
        .fold(0.0f32, |w, e| if e.abs() > w.abs() { e } else { w });
    println!(
        "balance over {} programs: rms {rms_err:.2} dB, worst {worst:+.1} dB, within 2 dB: {within}/{}",
        live.len(),
        live.len()
    );
    assert!(
        rms_err < 2.0,
        "this unit has drifted from the reference player's balance: rms {rms_err:.2} dB"
    );
}
