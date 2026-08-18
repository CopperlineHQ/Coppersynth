//! The front panel: what its buttons do to the engine, and what the
//! glass shows for it. All timing is explicit milliseconds, so every
//! screen here is deterministic. Skips quietly without the local
//! soundfont.

use coppersynth::engine::{Engine, Monitor, Mt32Mode, DRUM_PART};
use coppersynth::panel::{Button, Dir, FrontPanel, Pair, PanelRequest};

fn engine(mode: Mt32Mode) -> Option<Engine> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/GeneralUser-GS.sf2");
    if !std::path::Path::new(path).is_file() {
        return None;
    }
    Some(Engine::open(std::path::Path::new(path), 44_100, mode).expect("engine opens"))
}

/// Draw once at zero to start the boot line, then step past it.
fn settled(panel: &mut FrontPanel, engine: &mut Engine) {
    panel.screen(engine, 0);
    panel.screen(engine, 3_100);
}

fn send(engine: &mut Engine, bytes: &[u8]) {
    for &b in bytes {
        engine.write_byte(b);
    }
}

/// Sysex framing helper: model, address, data, correct checksum.
fn dt1(model: u8, addr: [u8; 3], data: &[u8]) -> Vec<u8> {
    let mut msg = vec![0xF0, 0x41, 0x10, model, 0x12];
    msg.extend(addr);
    msg.extend(data);
    let sum: u32 = addr.iter().chain(data).map(|&b| b as u32).sum();
    msg.push(((128 - (sum % 128)) % 128) as u8);
    msg.push(0xF7);
    msg
}

/// Power-on: COPPERSYNTH holds the glass, the soundfont introduces
/// itself by its own name, and the part screen follows with the LEVEL
/// cap wide open.
#[test]
fn the_boot_line_greets_and_names_the_bank() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    let greeting = panel.screen(&mut e, 0);
    assert_eq!(greeting.name, "COPPERSYNTH");
    assert_eq!(greeting.part, "---", "an empty slot wears dashes");
    assert_eq!(greeting.instrument, "---");
    let bank = panel.screen(&mut e, 1_600);
    let expect: String = e.bank_name().chars().take(20).collect();
    assert!(!expect.is_empty(), "GeneralUser names itself");
    assert_eq!(bank.name, expect);
    let home = panel.screen(&mut e, 3_100);
    assert_eq!(home.part, "01");
    assert_eq!(home.instrument, "001");
    assert!(!home.name.is_empty());
    assert_eq!(home.level, "127", "the cap starts wide open");
    assert_eq!(home.pan, "0");
    assert_eq!(home.reverb, "40");
    assert_eq!(home.chorus, "0");
    assert_eq!(home.key_shift, "0");
    assert_eq!(home.midi_ch, "01");
}

/// PART arrows walk 1..16 and wrap; the display follows.
#[test]
fn part_arrows_walk_and_wrap() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Left));
    assert_eq!(panel.screen(&mut e, 4_000).part, "16");
    panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Right));
    panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Right));
    assert_eq!(panel.screen(&mut e, 4_000).part, "02");
}

/// INSTRUMENT arrows change the program; on the drum part they step
/// the ten sets and the name wears the asterisk.
#[test]
fn instrument_arrows_step_programs_and_kits() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::Arrow(Pair::Instrument, Dir::Right));
    assert_eq!(e.part_view(0).instrument, 1);
    assert_eq!(panel.screen(&mut e, 4_000).instrument, "002");
    for _ in 0..9 {
        panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Right));
    }
    let screen = panel.screen(&mut e, 4_000);
    assert_eq!(screen.part, "10");
    assert!(screen.name.starts_with('*'), "a drum set wears the star");
    // The arrow lands on the next kit the font actually carries.
    let next = e.neighbour_kit(1).expect("kits exist");
    assert_ne!(next, 0, "the font carries more than one kit");
    panel.button(&mut e, Button::Arrow(Pair::Instrument, Dir::Right));
    assert_eq!(e.part_view(DRUM_PART).instrument, next);
}

