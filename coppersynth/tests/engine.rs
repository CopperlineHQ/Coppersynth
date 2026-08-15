//! The part layer between the wire and the synthesizer: routing, mute,
//! key shift, the volume knob, and the live read-back the front panel
//! shows. Every test skips quietly without the local soundfont, like
//! the listening rig.

use coppersynth::engine::{GmEngine, DRUM_PART, PARTS};
use coppersynth::mt32::translator::Mt32Mode;

fn engine(mode: Mt32Mode) -> Option<GmEngine> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/GeneralUser-GS.sf2");
    if !std::path::Path::new(path).is_file() {
        return None;
    }
    Some(GmEngine::open(std::path::Path::new(path), 44_100, mode).expect("engine opens"))
}

fn send(engine: &mut GmEngine, bytes: &[u8]) {
    for &b in bytes {
        engine.write_byte(b);
    }
}

fn render_rms(engine: &mut GmEngine, frames: usize) -> f32 {
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
