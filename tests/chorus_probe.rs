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
    let mut e = engine()?;
    send(&mut e, &[0xC0, 48]); // strings
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
        .map(|(&(al, ar), &(bl, br))| {
            ((al - bl) as f64).powi(2) + ((ar - br) as f64).powi(2)
        })
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
}
