//! The part layer between the wire and the synthesizer: routing, mute,
//! key shift, the volume knob, and the live read-back the front panel
//! shows. Every test skips quietly without the local soundfont, like
//! the listening rig.

use coppersynth::engine::{Engine, DRUM_PART, PARTS};
use coppersynth::mt32::translator::Mt32Mode;

fn engine(mode: Mt32Mode) -> Option<Engine> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/GeneralUser-GS.sf2");
    if !std::path::Path::new(path).is_file() {
        return None;
    }
    Some(Engine::open(std::path::Path::new(path), 44_100, mode).expect("engine opens"))
}

fn send(engine: &mut Engine, bytes: &[u8]) {
    for &b in bytes {
        engine.write_byte(b);
    }
}

fn render_rms(engine: &mut Engine, frames: usize) -> f32 {
    let mut block = vec![(0f32, 0f32); frames];
    engine.render(&mut block);
    (block.iter().map(|(l, r)| l * l + r * r).sum::<f32>() / frames as f32).sqrt()
}

/// At its defaults the layer is a wire: a note on channel 3 sounds on
/// part 3 and nowhere else.
#[test]
fn parts_default_to_a_wire() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    send(&mut e, &[0x93, 60, 100]);
    assert!(render_rms(&mut e, 4410) > 0.001, "the note must sound");
    let activity = e.part_activity();
    assert!(activity[3] > 0.0, "part 3 carries the note");
    for (part, level) in activity.iter().enumerate() {
        assert!(
            part == 3 || *level == 0.0,
            "part {part} should be idle, reads {level}"
        );
    }
}

/// Re-pointing a part's receive channel moves the traffic; the part
/// whose channel was taken away falls silent.
#[test]
fn rx_remap_moves_a_channel() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    e.set_part_rx_channel(4, Some(0));
    e.set_part_rx_channel(0, None);
    send(&mut e, &[0x90, 60, 100]);
    render_rms(&mut e, 4410);
    let activity = e.part_activity();
    assert!(activity[4] > 0.0, "part 4 listens on channel 1 now");
    assert_eq!(activity[0], 0.0, "part 0 is off the wire");
}

/// Two parts on one channel layer: both sound the same note.
#[test]
fn layered_parts_both_sound() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    e.set_part_rx_channel(1, Some(0));
    send(&mut e, &[0x90, 60, 100]);
    render_rms(&mut e, 4410);
    let activity = e.part_activity();
    assert!(
        activity[0] > 0.0 && activity[1] > 0.0,
        "both parts carry it"
    );
}

/// Mute silences what is sounding and gates what arrives; unmuting lets
/// notes through again.
#[test]
fn mute_gates_and_silences() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    // A dry room: muting silences the part, but a reverb tail would
    // ring past the gate, and it is the gate under test.
    e.set_part_reverb(0, 0);
    send(&mut e, &[0x90, 60, 100]);
    assert!(render_rms(&mut e, 4410) > 0.001);
    e.set_part_mute(0, true);
    // The release tail rings down; by half a second it is gone.
    render_rms(&mut e, 22_050);
    assert!(
        render_rms(&mut e, 4410) < 0.0005,
        "muting silences the part"
    );
    send(&mut e, &[0x90, 64, 100]);
    assert!(
        render_rms(&mut e, 4410) < 0.0005,
        "a muted part gates notes"
    );
    e.set_part_mute(0, false);
    send(&mut e, &[0x90, 64, 100]);
    assert!(
        render_rms(&mut e, 4410) > 0.001,
        "unmuted, notes sound again"
    );
}

/// A note that went in shifted comes off cleanly even when the shift
/// changes while it is held: no stuck notes.
#[test]
fn key_shift_survives_a_change_mid_note() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    e.set_part_key_shift(0, 12);
    send(&mut e, &[0x90, 60, 100]);
    assert!(render_rms(&mut e, 4410) > 0.001);
    e.set_part_key_shift(0, -12);
    send(&mut e, &[0x80, 60, 0]);
    // A grand piano's release is short; a stuck note would still be
    // ringing at full level two seconds on.
    render_rms(&mut e, 88_200);
    assert!(
        render_rms(&mut e, 4410) < 0.0005,
        "the note-off must find the shifted key"
    );
}