/// A sparse font cycles through the programs it loaded and skips the
/// numbers between; a full circuit returns home. The drum bank is
/// sparse by nature, so it is the natural place to prove it.
#[test]
fn cycling_walks_the_fonts_own_list() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let start = e.part_view(DRUM_PART).instrument;
    let mut seen = vec![start];
    let mut at = start;
    for _ in 0..129 {
        at = e.neighbour_kit(1).expect("kits exist");
        e.set_part_instrument(DRUM_PART, at);
        if at == start {
            break;
        }
        assert!(!seen.contains(&at), "a circuit never repeats mid-way");
        seen.push(at);
    }
    assert_eq!(at, start, "the circuit comes home");
    assert!(seen.len() > 1, "and visited the font's other kits");
}

/// Value arrows edit the selected part in the synthesizer itself; a
/// panel edit then holds against the wire.
#[test]
fn value_arrows_edit_and_hold() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::Arrow(Pair::Level, Dir::Left));
    panel.button(&mut e, Button::Arrow(Pair::Pan, Dir::Left));
    panel.button(&mut e, Button::Arrow(Pair::Reverb, Dir::Right));
    panel.button(&mut e, Button::Arrow(Pair::KeyShift, Dir::Left));
    panel.button(&mut e, Button::Arrow(Pair::MidiCh, Dir::Right));
    let view = e.part_view(0);
    assert_eq!(view.level, 126, "LEVEL is the cap");
    assert_eq!(view.pan, 63);
    assert_eq!(view.reverb, 41);
    assert_eq!(view.key_shift, -1);
    assert_eq!(view.rx_channel, Some(1));
    let screen = panel.screen(&mut e, 4_000);
    assert_eq!(screen.pan, "L1");
    assert_eq!(screen.key_shift, "-1");
    assert_eq!(screen.midi_ch, "02");
    // The game re-programs its channels; the panel's word stands.
    send(&mut e, &[0xB0, 10, 20, 0xB0, 91, 5]);
    let view = e.part_view(0);
    assert_eq!(view.pan, 63, "pan is locked to the panel's setting");
    assert_eq!(view.reverb, 41, "reverb too");
    // Sixteen steps later the channel has been through Off and round.
    for _ in 0..15 {
        panel.button(&mut e, Button::Arrow(Pair::MidiCh, Dir::Right));
    }
    assert_eq!(panel.screen(&mut e, 4_000).midi_ch, "---");
    panel.button(&mut e, Button::Arrow(Pair::MidiCh, Dir::Right));
    assert_eq!(panel.screen(&mut e, 4_000).midi_ch, "01");
}

/// ALL mode: the line reads ALL, and the pairs set every part at once
/// -- uniform values read out, mixed ones shrug.
#[test]
fn all_mode_broadcasts_to_every_part() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::All);
    let screen = panel.screen(&mut e, 4_000);
    assert_eq!(screen.part, "ALL");
    assert!(screen.all_led);
    let bank: String = e.bank_name().chars().take(20).collect();
    assert_eq!(screen.name, bank, "ALL introduces the loaded bank");
    assert_eq!(screen.level, "127");
    assert_eq!(screen.midi_ch, "---", "no channel speaks for all parts");
    panel.button(&mut e, Button::Arrow(Pair::Level, Dir::Left));
    panel.button(&mut e, Button::Arrow(Pair::KeyShift, Dir::Right));
    for p in [0, 7, 15] {
        assert_eq!(e.part_view(p).level, 126, "the cap lands on part {p}");
        assert_eq!(e.part_view(p).key_shift, 1);
    }
    assert_eq!(panel.screen(&mut e, 4_000).level, "126");
    // One part out of step and the read-out shrugs.
    e.set_part_reverb(2, 90);
    assert_eq!(panel.screen(&mut e, 4_000).reverb, "---");
    // The MIDI CH pair sits inert in ALL mode: no channel speaks for
    // sixteen parts, and the read-out says so with dashes.
    panel.button(&mut e, Button::Arrow(Pair::MidiCh, Dir::Left));
    assert_eq!(e.device_id(), 17, "the device ID is not a fascia setting");
    assert_eq!(panel.screen(&mut e, 4_000).midi_ch, "---");
}

