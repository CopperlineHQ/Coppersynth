//! The MT-32 -> General MIDI data, cross-referenced from three sources
//! and resolved with a stated rule; regenerate with the scripts kept in
//! `reference/` rather than editing values here.
//!
//! - ScummVM's `MidiDriver::_mt32ToGm` (audio/mididrv.cpp), the
//!   production map a shipping project has carried for two decades.
//! - wildmidi's `mt32asgm` (src/xmi2mid.c), the other long-lived table.
//! - Sierra's own MT-32 -> GM translations, recovered from the LB2, PQ1
//!   and QFG1 patch banks (wjp/freesci-archive, mt32_GM_mapping): the
//!   people who voiced the originals saying what the GM rendering is.
//!
//! The two tables agree on 96 of 128 presets. Where they differ, a
//! Sierra witness settles it; with no witness, ScummVM stands. Sierra
//! outvotes an *agreed* answer only when at least two of their games
//! concur.

/// GM program for each MT-32 preset timbre (group a: 0-63, group b:
/// 64-127). Contested entries carry their provenance.
pub const PATCH_TO_GM: [u8; 128] = [
    0,   // AcouPiano1
    1,   // AcouPiano2
    0,   // AcouPiano3 -- sierra (1 game); scummvm 0, wildmidi 2
    2,   // ElecPiano1 -- scummvm; wildmidi says 4
    4,   // ElecPiano2
    4,   // ElecPiano3 -- scummvm; wildmidi says 5
    5,   // ElecPiano4
    3,   // Honkytonk
    16,  // Elec Org 1
    17,  // Elec Org 2
    18,  // Elec Org 3
    16,  // Elec Org 4
    19,  // Pipe Org 1 -- sierra (1 game); scummvm 16, wildmidi 19
    19,  // Pipe Org 2
    20,  // Pipe Org 3 -- scummvm; wildmidi says 19
    21,  // Accordion
    6,   // Harpsi 1
    6,   // Harpsi 2
    6,   // Harpsi 3
    7,   // Clavi 1
    7,   // Clavi 2
    7,   // Clavi 3
    8,   // Celesta 1
    112, // Celesta 2 -- scummvm; wildmidi says 8
    62,  // Syn Brass1
    62,  // Syn Brass2 -- scummvm; wildmidi says 63
    63,  // Syn Brass3 -- scummvm; wildmidi says 62
    63,  // Syn Brass4
    38,  // Syn Bass 1
    38,  // Syn Bass 2 -- scummvm; wildmidi says 39
    39,  // Syn Bass 3 -- scummvm; wildmidi says 38
    39,  // Syn Bass 4
    88,  // Fantasy
    95,  // Harmo Pan -- scummvm; wildmidi says 90
    52,  // Chorale
    98,  // Glasses -- scummvm; wildmidi says 92
    97,  // Soundtrack
    99,  // Atmosphere
    14,  // Warm Bell
    54,  // Funny Vox
    102, // Echo Bell -- scummvm; wildmidi says 98
    96,  // Ice Rain
    53,  // Oboe 2001 -- scummvm; wildmidi says 68
    102, // Echo Pan -- scummvm; wildmidi says 95
    81,  // DoctorSolo
    100, // Schooldaze -- scummvm; wildmidi says 87
    14,  // BellSinger -- scummvm; wildmidi says 112
    80,  // SquareWave
    48,  // Str Sect 1
    48,  // Str Sect 2
    49,  // Str Sect 3 -- scummvm; wildmidi says 44
    45,  // Pizzicato
    41,  // Violin 1 -- scummvm; wildmidi says 40
    40,  // Violin 2
    42,  // Cello 1
    42,  // Cello 2
    43,  // Contrabass
    46,  // Harp 1
    45,  // Harp 2 -- scummvm; wildmidi says 46
    24,  // Guitar 1
    25,  // Guitar 2
    28,  // Elec Gtr 1 -- scummvm; wildmidi says 26
    27,  // Elec Gtr 2
    104, // Sitar
    32,  // Acou Bass1
    32,  // Acou Bass2
    34,  // Elec Bass1 -- scummvm; wildmidi says 33
    33,  // Elec Bass2 -- scummvm; wildmidi says 34
    36,  // Slap Bass1
    37,  // Slap Bass2
    35,  // Fretless 1
    35,  // Fretless 2
    79,  // Flute 1 -- scummvm; wildmidi says 73
    73,  // Flute 2
    72,  // Piccolo 1
    72,  // Piccolo 2
    74,  // Recorder
    75,  // Panpipes
    64,  // Sax 1
    65,  // Sax 2
    66,  // Sax 3
    67,  // Sax 4
    71,  // Clarinet 1
    71,  // Clarinet 2
    68,  // Oboe
    69,  // Engl Horn
    70,  // Bassoon
    22,  // Harmonica
    56,  // Trumpet 1
    56,  // Trumpet 2 -- sierra (2 games); scummvm 59, wildmidi 56
    57,  // Trombone 1
    57,  // Trombone 2
    60,  // Fr Horn 1
    60,  // Fr Horn 2
    58,  // Tuba
    61,  // Brs Sect 1
    61,  // Brs Sect 2
    11,  // Vibe 1
    11,  // Vibe 2
    15,  // Syn Mallet -- sierra (1 game); scummvm 98, wildmidi 99
    14,  // Wind Bell -- scummvm; wildmidi says 112
    9,   // Glock
    14,  // Tube Bell
    13,  // Xylophone
    12,  // Marimba
    107, // Koto
    107, // Sho -- scummvm; wildmidi says 111
    77,  // Shakuhachi
    78,  // Whistle 1
    78,  // Whistle 2
    76,  // BottleBlow
    76,  // BreathPipe
    47,  // Timpani
    117, // MelodicTom
    127, // Deep Snare -- scummvm; wildmidi says 116
    118, // Elec Perc1
    118, // Elec Perc2
    116, // Taiko
    115, // Taiko Rim
    119, // Cymbal
    115, // Castanets
    112, // Triangle
    55,  // Orche Hit
    124, // Telephone
    123, // Bird Tweet
    0,   // OneNoteJam -- scummvm; wildmidi says 94
    14,  // WaterBells -- scummvm; wildmidi says 98
    117, // JungleTune -- scummvm; wildmidi says 121
];

