//! Replay a captured MIDI byte stream (COPPERLINE_GM_CAPTURE) through the
//! MT-32 translation layer and report what it decided: how each custom
//! timbre's name resolved, what every program change became, and what the
//! guest wrote to the display. This is how a play-through becomes a
//! reviewable, replayable corpus.
//!
//!     replay-mt32 CAPTURE.bytes

use copperline_gm::mt32::tables;
use copperline_gm::mt32::translator::{Event, Mt32Mode, Mt32Translator};

const GM_NAMES: [&str; 128] = [
    "Acoustic Grand Piano",
    "Bright Acoustic Piano",
    "Electric Grand Piano",
    "Honky-tonk Piano",
    "Electric Piano 1",
    "Electric Piano 2",
    "Harpsichord",
    "Clavinet",
    "Celesta",
    "Glockenspiel",
    "Music Box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular Bells",
    "Dulcimer",
    "Drawbar Organ",
    "Percussive Organ",
    "Rock Organ",
    "Church Organ",
    "Reed Organ",
    "Accordion",
    "Harmonica",
    "Tango Accordion",
    "Acoustic Guitar (nylon)",
    "Acoustic Guitar (steel)",
    "Electric Guitar (jazz)",
    "Electric Guitar (clean)",
    "Electric Guitar (muted)",
    "Overdriven Guitar",
    "Distortion Guitar",
    "Guitar Harmonics",
    "Acoustic Bass",
    "Electric Bass (finger)",
    "Electric Bass (pick)",
    "Fretless Bass",
    "Slap Bass 1",
    "Slap Bass 2",
    "Synth Bass 1",
    "Synth Bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Contrabass",
    "Tremolo Strings",
    "Pizzicato Strings",
    "Orchestral Harp",
    "Timpani",
    "String Ensemble 1",
    "String Ensemble 2",
    "Synth Strings 1",
    "Synth Strings 2",
    "Choir Aahs",
    "Voice Oohs",
    "Synth Voice",
    "Orchestra Hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted Trumpet",
    "French Horn",
    "Brass Section",
    "Synth Brass 1",
    "Synth Brass 2",
    "Soprano Sax",
    "Alto Sax",
    "Tenor Sax",
    "Baritone Sax",
    "Oboe",
    "English Horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan Flute",
    "Blown Bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Lead 1 (square)",
    "Lead 2 (sawtooth)",
    "Lead 3 (calliope)",
    "Lead 4 (chiff)",
    "Lead 5 (charang)",
    "Lead 6 (voice)",
    "Lead 7 (fifths)",
    "Lead 8 (bass + lead)",
    "Pad 1 (new age)",
    "Pad 2 (warm)",
    "Pad 3 (polysynth)",
    "Pad 4 (choir)",
    "Pad 5 (bowed)",
    "Pad 6 (metallic)",
    "Pad 7 (halo)",
    "Pad 8 (sweep)",
    "FX 1 (rain)",
    "FX 2 (soundtrack)",
    "FX 3 (crystal)",
    "FX 4 (atmosphere)",
    "FX 5 (brightness)",
    "FX 6 (goblins)",
    "FX 7 (echoes)",
    "FX 8 (sci-fi)",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bag pipe",
    "Fiddle",
    "Shanai",
    "Tinkle Bell",
    "Agogo",
    "Steel Drums",
    "Woodblock",
    "Taiko Drum",
    "Melodic Tom",
    "Synth Drum",
    "Reverse Cymbal",
    "Guitar Fret Noise",
    "Breath Noise",
    "Seashore",
    "Bird Tweet",
    "Telephone Ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: replay-mt32 CAPTURE.bytes");
    let bytes = std::fs::read(&path).expect("read capture");
    println!("{path}: {} bytes\n", bytes.len());

    // The timbre names the guest uploaded, straight off the wire, so the
    // matcher's verdict on each can be reported.
    let mut names: Vec<[u8; 10]> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0xF0 {
            let end = bytes[i..].iter().position(|&b| b == 0xF7).map(|e| i + e);
            let Some(end) = end else { break };
            let body = &bytes[i + 1..end];
            if body.len() >= 17 && body[0] == 0x41 && body[2] == 0x16 && body[3] == 0x12 {
                let addr = ((body[4] as u32) << 14) | ((body[5] as u32) << 7) | body[6] as u32;
                if (0x08 << 14..(0x08 << 14) + 64 * 256).contains(&addr)
                    && (addr - (0x08 << 14)).is_multiple_of(256)
                {
                    let mut n = [b' '; 10];
                    n.copy_from_slice(&body[7..17]);
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    println!("--- custom timbres and the matcher's verdicts ---");
    for n in &names {
        let shown: String = n.iter().map(|&b| b as char).collect();
        let (gm, ksh, vol) = tables::match_custom_name(n);
        println!(
            "  {shown:?} -> {:3} {} (shift {ksh:+}, vol {vol:+})",
            gm, GM_NAMES[gm as usize]
        );
    }

    // The full replay: every event the synthesizer would have seen.
    let mut t = Mt32Translator::new(Mt32Mode::Auto);
    let mut programs: Vec<(u8, u8)> = Vec::new();
    let mut notes = 0usize;
    let mut rhythm_notes = 0usize;
    println!("\n--- replay ---");
    for &b in &bytes {
        for event in t.push(b) {
            match event {
                Event::Translating(on) => {
                    println!("  translation {}", if on { "ON" } else { "off" })
                }
                Event::Display(text) => println!("  display: {text:?}"),
                Event::MasterVolume(v) => println!("  master volume {v}"),
                Event::Midi {
                    command: 0xC0,
                    channel,
                    data1,
                    ..
                } => {
                    if !programs.contains(&(channel, data1)) {
                        programs.push((channel, data1));
                    }
                }
                Event::Midi {
                    command: 0x90,
                    channel,
                    ..
                } => {
                    notes += 1;
                    if channel == 9 {
                        rhythm_notes += 1;
                    }
                }
                _ => {}
            }
        }
    }
    // Rhythm keys the source stream carried that the translator dropped
    // are invisible above; count them from a second pass in Off mode.
    let mut raw = Mt32Translator::new(Mt32Mode::Off);
    let mut raw_rhythm = 0usize;
    for &b in &bytes {
        for event in raw.push(b) {
            if let Event::Midi {
                command: 0x90,
                channel: 9,
                ..
            } = event
            {
                raw_rhythm += 1;
            }
        }
    }
    let dropped_rhythm = raw_rhythm.saturating_sub(rhythm_notes);

    println!("\n--- programs in force (channel -> GM) ---");
    for (ch, gm) in &programs {
        println!("  ch{:2} -> {:3} {}", ch + 1, gm, GM_NAMES[*gm as usize]);
    }
    println!("\nnotes: {notes} ({rhythm_notes} rhythm, {dropped_rhythm} dropped outside the kit)");
}