/// MUTE gates the shown part and the screen says so; in ALL mode it
/// takes everything down at once.
#[test]
fn mute_by_part_and_all_at_once() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::Mute);
    assert!(e.part_view(0).muted);
    assert!(panel.screen(&mut e, 4_000).mute_led);
    let bars = panel.screen(&mut e, 4_000).bars;
    assert_eq!(bars[0] & 1, 0, "a muted part loses its baseline dot");
    assert_eq!(bars[1] & 1, 1);
    panel.button(&mut e, Button::Mute);
    assert!(!e.part_view(0).muted);
    panel.button(&mut e, Button::All);
    panel.button(&mut e, Button::Mute);
    assert!((0..16).all(|p| e.part_muted(p)));
    panel.button(&mut e, Button::Mute);
    assert!((0..16).all(|p| !e.part_muted(p)));
}

/// ALL+MUTE monitors: the shown part solos, and moving the selection
/// moves the solo.
#[test]
fn monitor_solos_the_shown_part() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::Monitor);
    assert_eq!(e.monitor(), Monitor::Solo(0));
    send(&mut e, &[0x90, 60, 100, 0x91, 64, 100]);
    let mut block = vec![(0f32, 0f32); 4410];
    e.render(&mut block);
    let activity = e.part_activity();
    assert!(activity[0] > 0.0, "the monitored part sounds");
    assert_eq!(activity[1], 0.0, "the other part is held");
    // The PART pair is the exception to the stand-down rule: the solo
    // travels with the selection.
    panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Right));
    assert_eq!(e.monitor(), Monitor::Solo(1), "the solo follows the part");
    panel.button(&mut e, Button::Mute);
    assert_eq!(e.monitor(), Monitor::Off);
}

/// INSTRUMENT ◄ held through the power-on asks the MT-32 question,
/// lamps flashing; ALL answers yes and the choice reaches the host.
#[test]
fn the_mt32_prompt_enables_translation() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    panel.power_on_held(&[Button::Arrow(Pair::Instrument, Dir::Left)]);
    let prompt = panel.screen(&mut e, 0);
    assert_eq!(prompt.name, "Init MT-32, Sure?");
    assert!(prompt.all_led, "the lamps flash on this half-second");
    assert!(prompt.mute_led);
    let off_beat = panel.screen(&mut e, 500);
    assert!(!off_beat.all_led, "and off on the other");
    assert!(!e.translating());
    let request = panel.button(&mut e, Button::All);
    assert_eq!(request, Some(PanelRequest::Mt32Mode(Mt32Mode::On)));
    assert!(e.translating(), "the unit is an MT-32 now");
    assert_eq!(panel.screen(&mut e, 1_000).name, "MT-32 Enabled");
    assert_eq!(panel.screen(&mut e, 3_200).part, "01", "the notice passes");
}

/// MUTE answers no: translation is off for good, and says so.
#[test]
fn the_mt32_prompt_disables_translation() {
    let Some(mut e) = engine(Mt32Mode::Auto) else {
        return;
    };
    let mut panel = FrontPanel::default();
    panel.power_on_held(&[Button::Arrow(Pair::Instrument, Dir::Left)]);
    panel.screen(&mut e, 0);
    let request = panel.button(&mut e, Button::Mute);
    assert_eq!(request, Some(PanelRequest::Mt32Mode(Mt32Mode::Off)));
    assert_eq!(panel.screen(&mut e, 500).name, "MT-32 Disabled");
    // Auto detection is off with it: MT-32 sysex no longer activates.
    send(&mut e, &dt1(0x16, [0x20, 0x00, 0x00], b"HELLO"));
    assert!(!e.translating());
}