/// The preset timbre names, in preset order, as the control ROM spells
/// them: what a patch-memory write names, and what the panel shows.
pub const PRESET_NAMES: [&str; 128] = [
    "AcouPiano1",
    "AcouPiano2",
    "AcouPiano3",
    "ElecPiano1",
    "ElecPiano2",
    "ElecPiano3",
    "ElecPiano4",
    "Honkytonk ",
    "Elec Org 1",
    "Elec Org 2",
    "Elec Org 3",
    "Elec Org 4",
    "Pipe Org 1",
    "Pipe Org 2",
    "Pipe Org 3",
    "Accordion ",
    "Harpsi 1  ",
    "Harpsi 2  ",
    "Harpsi 3  ",
    "Clavi 1   ",
    "Clavi 2   ",
    "Clavi 3   ",
    "Celesta 1 ",
    "Celesta 2 ",
    "Syn Brass1",
    "Syn Brass2",
    "Syn Brass3",
    "Syn Brass4",
    "Syn Bass 1",
    "Syn Bass 2",
    "Syn Bass 3",
    "Syn Bass 4",
    "Fantasy   ",
    "Harmo Pan ",
    "Chorale   ",
    "Glasses   ",
    "Soundtrack",
    "Atmosphere",
    "Warm Bell ",
    "Funny Vox ",
    "Echo Bell ",
    "Ice Rain  ",
    "Oboe 2001 ",
    "Echo Pan  ",
    "DoctorSolo",
    "Schooldaze",
    "BellSinger",
    "SquareWave",
    "Str Sect 1",
    "Str Sect 2",
    "Str Sect 3",
    "Pizzicato ",
    "Violin 1  ",
    "Violin 2  ",
    "Cello 1   ",
    "Cello 2   ",
    "Contrabass",
    "Harp 1    ",
    "Harp 2    ",
    "Guitar 1  ",
    "Guitar 2  ",
    "Elec Gtr 1",
    "Elec Gtr 2",
    "Sitar     ",
    "Acou Bass1",
    "Acou Bass2",
    "Elec Bass1",
    "Elec Bass2",
    "Slap Bass1",
    "Slap Bass2",
    "Fretless 1",
    "Fretless 2",
    "Flute 1   ",
    "Flute 2   ",
    "Piccolo 1 ",
    "Piccolo 2 ",
    "Recorder  ",
    "Panpipes  ",
    "Sax 1     ",
    "Sax 2     ",
    "Sax 3     ",
    "Sax 4     ",
    "Clarinet 1",
    "Clarinet 2",
    "Oboe      ",
    "Engl Horn ",
    "Bassoon   ",
    "Harmonica ",
    "Trumpet 1 ",
    "Trumpet 2 ",
    "Trombone 1",
    "Trombone 2",
    "Fr Horn 1 ",
    "Fr Horn 2 ",
    "Tuba      ",
    "Brs Sect 1",
    "Brs Sect 2",
    "Vibe 1    ",
    "Vibe 2    ",
    "Syn Mallet",
    "Wind Bell ",
    "Glock     ",
    "Tube Bell ",
    "Xylophone ",
    "Marimba   ",
    "Koto      ",
    "Sho       ",
    "Shakuhachi",
    "Whistle 1 ",
    "Whistle 2 ",
    "BottleBlow",
    "BreathPipe",
    "Timpani   ",
    "MelodicTom",
    "Deep Snare",
    "Elec Perc1",
    "Elec Perc2",
    "Taiko     ",
    "Taiko Rim ",
    "Cymbal    ",
    "Castanets ",
    "Triangle  ",
    "Orche Hit ",
    "Telephone ",
    "Bird Tweet",
    "OneNoteJam",
    "WaterBells",
    "JungleTune",
];

