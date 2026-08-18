//! Temporary measurement probe: how much does CC91/CC93 actually change
//! the rendered audio? Run with --nocapture for the numbers.
use coppersynth::engine::{Engine, Mt32Mode};

fn engine() -> Option<Engine> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/GeneralUser-GS.sf2");
    if !std::path::Path::new(path).is_file() {
        return None;
    }
    Some(Engine::open(std::path::Path::new(path), 44_100, Mt32Mode::Off).expect("engine opens"))
}

fn send(e: &mut Engine, bytes: &[u8]) {
    for &b in bytes {
        e.write_byte(b);
    }
}

/// Render a few seconds of a sustained pad with the given controller
/// value, return the whole buffer.
fn take(cc: u8, value: u8) -> Option<Vec<(f32, f32)>> {
    take_program(48, cc, value) // strings
}

fn take_program(program: u8, cc: u8, value: u8) -> Option<Vec<(f32, f32)>> {
    let mut e = engine()?;
    send(&mut e, &[0xC0, program]);
    send(&mut e, &[0xB0, cc, value]);
    send(&mut e, &[0x90, 60, 100]);
    let mut out = vec![(0.0f32, 0.0f32); 44_100 * 3];
    e.render(&mut out);
    Some(out)
}

fn rms(buf: &[(f32, f32)]) -> f64 {
    (buf.iter()
        .map(|&(l, r)| (l as f64).powi(2) + (r as f64).powi(2))
        .sum::<f64>()
        / (2.0 * buf.len() as f64))
        .sqrt()
}

fn diff_rms(a: &[(f32, f32)], b: &[(f32, f32)]) -> f64 {
    (a.iter()
        .zip(b)
        .map(|(&(al, ar), &(bl, br))| ((al - bl) as f64).powi(2) + ((ar - br) as f64).powi(2))
        .sum::<f64>()
        / (2.0 * a.len() as f64))
        .sqrt()
}

#[test]
fn measure_effect_sends() {
    let (Some(r0), Some(r127)) = (take(91, 0), take(91, 127)) else {
        return;
    };
    let (Some(c0), Some(c127)) = (take(93, 0), take(93, 127)) else {
        return;
    };
    println!("dry rms          : {:.6}", rms(&r0));
    println!("reverb 0 vs 127  : diff rms {:.6}", diff_rms(&r0, &r127));
    println!("chorus 0 vs 127  : diff rms {:.6}", diff_rms(&c0, &c127));
    // The woodblock ratio the engine's audibility test leans on.
    if let (Some(w0), Some(w127)) = (take_program(115, 93, 0), take_program(115, 93, 127)) {
        println!(
            "woodblock chorus : rms {:.6} -> {:.6} (x{:.3})",
            rms(&w0),
            rms(&w127),
            rms(&w127) / rms(&w0)
        );
    }
}

/// Write the takes out as WAVs for a human audition. Ignored: it is a
/// listening aid, not an assertion. COPPERSYNTH_AUDITION_DIR names the
/// output directory.
#[test]
#[ignore]
fn render_audition_wavs() {
    let Some(dir) = std::env::var_os("COPPERSYNTH_AUDITION_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    for (name, program, value) in [
        ("piano-chorus-off.wav", 0, 0),
        ("piano-chorus-full.wav", 0, 127),
        ("guitar-chorus-off.wav", 24, 0),
        ("guitar-chorus-full.wav", 24, 127),
    ] {
        let Some(buf) = take_program(program, 93, value) else {
            return;
        };
        write_wav(&dir.join(name), &buf);
    }
}

fn write_wav(path: &std::path::Path, frames: &[(f32, f32)]) {
    let mut data = Vec::with_capacity(frames.len() * 4);
    for &(l, r) in frames {
        for v in [l, r] {
            let s = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
            data.extend_from_slice(&s.to_le_bytes());
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out).expect("wav written");
}
