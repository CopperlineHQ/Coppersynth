# Coppersynth

Coppersynth is the General MIDI sound module inside
[Copperline](https://github.com/CopperlineHQ/Copperline), built to play
the MIDI soundtracks of Amiga-era games -- and, one day, to stand on its
own. It is a SoundFont synthesizer with the habits of a Roland Sound
Canvas: sixteen parts, an SC-55-shaped front panel model, and an MT-32
translation layer in front, so a Sierra game that uploads sysex meant
for an MT-32 "just works" and is none the wiser.

The workspace has two crates:

- `rustysynth/` -- a derivative of
  [RustySynth](https://github.com/sinshu/rustysynth) (v1.3.6), the pure
  Rust SoundFont synthesizer by Nobuaki Tanaka, itself ported from his
  [MeltySynth](https://github.com/sinshu/meltysynth). The fork adds
  SF2.01 modulator support, live per-channel introspection, master
  effect returns, tolerant loading that repairs bruised soundfonts
  instead of refusing them, and effect calibration fixes (the reverb
  and chorus feeds were being silenced by the dry path's audibility
  gate). The reverb is Freeverb, as it is upstream.
- `coppersynth/` -- the engine an emulator embeds: a byte in from the
  serial line, stereo frames out. It carries the sixteen-part layer
  (receive channels, mutes, level caps, key shifts), the MT-32-to-GM
  translation built from Sierra's own driver data, the Sound Canvas
  display sysex (letters and dot pictures), and the front panel state
  machine -- every character the host draws is composed here.

The default soundfont is
[GeneralUser GS](https://github.com/mrbumpy409/GeneralUser-GS) by
S. Christian Collins -- an instrument library in its own right, with
the complete General MIDI sound set including the SFX bank and drum
kits at a very reasonable size. It is fetched from its repository at
build time, zipped and embedded, licence text alongside. Delete `assets/GeneralUser-GS.sf2`
to pick up a newer release on the next build.

## Building and testing

```sh
cargo build --release
cargo test --release
```

The build script uses `curl` to fetch the soundfont and `git` to stamp
the release date; both ship with stock Windows 10+, macOS and Linux,
and a build without them still builds (without the bundled bank, and
with an undated splash). The first build downloads the soundfont
(about 32 MB) into `assets/`;
`tools/fetch-assets.sh` also fetches the GeneralUser demo MIDIs for the
listening rig (`cargo run --bin ab-render`), which renders A/B pairs
against FluidSynth output for by-ear regression checks. The
`replay-mt32` tool replays captured game MIDI through the translation
layer and reports what an MT-32 would have understood.

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
- ScummVM's MT-32-to-GM driver and wildmidi, cross-referenced (with
  Sierra's own patch data) when the translation tables were assembled;
  the tables here were built independently from that comparison.

Coppersynth is MIT licensed; see `LICENSE.txt`, and
`rustysynth/LICENSE.txt` for the base work's licence.