/// The drum part is never shifted: moving a kit's keys renames its
/// instruments rather than transposing them.
#[test]
fn drums_ignore_key_shift() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let unshifted = {
        let Some(mut e) = engine(Mt32Mode::Off) else {
            return;
        };
        send(&mut e, &[0x99, 38, 100]);
        let mut block = vec![(0f32, 0f32); 4410];
        e.render(&mut block);
        block
    };
    e.set_part_key_shift(DRUM_PART, 12);
    send(&mut e, &[0x99, 38, 100]);
    let mut block = vec![(0f32, 0f32); 4410];
    e.render(&mut block);
    assert_eq!(block, unshifted, "the snare stays a snare");
}

/// The VOLUME knob is a pot after the DAC: it scales the frames and
/// leaves the synth's own master volume alone.
#[test]
fn the_volume_knob_scales_the_output() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    send(&mut e, &[0x90, 60, 100]);
    let loud = render_rms(&mut e, 4410);
    assert!(loud > 0.001);
    e.set_output_gain(0.0);
    assert_eq!(render_rms(&mut e, 4410), 0.0, "the knob at zero is silence");
    e.set_output_gain(1.0);
    assert!(render_rms(&mut e, 4410) > 0.001, "and back up again");
}

/// The panel reads live truth: an edit or a wire message lands in the
/// same place, and the view shows whichever came last.
#[test]
fn part_view_reads_the_truth() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    // Church Organ via the wire, level via the panel's own setter.
    send(&mut e, &[0xC3, 19, 0xB3, 7, 64]);
    e.set_part_level(3, 90);
    let view = e.part_view(3);
    assert_eq!(view.instrument, 19);
    assert_eq!(view.level, 90);
    assert!(!view.name.is_empty(), "the soundfont names the preset");
    assert!(!view.drums);
    assert_eq!(view.rx_channel, Some(3));
    let drums = e.part_view(DRUM_PART);
    assert!(drums.drums);
    assert!(
        !drums.name.is_empty(),
        "the drum part reads its kit's name from bank 128"
    );
}

/// Translating, the part is called what an MT-32's display would call
/// it -- the factory timbre name, not the soundfont preset's.
#[test]
fn mt32_names_reach_the_panel() {
    let Some(mut e) = engine(Mt32Mode::On) else {
        return;
    };
    // MT-32 patch 12 on channel 2 (part 2): Pipe Org 1's slot.
    send(&mut e, &[0xC1, 12]);
    let name = e.part_view(1).name;
    assert_eq!(name, "Pipe Org 1", "the MT-32's own name for patch 12");
    // A panel edit takes the program over, and the glass names what is
    // actually loaded now -- not the timbre the game meant.
    e.set_part_instrument(1, 56);
    let view = e.part_view(1);
    assert_eq!(view.instrument, 56);
    assert_ne!(view.name, "Pipe Org 1");
    assert!(!view.name.is_empty(), "the soundfont names the new program");
    // And the game's next re-programming cannot take it back.
    send(&mut e, &[0xC1, 12]);
    assert_eq!(e.part_view(1).instrument, 56, "the panel's word stands");
}

/// Sound Canvas display text reaches the display queue in every mode,
/// translation off included.
#[test]
fn gs_display_text_lands() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    // F0 41 10 45 12 10 00 00 "HI" sum F7 -- model 45, the SC-55's
    // display -- checksum over addr+data.
    let payload: Vec<u8> = vec![0x10, 0x00, 0x00, b'H', b'I'];
    let sum = (128 - payload.iter().map(|&b| b as u32).sum::<u32>() % 128) % 128;
    let mut msg = vec![0xF0, 0x41, 0x10, 0x45, 0x12];
    msg.extend(&payload);
    msg.push(sum as u8);
    msg.push(0xF7);
    send(&mut e, &msg);
    assert_eq!(e.take_display(), vec!["HI".to_string()]);
}

/// LEVEL is a cap: the wire's volume passes under it, is held down by
/// it, and comes back when it lifts.
#[test]
fn the_level_cap_holds_the_wire_down() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    send(&mut e, &[0x90, 60, 100]);
    let open = render_rms(&mut e, 4410);
    assert!(open > 0.001);
    // Cap low: the same wire volume now sounds much quieter.
    e.set_part_level(0, 20);
    send(&mut e, &[0x90, 64, 100]);
    let capped = render_rms(&mut e, 4410);
    // Lift the cap: the wire's own volume is restored.
    e.set_part_level(0, 127);
    send(&mut e, &[0x90, 67, 100]);
    let lifted = render_rms(&mut e, 4410);
    assert!(capped < open * 0.6, "the cap holds the level down");
    assert!(lifted > capped * 2.0, "and lifting it lets the wire back");
    // The wire cannot climb over it either.
    e.set_part_level(0, 20);
    send(&mut e, &[0xB0, 7, 127, 0x90, 72, 100]);
    let pinned = render_rms(&mut e, 4410);
    assert!(pinned < open, "full wire volume stays under the cap");
}