/// Sierra's own GM choices for their custom timbres, by the 10-column
/// name a game uploads to timbre memory: (name, GM program, key shift,
/// volume adjustment). Merged across the three recovered banks by
/// majority. The name matcher trims both sides, so \"Explode \" and
/// \"Explode\" meet.
pub const CUSTOM_NAME_TO_GM: [(&str, u8, i8, i8); 91] = [
    ("ANALOG SYN", 90, 0, 127),
    ("Arena2  MS", 126, 0, 0),
    ("ArenaNoSus", 126, 0, 0),
    ("Armor   MS", 113, 12, -20),
    ("BIG BANJO", 98, 0, -10),
    ("BIG TOMS", 116, 0, 0),
    ("BanjoLB2", 105, 0, -20),
    ("BassPizzMS", 45, -12, 0),
    ("Bell Tree", 9, 0, 0),
    ("Bells   MS", 112, 12, 80),
    ("Boing   MS", 116, 0, 0),
    ("CLICKS", 115, 0, 0),
    ("CabEngine", 66, -60, 50),
    ("Calliope", 16, 0, 0),
    ("Chicago MS", 1, 0, 30),
    ("ChurchB MS", 14, 0, 0),
    ("ClarinetMS", 71, 0, -20),
    ("Claw    MS", 118, 0, 0),
    ("Conga   MS", 116, 0, 0),
    ("CracklesMS", 115, 0, 0),
    ("Crash   MS", 127, 0, 127),
    ("Cricket", 120, 0, 0),
    ("CstlGateMS", 127, -50, 50),
    ("CymSwellMS", 119, 0, 0),
    ("DoorSlamMS", 115, -12, 0),
    ("ElecGtr MS", 27, 0, 0),
    ("EnglHornMS", 69, 12, 40),
    ("Explode MS", 127, -12, -15),
    ("F VoxStrg", 48, 0, 0),
    ("FEEDBAK AX", 30, -12, 65),
    ("FUNK PING", 46, 27, -10),
    ("Fantasy2MS", 88, 0, 10),
    ("FireDartMS", 122, 0, 120),
    ("Flame2  MS", 121, 0, 100),
    ("Flames  MS", 121, 0, 0),
    ("Flames3 MS", 125, 0, 80),
    ("Flute   MS", 73, 0, 45),
    ("FrHorn1 MS", 60, 0, 40),
    ("FrHorn1MS2", 60, 0, 0),
    ("GameSnd MS", 80, 0, 0),
    ("Glock   MS", 9, 0, 0),
    ("Gun     MS", 127, 0, 0),
    ("HEFTY BASS", 33, -24, 100),
    ("Horse1  MS", 115, 0, 0),
    ("Horse2  MS", 115, 0, 0),
    ("InHale  MS", 121, 0, 25),
    ("Kiss    MS", 127, 100, 0),
    ("KnifeStkMS", 115, 0, 110),
    ("LUSH STRNG", 48, 0, 117),
    ("Laser   MS", 81, 0, -40),
    ("LghtboltMS", 122, 0, 120),
    ("Lock    MS", 115, 0, 0),
    ("NewSplatMS", 117, 0, 0),
    ("Ninga   MS", 121, 0, 100),
    ("ORGAN B", 16, 0, 55),
    ("Oboe    MS", 68, 0, -20),
    ("Owl     MS", 123, -12, 0),
    ("Pft     MS", 125, 0, 110),
    ("Pizz    MS", 45, 0, 0),
    ("Punch   MS", 118, 0, 80),
    ("REV CYMBAL", 119, 0, 0),
    ("ROCK GUIT1", 30, -12, 127),
    ("Raspbry MS", 81, 0, 0),
    ("RatSqueek", 120, 12, 0),
    ("RimShot MS", 115, 0, 0),
    ("STACKBASS", 37, -12, 100),
    ("Scrubin'MS", 119, 0, 0),
    ("Skid    MS", 125, 0, 0),
    ("SmileFacMS", 125, 0, 40),
    ("Snare", 115, 0, 0),
    ("Snare   MS", 116, 0, 0),
    ("SpaceVibes", 11, 0, 0),
    ("Spit    MS", 115, -12, 0),
    ("Splat   MS", 118, 0, 0),
    ("SqurWaveMS", 80, 0, 0),
    ("StoneDr MS", 119, -48, 0),
    ("StrSect1MS", 48, 0, 127),
    ("SwmpBackgr", 120, 0, 0),
    ("T-Bone2 MS", 57, 0, 65),
    ("TRAFFIC", 122, 0, 0),
    ("Taiko", 116, 12, -10),
    ("Thud    MS", 116, -12, 0),
    ("Thunder MS", 125, -12, 100),
    ("TireSqueal", 109, 0, 25),
    ("Toms    MS", 117, 0, 0),
    ("Tumble  MS", 118, 0, 40),
    ("Vase    MS", 127, -12, 0),
    ("WarmPadStr", 49, 0, 15),
    ("Window", 119, -48, 0),
    ("WoodBlox", 115, 0, 0),
    ("seagulls", 123, 0, 0),
];