/// Both MIDI CH halves and both INSTRUMENT halves: the version rides
/// the second line as documented -- and the credits roll over it,
/// lamps flashing, until ALL or MUTE lets the unit boot.
#[test]
fn the_debug_print_rolls_its_credits() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    panel.power_on_held(&[Button::Both(Pair::MidiCh), Button::Both(Pair::Instrument)]);
    let screen = panel.screen(&mut e, 0);
    let date = env!("COPPERSYNTH_RELEASE_DATE");
    let expect = if date.is_empty() {
        format!("v{}", env!("CARGO_PKG_VERSION"))
    } else {
        format!("v{} {date}", env!("CARGO_PKG_VERSION"))
    };
    assert_eq!(screen.name, "COPPERSYNTH made wit", "the roll at its head");
    assert_eq!(screen.subtitle, expect, "the version rides under it");
    assert_eq!(screen.part, "---");
    assert!(screen.all_led && screen.mute_led, "the lamps flash on");
    assert!(!panel.screen(&mut e, 500).all_led, "and off");
    // Once the head has rested, the roll walks a column at a time.
    assert_eq!(panel.screen(&mut e, 1_000).name, "OPPERSYNTH made with");
    // A value button is not a dismissal.
    panel.button(&mut e, Button::Arrow(Pair::Level, Dir::Right));
    assert_eq!(panel.screen(&mut e, 1_050).subtitle, expect);
    // ALL lets the boot go on: the greeting, the bank, then home.
    panel.button(&mut e, Button::All);
    assert_eq!(panel.screen(&mut e, 2_000).name, "COPPERSYNTH");
    let bank: String = e.bank_name().chars().take(20).collect();
    assert_eq!(panel.screen(&mut e, 3_600).name, bank);
    assert_eq!(panel.screen(&mut e, 5_100).part, "01");
}

/// INSTRUMENT ► held through the power-on asks the factory question:
/// ALL initialises -- the reset request reaches the host, the screen
/// holds a second -- and the unit boots as on any morning.
#[test]
fn the_font_prompt_initialises_after_a_pause() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    panel.power_on_held(&[Button::Arrow(Pair::Instrument, Dir::Right)]);
    let prompt = panel.screen(&mut e, 0);
    assert_eq!(prompt.name, "Init SoundFont,Sure?");
    assert!(prompt.all_led && prompt.mute_led, "the lamps flash on");
    assert!(!panel.screen(&mut e, 500).all_led, "and off");
    let request = panel.button(&mut e, Button::All);
    assert_eq!(request, Some(PanelRequest::ResetSoundfont));
    let held = panel.screen(&mut e, 1_000);
    assert_eq!(held.name, "Initializing...");
    assert_eq!(held.part, "---");
    // Buttons wait while it says so.
    assert_eq!(panel.button(&mut e, Button::Mute), None);
    assert_eq!(panel.screen(&mut e, 1_900).name, "Initializing...");
    // Its second served, the ordinary boot runs.
    assert_eq!(panel.screen(&mut e, 2_100).name, "COPPERSYNTH");
    let bank: String = e.bank_name().chars().take(20).collect();
    assert_eq!(panel.screen(&mut e, 3_700).name, bank);
    assert_eq!(panel.screen(&mut e, 5_200).part, "01");
}

/// A panel born mid-reset (the host rebuilds the unit around the new
/// bank) holds Initializing... for its second, then boots.
#[test]
fn a_rebuilt_panel_can_open_on_the_initializing_hold() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    panel.begin_initializing();
    assert_eq!(panel.screen(&mut e, 0).name, "Initializing...");
    assert_eq!(panel.screen(&mut e, 900).name, "Initializing...");
    assert_eq!(panel.screen(&mut e, 1_100).name, "COPPERSYNTH");
}