/// A panel edit holds against the wire until the unit is reset: the
/// game can no more turn it back than it could on the desk.
#[test]
fn panel_edits_hold_against_the_wire() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    e.set_part_instrument(0, 19);
    e.set_part_reverb(0, 100);
    e.set_part_pan(0, 10);
    send(&mut e, &[0xC0, 5, 0xB0, 91, 3, 0xB0, 10, 64]);
    let view = e.part_view(0);
    assert_eq!(view.instrument, 19, "the program is the panel's");
    assert_eq!(view.reverb, 100);
    assert_eq!(view.pan, 10);
    // An untouched part still follows the wire.
    send(&mut e, &[0xC1, 5]);
    assert_eq!(e.part_view(1).instrument, 5);
    // Reset clears the locks; the wire speaks again.
    e.reset();
    send(&mut e, &[0xC0, 7]);
    assert_eq!(e.part_view(0).instrument, 7);
}

/// A slot the font never filled keeps its number and reads Empty --
/// the numbering must never shift on a sparse font.
#[test]
fn empty_slots_keep_their_numbers() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    // GeneralUser's drum bank is sparse: program 3 is not a kit.
    e.set_part_instrument(DRUM_PART, 3);
    let view = e.part_view(DRUM_PART);
    assert_eq!(view.instrument, 3, "the number is the slot asked for");
    assert_eq!(view.name, "Empty", "and the hole says what it is");
}

/// The effects respond across their whole range: 0 is dry, 127 is
/// unmistakably wet, and the GM default sits between. A woodblock is
/// over in an instant, so everything after the hit is the room.
#[test]
fn reverb_and_chorus_are_audible() {
    let probe = |reverb: u8, chorus: u8| -> (f32, f32) {
        let Some(mut e) = engine(Mt32Mode::Off) else {
            return (-1.0, -1.0);
        };
        e.set_part_reverb(0, reverb);
        e.set_part_chorus(0, chorus);
        send(&mut e, &[0xC0, 115, 0x90, 60, 110]);
        let held = render_rms(&mut e, 44_100);
        send(&mut e, &[0x80, 60, 0]);
        let ring = render_rms(&mut e, 22_050);
        (held, ring)
    };
    let (dry_held, dry_ring) = probe(0, 0);
    if dry_held < 0.0 {
        return;
    }
    let (mid_held, mid_ring) = probe(40, 0);
    let (wet_held, wet_ring) = probe(127, 0);
    assert!(
        wet_held > dry_held * 1.3,
        "full reverb must be unmistakable"
    );
    assert!(
        mid_held > dry_held * 1.05 && mid_held < wet_held,
        "the GM default sits between dry and drenched"
    );
    assert!(
        wet_ring > dry_ring * 5.0 && mid_ring > dry_ring * 2.0,
        "the room rings after the hit, more the more send"
    );
    // Chorus is a doubled, detuned voice, not a louder one: an 18 ms
    // wet copy barely moves a held note's RMS (the old 2 ms comb summed
    // coherently and did, which is what the previous assertion leaned
    // on). What proves the send is alive is wet content: the difference
    // between the chorus-off and chorus-full renders of the same
    // sustained note.
    let strings = |chorus: u8| -> Vec<(f32, f32)> {
        let Some(mut e) = engine(Mt32Mode::Off) else {
            return Vec::new();
        };
        e.set_part_chorus(0, chorus);
        send(&mut e, &[0xC0, 48, 0x90, 60, 110]);
        let mut out = vec![(0.0f32, 0.0f32); 44_100];
        e.render(&mut out);
        out
    };
    let (c0, c127) = (strings(0), strings(127));
    if !c0.is_empty() {
        let dry = rms_of(&c0);
        let wet = diff_rms_of(&c0, &c127);
        assert!(
            wet > dry * 0.25,
            "full chorus must put real wet under the note (wet {wet}, dry {dry})"
        );
    }
}

fn rms_of(buf: &[(f32, f32)]) -> f32 {
    (buf.iter().map(|&(l, r)| l * l + r * r).sum::<f32>() / (2.0 * buf.len() as f32)).sqrt()
}