/// The rhythm key map. The MT-32/CM-64 arrangement is what the GM
/// percussion map was modelled on, and Sierra's three banks contain no
/// unanimous non-identity mapping, so keys 35..=75 pass through and
/// anything outside the MT-32's rhythm range is dropped rather than
/// guessed. A game's own rhythm-setup writes override this at runtime.
pub fn rhythm_key_to_gm(key: u8) -> Option<u8> {
    (35..=75).contains(&key).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_patch_lands_on_a_gm_program() {
        for (i, &gm) in PATCH_TO_GM.iter().enumerate() {
            assert!(gm < 128, "preset {i} maps to {gm}");
        }
    }

    /// The entries the three sources fought over, pinned to the resolution
    /// rule's outcome so a regeneration cannot quietly change sides.
    #[test]
    fn the_contested_entries_hold_their_resolutions() {
        // Sierra witnesses.
        assert_eq!(PATCH_TO_GM[2], 0, "AcouPiano3: Sierra sides with ScummVM");
        assert_eq!(
            PATCH_TO_GM[12], 19,
            "Pipe Org 1 is a church organ, per Sierra"
        );
        assert_eq!(
            PATCH_TO_GM[89], 56,
            "Trumpet 2 is not muted, per two Sierra games"
        );
        assert_eq!(PATCH_TO_GM[99], 15, "Syn Mallet: Sierra chose Dulcimer");
        // No witness: ScummVM stands.
        assert_eq!(PATCH_TO_GM[23], 112, "Sound Tracker: ScummVM's Tinkle Bell");
        assert_eq!(PATCH_TO_GM[114], 127, "Jungle Tune: ScummVM's Gunshot");
    }

    #[test]
    fn sierra_custom_names_resolve() {
        let find = |n: &str| {
            CUSTOM_NAME_TO_GM
                .iter()
                .find(|(name, ..)| name.trim() == n)
                .copied()
        };
        let (_, gm, _, _) = find("Explode MS").expect("LB2's explosion");
        assert_eq!(gm, 127, "an explosion is a gunshot in GM");
        assert!(find("SqurWaveMS").is_some(), "the square lead");
    }

    #[test]
    fn rhythm_keys_pass_through_inside_the_kit() {
        assert_eq!(rhythm_key_to_gm(35), Some(35));
        assert_eq!(rhythm_key_to_gm(75), Some(75));
        assert_eq!(rhythm_key_to_gm(34), None);
        assert_eq!(rhythm_key_to_gm(76), None);
    }
}