/// MUTE declines the factory question: no request, the loaded bank
/// stays, and the unit goes straight home.
#[test]
fn the_font_prompt_declines_quietly() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    panel.power_on_held(&[Button::Arrow(Pair::Instrument, Dir::Right)]);
    panel.screen(&mut e, 0);
    assert_eq!(panel.button(&mut e, Button::Mute), None);
    assert_eq!(panel.screen(&mut e, 100).part, "01");
}

/// A game's letter takes the line and stays until a button sends it
/// away; that press is spent on the dismissal.
#[test]
fn letters_stay_until_dismissed() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    send(&mut e, &dt1(0x45, [0x10, 0x00, 0x00], b"<< SIERRA >>"));
    for feed in e.take_panel_feed() {
        panel.feed(feed);
    }
    let letter = panel.screen(&mut e, 10_000);
    assert_eq!(letter.name, "<< SIERRA >>");
    assert_eq!(letter.part, "---", "the letter's blank slots dash too");
    assert_eq!(letter.instrument, "---");
    assert_eq!(
        panel.screen(&mut e, 600_000).name,
        "<< SIERRA >>",
        "it does not time out"
    );
    assert_eq!(
        panel.screen(&mut e, 600_000).level,
        "127",
        "values stay live"
    );
    // The dismissing press does nothing else.
    panel.button(&mut e, Button::All);
    let screen = panel.screen(&mut e, 600_100);
    assert_eq!(screen.part, "01", "home again");
    assert!(!screen.all_led, "the press was spent on the letter");
}

/// A long letter scrolls, rests on its tail, and comes round again.
#[test]
fn long_letters_scroll_round() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    send(
        &mut e,
        &dt1(0x45, [0x10, 0x00, 0x00], b"THE QUICK BROWN FOX JUMPS"),
    );
    for feed in e.take_panel_feed() {
        panel.feed(feed);
    }
    assert_eq!(panel.screen(&mut e, 20_000).name, "THE QUICK BROWN FOX ");
    assert_eq!(
        panel.screen(&mut e, 20_000 + 600 + 300).name,
        "HE QUICK BROWN FOX J"
    );
    // One full cycle later it is back at the head.
    let cycle = 600 + 5 * 300 + 1500;
    assert_eq!(
        panel.screen(&mut e, 20_000 + cycle).name,
        "THE QUICK BROWN FOX "
    );
}

/// A dot picture owns the matrix for a moment, exactly as mapped in
/// the manual: five bits a byte, four groups a row.
#[test]
fn a_picture_takes_the_bars() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    let mut dots = [0u8; 64];
    dots[0] = 0x1F;
    dots[16] = 0x1F;
    dots[32] = 0x1F;
    dots[48] = 0x10;
    send(&mut e, &dt1(0x45, [0x10, 0x01, 0x00], &dots));
    for feed in e.take_panel_feed() {
        panel.feed(feed);
    }
    let screen = panel.screen(&mut e, 30_000);
    for (column, bar) in screen.bars.iter().enumerate() {
        assert_eq!(*bar, 1 << 15, "column {column} lights only its top dot");
    }
    let after = panel.screen(&mut e, 34_000);
    assert_eq!(after.bars[0] & 1, 1, "the meters return with a baseline");
}

/// A pair pressed together shows its values across the parts: the
/// MIDI CH staircase of a factory unit.
#[test]
fn a_pair_together_shows_the_staircase() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::Both(Pair::MidiCh));
    let screen = panel.screen(&mut e, 4_000);
    for (part, bar) in screen.bars.iter().enumerate() {
        assert_eq!(bar.count_ones(), part as u32 + 1, "one more dot a part");
    }
    panel.button(&mut e, Button::Both(Pair::MidiCh));
    let screen = panel.screen(&mut e, 4_100);
    assert_eq!(screen.bars[15] & 1, 1, "back to the meters");
}

