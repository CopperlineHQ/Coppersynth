//! The translator, driven a byte at a time exactly as the serial line
//! will drive it: a Sierra-shaped session in miniature. Sysex framing,
//! running status, the pan flip, patch-memory tracking, the custom-name
//! matcher, rhythm identity and the auto-detect are each held by the
//! byte streams a real game produces.

use coppersynth::mt32::translator::{Event, Mt32Mode, Mt32Translator};

fn push_all(t: &mut Mt32Translator, bytes: &[u8]) -> Vec<Event> {
    let mut out = Vec::new();
    for &b in bytes {
        out.extend(t.push(b));
    }
    out
}

fn midi(command: u8, channel: u8, data1: u8, data2: u8) -> Event {
    Event::Midi {
        command,
        channel,
        data1,
        data2,
    }
}

/// A Roland DT1 write to the MT-32, checksummed like the real thing.
fn dt1(addr: u32, data: &[u8]) -> Vec<u8> {
    let mut body = vec![
        0xF0,
        0x41,
        0x10,
        0x16,
        0x12,
        ((addr >> 14) & 0x7F) as u8,
        ((addr >> 7) & 0x7F) as u8,
        (addr & 0x7F) as u8,
    ];
    body.extend_from_slice(data);
    let sum: u32 = body[5..].iter().map(|&b| b as u32).sum();
    body.push(((128 - (sum % 128)) % 128) as u8);
    body.push(0xF7);
    body
}

#[test]
fn off_mode_passes_channel_messages_untouched() {
    let mut t = Mt32Translator::new(Mt32Mode::Off);
    let out = push_all(&mut t, &[0x91, 60, 100, 0xB1, 0x0A, 0]);
    assert_eq!(
        out,
        vec![midi(0x90, 1, 60, 100), midi(0xB0, 1, 0x0A, 0)],
        "no pan flip, no shifts, nothing"
    );
}

#[test]
fn running_status_is_understood() {
    let mut t = Mt32Translator::new(Mt32Mode::Off);
    // One status byte, three note-ons.
    let out = push_all(&mut t, &[0x90, 60, 100, 64, 100, 67, 100]);
    assert_eq!(out.len(), 3);
    assert_eq!(out[2], midi(0x90, 0, 67, 100));
}

#[test]
fn translation_maps_programs_through_the_patch_table() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    // Patch 12 is Pipe Org 1 at power-on; Sierra says church organ.
    let out = push_all(&mut t, &[0xC1, 12]);
    assert!(
        out.contains(&midi(0xC0, 1, 19, 0)),
        "Pipe Org 1 must arrive as Church Organ: {out:?}"
    );
}

#[test]
fn the_pan_flip_only_applies_when_translating() {
    let mut on = Mt32Translator::new(Mt32Mode::On);
    let out = push_all(&mut on, &[0xB1, 0x0A, 0x00]);
    assert!(out.contains(&midi(0xB0, 1, 0x0A, 0x7F)), "{out:?}");
    let out = push_all(&mut on, &[0xB1, 0x0A, 0x40]);
    assert!(
        out.contains(&midi(0xB0, 1, 0x0A, 0x40)),
        "centre stays centred"
    );
}

#[test]
fn activation_sets_the_mt32_bend_range() {
    // Construction in On mode queues the activation burst; the first
    // push -- any byte, even a real-time one -- hands it over, so the
    // ranges are on the wire before any note can be.
    let mut t = Mt32Translator::new(Mt32Mode::On);
    let first: Vec<Event> = t.push(0xF8).collect();
    for channel in 1..=8 {
        assert!(
            first.contains(&midi(0xB0, channel, 0x06, 12)),
            "bend range 12 reaches channel {}: {first:?}",
            channel + 1
        );
    }
    assert!(first.contains(&Event::Translating(true)));
}

#[test]
fn a_patch_memory_write_repoints_a_program() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    // Re-point patch 5 at preset group b timbre 3 ("Harp 1" territory):
    // group=1, number=3, keyshift 24 (no shift), fine 50, bend 2.
    let sysex = dt1(0x05 << 14 | 5 * 8, &[1, 3, 24, 50, 2, 0, 1, 0]);
    push_all(&mut t, &sysex);
    let out = push_all(&mut t, &[0xC2, 5]);
    let expected = coppersynth::mt32::tables::PATCH_TO_GM[64 + 3];
    assert!(
        out.contains(&midi(0xC0, 2, expected, 0)),
        "patch 5 now renders group b timbre 3: {out:?}"
    );
    assert!(
        out.contains(&midi(0xB0, 2, 0x06, 2)),
        "the patch's own bend range rides along: {out:?}"
    );
}

#[test]
fn a_custom_timbre_is_matched_by_its_uploaded_name() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    // Upload "Explode MS" into memory timbre 0 -- the name is the first
    // ten bytes of the timbre's common area.
    let sysex = dt1(0x08 << 14, b"Explode MS");
    push_all(&mut t, &sysex);
    // Point patch 0 at memory timbre 0 and select it.
    let sysex = dt1(0x05 << 14, &[2, 0, 24, 50, 12, 0, 1, 0]);
    push_all(&mut t, &sysex);
    let out = push_all(&mut t, &[0xC3, 0]);
    assert!(
        out.contains(&midi(0xC0, 3, 127, 0)),
        "Sierra's explosion is a gunshot: {out:?}"
    );
}