/// The default key each preset rhythm timbre sits on, in rhythm-timbre
/// order, recovered from the Sierra banks' rhythm sections (they ship
/// the default arrangement). A game's rhythm-setup write that assigns
/// rhythm timbre N to some other key routes that key here. Timbres the
/// banks never placed have no home and stay unmapped.
pub(crate) const RHYTHM_TIMBRE_KEY: [Option<u8>; 30] = [
    Some(35), // Acou BD
    Some(38), // Acou SD
    Some(48), // Acou HiTom
    Some(45), // AcouMidTom
    Some(41), // AcouLowTom
    Some(40), // Elec SD
    Some(42), // Clsd HiHat
    Some(46), // OpenHiHat1
    Some(49), // Crash Cym
    Some(51), // Ride Cym
    Some(37), // Rim Shot
    Some(39), // Hand Clap
    Some(56), // Cowbell
    Some(62), // Mt HiConga
    Some(63), // High Conga
    Some(64), // Low Conga
    Some(65), // Hi Timbale
    Some(66), // LowTimbale
    Some(60), // High Bongo
    Some(61), // Low Bongo
    Some(67), // High Agogo
    Some(68), // Low Agogo
    Some(54), // Tambourine
    Some(75), // Claves
    Some(70), // Maracas
    Some(72), // SmbaWhis L
    Some(71), // SmbaWhis S
    Some(69), // Cabasa
    Some(73), // Quijada
    Some(44), // OpenHiHat2
];

/// Where a preset rhythm timbre sounds in the GM kit.
pub(crate) fn rhythm_timbre_home_key(timbre: u8) -> Option<u8> {
    RHYTHM_TIMBRE_KEY.get(timbre as usize).copied().flatten()
}

/// Squash a timbre name for matching: lowercase, alphanumerics only.
/// "FrHorn1MS2" and "Fr Horn 1" meet in the middle.
fn squash(name: &[u8]) -> Vec<u8> {
    name.iter()
        .filter(|b| b.is_ascii_alphanumeric())
        .map(|b| b.to_ascii_lowercase())
        .collect()
}

/// The GM rendering of a custom timbre, by the name the game uploaded:
/// (program, key shift, velocity adjustment).
///
/// Sierra's own table answers first -- they named these timbres and then
/// chose their GM stand-ins themselves. Failing that, the name is matched
/// against the preset timbres, longest squashed substring wins, which
/// catches the everyday "modified preset" ("StrSect1MS" lands on the
/// string section). A name matching nothing falls back to a square lead:
/// audibly synthetic, deliberately so, and logged upstream where there is
/// a log to write to.
pub fn match_custom_name(name: &[u8; 10]) -> (u8, i8, i8) {
    let uploaded = squash(name);
    if uploaded.is_empty() {
        return (80, 0, 0);
    }
    // Sierra's table stores names without their "m " marker; exact squash
    // equality first, then containment either way.
    let mut best: Option<(usize, (u8, i8, i8))> = None;
    for &(sierra, gm, ksh, vol) in CUSTOM_NAME_TO_GM.iter() {
        let s = squash(sierra.as_bytes());
        if s.is_empty() {
            continue;
        }
        let score = if s == uploaded {
            usize::MAX
        } else if uploaded
            .windows(s.len().min(uploaded.len()))
            .any(|w| w == &s[..])
            || s.windows(uploaded.len().min(s.len()))
                .any(|w| w == &uploaded[..])
        {
            s.len()
        } else {
            continue;
        };
        if best.map_or(true, |(b, _)| score > b) {
            best = Some((score, (gm, ksh, vol)));
        }
    }
    if let Some((_, hit)) = best {
        return hit;
    }
    let mut preset_best: Option<(usize, u8)> = None;
    for (i, preset) in PRESET_NAMES.iter().enumerate() {
        let p = squash(preset.as_bytes());
        if p.len() < 3 {
            continue;
        }
        if uploaded
            .windows(p.len().min(uploaded.len()))
            .any(|w| w == &p[..])
        {
            if preset_best.map_or(true, |(b, _)| p.len() > b) {
                preset_best = Some((p.len(), PATCH_TO_GM[i]));
            }
        }
    }
    match preset_best {
        Some((_, gm)) => (gm, 0, 0),
        None => (80, 0, 0),
    }
}