/// Sounding notes raise the meters; silence lets them fall, peak dot
/// and all.
#[test]
fn the_meters_move_with_the_music() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    send(&mut e, &[0x90, 60, 120]);
    let mut block = vec![(0f32, 0f32); 4410];
    e.render(&mut block);
    let loud = panel.screen(&mut e, 4_000).bars[0];
    assert!(loud.count_ones() > 2, "a fortissimo note stacks dots");
    send(&mut e, &[0x80, 60, 0]);
    for _ in 0..40 {
        e.render(&mut block);
    }
    // Step time forward the way frames would, so the fall and the
    // peak's descent both run their course.
    for at in 0..60 {
        panel.screen(&mut e, 4_100 + at * 100);
    }
    let quiet = panel.screen(&mut e, 10_200).bars[0];
    assert_eq!(quiet, 1, "released and fallen: just the baseline");
}

/// Both PART halves through the power-on: the unit plays to itself.
/// ALL rolls the song, MUTE stops it, and every value reads "---"
/// because there is nothing to edit while it does.
#[test]
fn demo_mode_is_a_tiny_midi_player() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    panel.power_on_held(&[Button::Both(Pair::Part)]);
    let armed = panel.screen(&mut e, 0);
    assert_eq!(armed.part, "S-1");
    assert_eq!(armed.name, "Railgun Rain");
    assert_eq!(armed.instrument, "---");
    assert_eq!(armed.level, "---");
    assert_eq!(armed.reverb, "---");
    assert!(armed.mute_led, "MUTE lit while armed");
    assert!(!armed.all_led);
    // ALL rolls it; the meters carry the song.
    panel.button(&mut e, Button::All);
    let playing = panel.screen(&mut e, 500);
    assert!(playing.all_led);
    assert!(!playing.mute_led);
    let mut block = vec![(0f32, 0f32); 44_100];
    e.render(&mut block);
    let rms = (block.iter().map(|(l, r)| l * l + r * r).sum::<f32>() / block.len() as f32).sqrt();
    assert!(rms > 0.001, "the demo must actually sound: {rms}");
    // The wire is ignored while the unit plays to itself.
    let before = e.part_view(0).instrument;
    send(&mut e, &[0xC0, 55]);
    assert_eq!(
        e.part_view(0).instrument,
        before,
        "MIDI IN falls on deaf ears"
    );
    // MUTE stops it and hands the lamp back.
    panel.button(&mut e, Button::Mute);
    let stopped = panel.screen(&mut e, 1_000);
    assert!(stopped.mute_led);
    assert!(!stopped.all_led);
    // The PART arrows walk the two songs and come round again.
    panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Right));
    let second = panel.screen(&mut e, 1_100);
    assert_eq!(second.part, "S-2");
    assert_eq!(second.name, "Title Screen");
    panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Right));
    assert_eq!(panel.screen(&mut e, 1_200).part, "S-1");
}

/// The boot line shows no values either: dashes in every segment
/// while COPPERSYNTH holds the glass.
#[test]
fn the_boot_line_wears_dashes() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    let greeting = panel.screen(&mut e, 0);
    assert_eq!(greeting.name, "COPPERSYNTH");
    assert_eq!(greeting.level, "---");
    assert_eq!(greeting.midi_ch, "---");
    assert_eq!(panel.screen(&mut e, 3_100).level, "127", "and home again");
}

