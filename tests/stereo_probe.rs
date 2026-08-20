// The stereo stage: how much side (L-R) content each effect really
// makes. The DSP once collapsed to polite near-mono -- the echo fed
// at the rooms' whisper, the width matrix skipped, the chorus taps
// agreeing down the middle -- and these floors keep that from coming
// back.
use coppersynth::engine::Engine;
use coppersynth::mt32::translator::Mt32Mode;

fn engine() -> Option<Engine> {
    Engine::open_bundled(44_100, Mt32Mode::Off).ok()
}

fn send(e: &mut Engine, bytes: &[u8]) {
    for &b in bytes {
        e.write_byte(b);
    }
}

fn mid_side(buf: &[(f32, f32)]) -> (f32, f32) {
    let n = buf.len() as f32;
    let mid = (buf.iter().map(|&(l, r)| (l + r) * (l + r)).sum::<f32>() / n).sqrt();
    let side = (buf.iter().map(|&(l, r)| (l - r) * (l - r)).sum::<f32>() / n).sqrt();
    (mid, side)
}

#[test]
fn the_effects_are_genuinely_stereo() {
    let run = |setup: &dyn Fn(&mut Engine)| -> (f32, f32) {
        let mut e = engine().unwrap();
        setup(&mut e);
        let mut out = vec![(0.0f32, 0.0f32); 88_200];
        e.render(&mut out);
        mid_side(&out)
    };
    let dry = run(&|e| {
        send(e, &[0xC0, 48, 0x90, 60, 110]);
    });
    println!(
        "dry strings   mid {:.5} side {:.5} ratio {:.3}",
        dry.0,
        dry.1,
        dry.1 / dry.0
    );
    let chorus = run(&|e| {
        e.set_part_chorus(0, 127);
        send(e, &[0xC0, 48, 0x90, 60, 110]);
    });
    println!(
        "chorus full   mid {:.5} side {:.5} ratio {:.3}",
        chorus.0,
        chorus.1,
        chorus.1 / chorus.0
    );
    let hall = run(&|e| {
        e.set_part_reverb(0, 127);
        send(e, &[0xC0, 48, 0x90, 60, 110]);
    });
    println!(
        "hall2 full    mid {:.5} side {:.5} ratio {:.3}",
        hall.0,
        hall.1,
        hall.1 / hall.0
    );
    let pan = run(&|e| {
        e.set_reverb_type(7);
        e.set_part_reverb(0, 127);
        send(e, &[0xC0, 115, 0x90, 72, 127]);
    });
    println!(
        "pan delay     mid {:.5} side {:.5} ratio {:.3}",
        pan.0,
        pan.1,
        pan.1 / pan.0
    );
    let delay = run(&|e| {
        e.set_reverb_type(6);
        e.set_part_reverb(0, 127);
        send(e, &[0xC0, 115, 0x90, 72, 127]);
    });
    println!(
        "delay         mid {:.5} side {:.5} ratio {:.3}",
        delay.0,
        delay.1,
        delay.1 / delay.0
    );
    assert!(
        chorus.1 / chorus.0 > 0.75 && chorus.1 > dry.1 * 2.0,
        "full chorus must spread across the stage"
    );
    assert!(
        hall.1 / hall.0 > 0.55,
        "the hall must be wider than the source"
    );
    assert!(pan.1 > 0.002, "the panning delay's repeats must lean hard");
    assert!(delay.1 < 1.0e-4, "the plain delay stays down the middle");
}