fn diff_rms_of(a: &[(f32, f32)], b: &[(f32, f32)]) -> f32 {
    (a.iter()
        .zip(b)
        .map(|(&(al, ar), &(bl, br))| (al - bl) * (al - bl) + (ar - br) * (ar - br))
        .sum::<f32>()
        / (2.0 * a.len() as f32))
        .sqrt()
}

/// The bundled bank is really in there: no files, no configuration,
/// a note. The zip pipeline, the licence and the metadata all ride on
/// this working.
#[test]
fn the_bundled_bank_needs_no_files() {
    let mut e = Engine::open_bundled(44_100, Mt32Mode::Off).expect("the bundle opens");
    assert!(
        e.bank_name().starts_with("GeneralUser"),
        "the bank names itself: {:?}",
        e.bank_name()
    );
    send(&mut e, &[0x90, 60, 100]);
    assert!(render_rms(&mut e, 4410) > 0.001, "and it plays");
}

/// The chorus genuinely modulates: the wet signal's best-fit delay
/// against the dry drifts across the LFO's period. A broken LFO would
/// leave a static comb -- louder on a meter, invisible to the ear.
#[test]
fn the_chorus_actually_swims() {
    let render = |chorus: u8| -> Option<Vec<f32>> {
        let mut e = engine(Mt32Mode::Off)?;
        e.set_part_chorus(0, chorus);
        // A church organ holds perfectly still.
        send(&mut e, &[0xC0, 19, 0x90, 60, 100]);
        let mut block = vec![(0f32, 0f32); 44_100 * 3];
        e.render(&mut block);
        Some(block.iter().map(|(l, _)| *l).collect())
    };
    let Some(dry) = render(0) else {
        return;
    };
    let wet: Vec<f32> = render(127)
        .unwrap()
        .iter()
        .zip(&dry)
        .map(|(w, d)| w - d)
        .collect();
    let best_lag = |centre: usize| -> isize {
        let mut best = (0isize, f32::MIN);
        for lag in 40..180isize {
            let mut sum = 0f32;
            for i in 0..1024 {
                sum += dry[centre + i] * wet[centre + i + lag as usize];
            }
            if sum > best.1 {
                best = (lag, sum);
            }
        }
        best.0
    };
    // Half the 0.4 Hz LFO period apart, the delay should have swum a
    // long way between its extremes.
    let early = best_lag(44_100);
    let late = best_lag(44_100 + 55_125);
    assert!(
        (early - late).abs() > 40,
        "the chorus delay must drift with its LFO: {early} vs {late}"
    );
}

/// The layer's bounds hold: parts count, and out-of-range accessors
/// answer rather than panic.
#[test]
fn the_edges_are_calm() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    assert_eq!(PARTS, 16);
    e.set_part_mute(PARTS + 4, true);
    e.set_part_key_shift(PARTS + 4, 3);
    e.set_part_rx_channel(PARTS + 4, Some(2));
    let view = e.part_view(3);
    assert_eq!(view.muted, false);
}

/// MT-32 traffic selects the CM-64/32L kit when the font carries one
/// -- GeneralUser does -- and a GM reset puts Standard back when auto
/// detection stands down with it.
#[test]
fn mt32_traffic_selects_the_cm64_kit() {
    let Some(mut e) = engine(Mt32Mode::Auto) else {
        return;
    };
    assert_eq!(e.part_view(DRUM_PART).instrument, 0, "Standard until told");
    // An MT-32 sysex identifies the traffic; the kit follows.
    let body = [0x20u8, 0x00, 0x00, b'H', b'I'];
    let sum = (128 - body.iter().map(|&b| b as u32).sum::<u32>() % 128) % 128;
    let mut msg = vec![0xF0, 0x41, 0x10, 0x16, 0x12];
    msg.extend(body);
    msg.push(sum as u8);
    msg.push(0xF7);
    send(&mut e, &msg);
    assert!(e.translating());
    assert_eq!(
        e.part_view(DRUM_PART).instrument,
        127,
        "the CM-64/32L kit is in for MT-32 traffic"
    );
    // The kit's own keys sound; whether the font voices the CM-32L
    // extras is its business -- the range plumbing is the translator
    // test's to prove.
    send(&mut e, &[0x99, 38, 100]);
    let mut block = vec![(0f32, 0f32); 4410];
    e.render(&mut block);
    assert!(e.part_activity()[DRUM_PART] > 0.0, "the snare sounds");
    // A GM reset stands translation down and Standard returns.
    send(&mut e, &[0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7]);
    assert!(!e.translating());
    assert_eq!(e.part_view(DRUM_PART).instrument, 0, "Standard is back");
}