/// MUTE latched under a MIDI CH arrow opens the Device ID edit: the
/// arrows cycle 1-32, ALL commits, MUTE cancels -- the mkII's own
/// panel operation in this unit's grammar.
#[test]
fn the_device_id_edit_cycles_commits_and_cancels() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::MuteArrow(Pair::MidiCh, Dir::Right));
    let screen = panel.screen(&mut e, 4_000);
    assert_eq!(screen.name, "Device ID: 17", "seeded on the factory ID");
    assert!(screen.all_led && screen.mute_led, "the lamps flash on");
    panel.button(&mut e, Button::Arrow(Pair::MidiCh, Dir::Right));
    assert_eq!(panel.screen(&mut e, 4_000).name, "Device ID: 18");
    // Wraps: 18 left to 17, 16 ... down past 1 comes round to 32.
    for _ in 0..18 {
        panel.button(&mut e, Button::Arrow(Pair::MidiCh, Dir::Left));
    }
    assert_eq!(panel.screen(&mut e, 4_000).name, "Device ID: 32");
    panel.button(&mut e, Button::All);
    assert_eq!(e.device_id(), 32, "ALL commits the pending value");
    assert_eq!(
        panel.screen(&mut e, 4_000).name,
        "Device ID 32",
        "the notice"
    );
    // Cancel leaves the ID alone.
    panel.button(&mut e, Button::MuteArrow(Pair::MidiCh, Dir::Left));
    panel.button(&mut e, Button::Arrow(Pair::MidiCh, Dir::Right));
    panel.button(&mut e, Button::Mute);
    assert_eq!(e.device_id(), 32, "MUTE cancels");
}

/// MUTE latched under a CHORUS arrow opens the Chorus Type edit: 0-8
/// with the type's name on the second line, ALL commits, MUTE cancels.
#[test]
fn the_chorus_type_edit_names_types_and_commits() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::MuteArrow(Pair::Chorus, Dir::Right));
    let screen = panel.screen(&mut e, 4_000);
    assert_eq!(screen.name, "Chorus Type: 2", "the unit wakes in Chorus 2");
    assert_eq!(screen.subtitle, "Chorus 2");
    for _ in 0..4 {
        panel.button(&mut e, Button::Arrow(Pair::Chorus, Dir::Right));
    }
    let screen = panel.screen(&mut e, 4_000);
    assert_eq!(screen.name, "Chorus Type: 6");
    assert_eq!(screen.subtitle, "Flanger");
    // The selection is already sounding for the audition.
    assert_eq!(e.chorus_type().index(), 6, "cycling activates at once");
    panel.button(&mut e, Button::All);
    assert_eq!(e.chorus_type().index(), 6, "ALL keeps the type");
    // Cancel puts the original back, even after auditioning another;
    // 0 is Off and stays selectable.
    panel.button(&mut e, Button::MuteArrow(Pair::Chorus, Dir::Left));
    for _ in 0..6 {
        panel.button(&mut e, Button::Arrow(Pair::Chorus, Dir::Left));
    }
    assert_eq!(panel.screen(&mut e, 4_000).subtitle, "Off");
    assert_eq!(e.chorus_type().index(), 0, "the audition sounds Off");
    panel.button(&mut e, Button::Mute);
    assert_eq!(e.chorus_type().index(), 6, "MUTE restores the original");
}

