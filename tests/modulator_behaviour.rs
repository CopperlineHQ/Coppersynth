//! Behavioural proof that bank modulators are applied: GeneralUser's
//! "Synth Bass 2" builds its sound out of velocity-to-cutoff and
//! velocity-to-start-offset modulators, so velocity must change the
//! *brightness* of the note, not just its level. Without modulator
//! support both renders differ only in gain and this fails.
//!
//! Skips without the local soundfont, like the ignored suites elsewhere.

use std::fs::File;
use std::sync::Arc;

use coppersynth::synth::{SoundFont, Synthesizer, SynthesizerSettings};

fn render_note(font: &Arc<SoundFont>, velocity: i32) -> (Vec<f32>, Vec<f32>) {
    let settings = SynthesizerSettings::new(44100);
    let mut synth = Synthesizer::new(font, &settings).unwrap();
    // Program 39: Synth Bass 2, the fixture patch. A low E, half a second.
    synth.process_midi_message(0, 0xC0, 39, 0);
    synth.note_on(0, 28, velocity);
    let mut left = vec![0f32; 22050];
    let mut right = vec![0f32; 22050];
    synth.render(&mut left, &mut right);
    (left, right)
}

fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()
}

/// Zero-crossing rate as a cheap brightness measure: a closed lowpass
/// yields far fewer crossings than an open one.
fn zcr(s: &[f32]) -> f32 {
    let crossings = s
        .windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count();
    crossings as f32 / s.len() as f32
}

#[test]
fn velocity_opens_the_synth_bass_filter() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/GeneralUser-GS.sf2");
    let Ok(mut file) = File::open(path) else {
        return;
    };
    let font = Arc::new(SoundFont::new(&mut file).unwrap());

    let (soft, _) = render_note(&font, 30);
    let (hard, _) = render_note(&font, 127);

    let soft_rms = rms(&soft);
    let hard_rms = rms(&hard);
    assert!(
        soft_rms > 0.0 && hard_rms > 0.0,
        "both notes must sound at all: soft {soft_rms}, hard {hard_rms}"
    );
    assert!(
        hard_rms > soft_rms,
        "velocity must still raise the level: soft {soft_rms}, hard {hard_rms}"
    );

    // The point of the patch: a hard note is brighter, not merely louder.
    // The margin is deliberately wide -- the filter swings thousands of
    // cents -- so ordinary DSP drift cannot pass or fail this by luck.
    let soft_zcr = zcr(&soft);
    let hard_zcr = zcr(&hard);
    assert!(
        hard_zcr > 1.5 * soft_zcr,
        "velocity must open the filter: soft zcr {soft_zcr}, hard zcr {hard_zcr}"
    );
}

/// The same MIDI twice renders byte-identically: the modulator path must
/// not introduce any nondeterminism.
#[test]
fn modulated_rendering_stays_deterministic() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/GeneralUser-GS.sf2");
    let Ok(mut file) = File::open(path) else {
        return;
    };
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let (a, _) = render_note(&font, 100);
    let (b, _) = render_note(&font, 100);
    assert_eq!(a, b);
}