#[test]
fn an_unknown_custom_name_lands_on_a_preset_by_substring() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    let sysex = dt1(0x08 << 14 | 256, b"xFrHorn1xx");
    push_all(&mut t, &sysex);
    let sysex = dt1(0x05 << 14, &[2, 1, 24, 50, 12, 0, 1, 0]);
    push_all(&mut t, &sysex);
    let out = push_all(&mut t, &[0xC3, 0]);
    let horn = coppersynth::mt32::tables::PATCH_TO_GM[coppersynth::mt32::tables::PRESET_NAMES
        .iter()
        .position(|n| n.trim() == "Fr Horn 1")
        .unwrap()];
    assert!(out.contains(&midi(0xC0, 3, horn, 0)), "{out:?}");
}

/// The rhythm part ignores program changes, as the hardware does; on a
/// GM synth they would select drum kits the game never chose. Found by
/// the KQ5 intro capture, which sends five of them.
#[test]
fn rhythm_channel_program_changes_are_consumed() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    let _: Vec<Event> = t.push(0xF8).collect();
    let out = push_all(&mut t, &[0xC9, 7]);
    assert!(
        !out.iter()
            .any(|e| matches!(e, Event::Midi { command: 0xC0, .. })),
        "{out:?}"
    );
    // And the kit still sounds afterwards.
    let out = push_all(&mut t, &[0x99, 38, 100]);
    assert!(out.contains(&midi(0x90, 9, 38, 100)));
}

#[test]
fn rhythm_notes_pass_inside_the_kit_and_drop_outside() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    let out = push_all(&mut t, &[0x99, 38, 100]);
    assert!(out.contains(&midi(0x90, 9, 38, 100)), "{out:?}");
    let out = push_all(&mut t, &[0x99, 20, 100]);
    assert!(
        !out.iter()
            .any(|e| matches!(e, Event::Midi { command: 0x90, .. })),
        "a key below the kit stays silent: {out:?}"
    );
}

#[test]
fn note_off_lands_where_its_note_on_was_emitted() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    // A patch with keyshift +12 (stored 36).
    let sysex = dt1(0x05 << 14, &[0, 0, 36, 50, 12, 0, 1, 0]);
    push_all(&mut t, &sysex);
    push_all(&mut t, &[0xC1, 0]);
    let on = push_all(&mut t, &[0x91, 60, 100]);
    assert!(on.contains(&midi(0x90, 1, 72, 100)), "{on:?}");
    // The game then re-points the patch; the off must still land on 72.
    let sysex = dt1(0x05 << 14, &[0, 0, 24, 50, 12, 0, 1, 0]);
    push_all(&mut t, &sysex);
    push_all(&mut t, &[0xC1, 0]);
    let off = push_all(&mut t, &[0x81, 60, 0]);
    assert!(off.contains(&midi(0x80, 1, 72, 0)), "{off:?}");
}

#[test]
fn auto_mode_flips_on_mt32_sysex_and_off_on_gm_reset() {
    let mut t = Mt32Translator::new(Mt32Mode::Auto);
    assert!(!t.is_translating());
    let out = push_all(&mut t, &[0xB1, 0x0A, 0x00]);
    assert!(out.contains(&midi(0xB0, 1, 0x0A, 0x00)), "no flip yet");

    let sysex = dt1(0x05 << 14, &[0, 0, 24, 50, 12, 0, 1, 0]);
    let out = push_all(&mut t, &sysex);
    assert!(t.is_translating());
    assert!(out.contains(&Event::Translating(true)));
    let out = push_all(&mut t, &[0xB1, 0x0A, 0x00]);
    assert!(
        out.contains(&midi(0xB0, 1, 0x0A, 0x7F)),
        "now flipped: {out:?}"
    );

    let out = push_all(&mut t, &[0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7]);
    assert!(!t.is_translating());
    assert!(out.contains(&Event::Translating(false)));
}

#[test]
fn display_text_surfaces_and_sysex_never_leaks() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    // Drain the activation burst first; it is not part of what the
    // display write produces.
    let _: Vec<Event> = t.push(0xF8).collect();
    let sysex = dt1(0x20 << 14, b"Insert disk 2       ");
    let out = push_all(&mut t, &sysex);
    assert!(out.contains(&Event::Display("Insert disk 2".to_string())));
    assert!(
        !out.iter().any(|e| matches!(e, Event::Midi { .. })),
        "sysex bytes must never reach the synthesizer: {out:?}"
    );
}

#[test]
fn a_corrupt_checksum_is_dropped_whole() {
    let mut t = Mt32Translator::new(Mt32Mode::On);
    let mut sysex = dt1(0x05 << 14, &[1, 3, 24, 50, 2, 0, 1, 0]);
    let n = sysex.len();
    sysex[n - 2] ^= 0x40; // break the checksum
    push_all(&mut t, &sysex);
    let out = push_all(&mut t, &[0xC1, 0]);
    assert!(
        out.contains(&midi(
            0xC0,
            1,
            coppersynth::mt32::tables::PATCH_TO_GM[0],
            0
        )),
        "the write must not have applied: {out:?}"
    );
}
