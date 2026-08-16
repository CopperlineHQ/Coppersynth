# Coppersynth

A General MIDI sound module in safe Rust, with the habits of a Roland
Sound Canvas: a sixteen-part SoundFont synthesizer, an MT-32 translation
layer in front, and an SC-55-shaped front panel model behind it. MIDI
goes in as bytes, stereo samples come out, and a Sierra-era game that
uploads sysex meant for an MT-32 "just works" with no ROMs at all.

<img width="1238" height="156" alt="Screenshot 2026-08-16 at 22 25 00" src="https://github.com/user-attachments/assets/1063f983-27d9-4971-8dcf-735e26ef8d00" />


Written for the [Copperline](https://github.com/CopperlineHQ/Copperline)
Amiga emulator, which pins this crate and offers it as the built-in MIDI
output; this repository is the upstream. For now it is purely a
Copperline thing -- the API moves at Copperline's pace and promises
nothing to anyone else -- though nothing in it knows what an Amiga is,
and it may stand alone one day. A sibling of
[mt32-rs](https://github.com/CopperlineHQ/mt32-rs) and
[FluxBridge](https://github.com/CopperlineHQ/FluxBridge).

## Use

```rust
use coppersynth::engine::GmEngine;
use coppersynth::mt32::translator::Mt32Mode;

let mut engine = GmEngine::open_bundled(44_100, Mt32Mode::Auto)
    .expect("the bundled bank");

// Raw bytes, straight off the wire: running status, SysEx and all.
// MT-32 traffic is recognised and translated as it arrives.
for byte in midi_bytes {
    engine.write_byte(byte);
}

// Interleaved stereo frames at the rate the engine was opened with.
let mut frames = vec![(0.0f32, 0.0f32); 4096];
engine.render(&mut frames);
```

`coppersynth::panel::FrontPanel` is the front panel as a model: buttons
go in, a composed LCD screen and lamp states come out. The host only
draws; every character is decided here.

## What's inside

- The sixteen-part layer: receive channels, mutes and solo, per-part
  level caps, key shifts, panel edits that hold against the wire, and
  live meters read straight from the voices.
- The MT-32-to-GM translation, inspired by Sierra's own driver data:
  custom timbre uploads matched by name, patch memory re-pointed,
  rhythm remapped, display text shown on the LCD.
- A synthesis core derived from RustySynth (under `src/synth/`),
  extended with SF2.01 modulator support, live introspection, master
  effect returns, and tolerant loading that repairs bruised soundfonts
  instead of refusing them.
- Deterministic: the same bytes render the same samples on every run.

## Building and testing

```sh
cargo build --release
cargo test --release
```

The build script uses `curl` to fetch the bundled soundfont and `git`
to stamp the release date; both ship with stock Windows 10+, macOS and
Linux, and a build without them still builds (without the bundled bank,
and with an undated splash). The first build downloads the bank (about
32 MB) into `assets/`; delete `assets/GeneralUser-GS.sf2` to pick up a
newer release. The `replay-mt32` tool replays captured game MIDI
through the translation layer and reports what an MT-32 would have
understood.

## The bundled bank

The default soundfont is
[GeneralUser GS](https://github.com/mrbumpy409/GeneralUser-GS) by
S. Christian Collins -- an instrument library in its own right, with
the complete General MIDI sound set including the SFX bank and drum
kits at a very reasonable size. It is fetched from its repository at
build time, zipped and embedded, licence text alongside. Any other
`.sf2` (or a `.zip` holding one) loads at runtime through
`GmEngine::open`.

## Credits

- [RustySynth](https://github.com/sinshu/rustysynth) and
  [MeltySynth](https://github.com/sinshu/meltysynth) by Nobuaki Tanaka
  (MIT) -- the synthesis core this work stands on.
- Freeverb by Jezar at Dreampoint (public domain), via RustySynth.
- [GeneralUser GS](https://github.com/mrbumpy409/GeneralUser-GS) by
  S. Christian Collins -- a complete General MIDI library of his own
  making -- under its own licence (bundling and redistribution
  permitted; the licence text ships in the bundle).
- The Roland SC-55 owner's manual, as the behavioural reference for the
  front panel, and the MT-32 owner's manual for the translation layer's
  memory map.
- The demo songs: "Railgun Rain" by Ivan Stanton (northivanastan),
  public domain; "Title Screen", artist unknown, under a licence
  asking no credit.
- ScummVM's MT-32-to-GM driver and wildmidi, cross-referenced (with
  Sierra's own patch data) when the translation tables were assembled;
  the tables here were built independently from that comparison.

## Licence and provenance

MIT, as a derivative work of RustySynth:

- The synthesis core under `src/synth/` descends from RustySynth v1.3.6
  by Nobuaki Tanaka, itself a port of his MeltySynth;
  `LICENSE-rustysynth.txt` carries the original MIT licence and
  copyright, which continue to apply to that code.
- Everything above the core -- the engine, the translation layer, the
  panel model -- is Copperline's own: `LICENSE.txt`, copyright Lee
  Hobson.
