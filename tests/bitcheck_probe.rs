//! An oracle for refactors that must not change a sample.
//!
//! It drives every event kind the dispatch knows -- notes across the
//! whole key range, every CC the unit reads, pitch-bend sweeps, RPN and
//! NRPN data entry, portamento, mono mode, a drum part with per-note
//! NRPN edits, poly and channel pressure -- renders the lot, and hashes
//! every sample of both channels.
//!
//! Run it on the commit before a refactor and on the commit after: the
//! hash must not move. It is `#[ignore]`d and prints rather than
//! asserts, because the number is only comparable against itself on one
//! machine and toolchain -- the envelope maths goes through the
//! platform's own `exp`, which is not required to agree bit for bit
//! between libms.
//!
//! ```text
//! cargo test --test bitcheck_probe -- --ignored --nocapture
//! ```
use coppersynth::synth::{SoundFont, Synthesizer, SynthesizerSettings};
use std::fs::File;
use std::sync::Arc;

#[test]
#[ignore = "an oracle for comparing one commit against another, not a check"]
fn bitcheck() {
    let mut file = File::open("assets/GeneralUser-GS.sf2").unwrap();
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let settings = SynthesizerSettings::new(44100);
    let mut synth = Synthesizer::new(&font, &settings).unwrap();

    let mut events: Vec<(u32, u8, u8, u8, u8)> = Vec::new();
    let mut t = 0u32;
    // Notes over the whole range on several parts, odd velocities.
    for (i, key) in [0u8, 1, 27, 59, 60, 61, 96, 126, 127].iter().enumerate() {
        let ch = (i % 4) as u8;
        events.push((t, ch, 0x90, *key, 1 + (i as u8).wrapping_mul(17) % 127));
        t += 3;
        events.push((t, ch, 0x80, *key, 64));
        t += 2;
    }
    // The CC set the dispatch reads, plus a few unnamed ones.
    for cc in [
        0u8, 1, 5, 6, 7, 10, 11, 33, 38, 39, 42, 43, 64, 65, 66, 67, 84, 91, 93, 98, 99, 100, 101,
        120, 121, 123,
    ] {
        events.push((t, 1, 0xB0, cc, cc.wrapping_mul(3) % 128));
        t += 1;
    }
    // Program + bank, pressure, bend sweep.
    events.push((t, 1, 0xC0, 39, 0));
    events.push((t + 1, 1, 0xA0, 60, 90));
    events.push((t + 2, 1, 0xD0, 77, 0));
    for (i, b) in [(0u32, 0u8), (1, 64), (2, 127)].iter() {
        events.push((t + 3 + i, 1, 0xE0, *b, *b));
    }
    t += 8;
    // RPN 0 (bend range), RPN 2 (coarse tune), then an NRPN drum edit.
    for (c, v) in [
        (101u8, 0u8),
        (100, 0),
        (6, 12),
        (101, 0),
        (100, 2),
        (6, 40),
        (99, 0x18),
        (98, 60),
        (6, 100),
    ] {
        events.push((t, 9, 0xB0, c, v));
        t += 1;
    }
    // Portamento: source, then the note that glides.
    events.push((t, 2, 0xB0, 84, 48));
    events.push((t + 1, 2, 0x90, 72, 100));
    events.push((t + 30, 2, 0x80, 72, 64));
    // Mono mode with an overlap.
    events.push((t + 2, 3, 0xB0, 126, 1));
    events.push((t + 3, 3, 0x90, 40, 80));
    events.push((t + 5, 3, 0x90, 47, 80));
    events.push((t + 40, 3, 0x80, 47, 64));
    // The drum part.
    events.push((t + 6, 9, 0x90, 36, 120));
    events.push((t + 8, 9, 0x90, 38, 90));
    t += 50;

    let block = 64usize;
    let mut left = vec![0f32; block];
    let mut right = vec![0f32; block];
    let mut hash = 0xcbf29ce484222325u64;
    let mut eat = |v: f32| {
        for b in v.to_bits().to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    let mut ev = events.into_iter().peekable();
    for tick in 0..t + 400 {
        while ev.peek().is_some_and(|e| e.0 <= tick) {
            let (_, ch, cmd, d1, d2) = ev.next().unwrap();
            synth.process_midi_message(ch, cmd, d1, d2);
        }
        synth.render(&mut left, &mut right);
        for i in 0..block {
            eat(left[i]);
            eat(right[i]);
        }
    }
    println!("BITCHECK {hash:016x}");
}