/// MUTE latched under an INSTRUMENT arrow opens the part-parameter
/// editor: INSTRUMENT arrows browse, LEVEL arrows set 0-127 sounding
/// at once, PART arrows move between parts, ALL keeps the lot and
/// MUTE puts the whole snapshot back.
#[test]
fn the_part_parameter_editor_browses_audits_and_restores() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::MuteArrow(Pair::Instrument, Dir::Right));
    let screen = panel.screen(&mut e, 4_000);
    assert_eq!(screen.name, "Portamento Time: 0");
    assert_eq!(screen.part, "01");
    assert!(screen.all_led && screen.mute_led, "the lamps flash on");
    // LEVEL edits the value, live.
    for _ in 0..3 {
        panel.button(&mut e, Button::Arrow(Pair::Level, Dir::Right));
    }
    assert_eq!(panel.screen(&mut e, 4_000).name, "Portamento Time: 3");
    assert_eq!(e.part_cc_value(0, 0x05), 3, "the edit sounds at once");
    // INSTRUMENT browses the settings.
    panel.button(&mut e, Button::Arrow(Pair::Instrument, Dir::Right));
    assert_eq!(panel.screen(&mut e, 4_000).name, "Portamento: 0");
    // PART moves the whole view to another part.
    panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Right));
    assert_eq!(panel.screen(&mut e, 4_000).part, "02");
    for _ in 0..6 {
        panel.button(&mut e, Button::Arrow(Pair::Instrument, Dir::Right));
    }
    assert_eq!(panel.screen(&mut e, 4_000).name, "Cutoff: 64");
    panel.button(&mut e, Button::Arrow(Pair::Level, Dir::Left));
    assert_eq!(e.part_nrpn_wire(1, 0x01, 0x20), 63, "the NRPN lands");
    // MUTE restores everything the audition touched, on every part.
    panel.button(&mut e, Button::Mute);
    assert_eq!(e.part_cc_value(0, 0x05), 0, "portamento time restored");
    assert_eq!(e.part_nrpn_wire(1, 0x01, 0x20), 64, "cutoff restored");
    // ALL keeps what the audition set. The editor kept part 2 selected
    // from the browse above, so the edit lands there.
    panel.button(&mut e, Button::MuteArrow(Pair::Instrument, Dir::Left));
    panel.button(&mut e, Button::Arrow(Pair::Level, Dir::Right));
    panel.button(&mut e, Button::All);
    assert_eq!(e.part_cc_value(1, 0x05), 1, "ALL keeps the edit");
    assert_eq!(
        panel.screen(&mut e, 4_000).name,
        "Part params saved",
        "the notice"
    );
}

/// Entered with ALL lit, the part-parameter editor writes every part
/// at once and says so; a PART press snaps back to the single part.
#[test]
fn the_part_parameter_editor_speaks_for_all_parts() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::All); // ALL mode first
    panel.button(&mut e, Button::MuteArrow(Pair::Instrument, Dir::Right));
    assert_eq!(panel.screen(&mut e, 4_000).part, "ALL");
    panel.button(&mut e, Button::Arrow(Pair::Level, Dir::Right));
    assert_eq!(e.part_cc_value(0, 0x05), 1, "part 1 took the edit");
    assert_eq!(e.part_cc_value(11, 0x05), 1, "part 12 took it too");
    // PART snaps out of ALL and back to the selected part.
    panel.button(&mut e, Button::Arrow(Pair::Part, Dir::Right));
    assert_eq!(panel.screen(&mut e, 4_000).part, "01");
    panel.button(&mut e, Button::Mute); // restore everything
    assert_eq!(e.part_cc_value(11, 0x05), 0, "the snapshot covers all");
}

/// A solo is a passing state: MUTE takes it back with the press
/// spent, and any other button takes it back on the way to its own
/// meaning.
#[test]
fn solo_stands_down_for_any_press() {
    let Some(mut e) = engine(Mt32Mode::Off) else {
        return;
    };
    let mut panel = FrontPanel::default();
    settled(&mut panel, &mut e);
    panel.button(&mut e, Button::Monitor);
    assert_ne!(e.monitor(), Monitor::Off, "the solo engages");
    // MUTE lets it go, and mutes nothing.
    panel.button(&mut e, Button::Mute);
    assert_eq!(e.monitor(), Monitor::Off);
    assert!(!e.part_muted(0), "the un-solo press is spent");
    // Engaged again, ALL lets it go and still enters ALL mode.
    panel.button(&mut e, Button::Monitor);
    panel.button(&mut e, Button::All);
    assert_eq!(e.monitor(), Monitor::Off, "any press stands the solo down");
    assert_eq!(panel.screen(&mut e, 5_000).part, "ALL", "and means itself");
    panel.button(&mut e, Button::All);
}
