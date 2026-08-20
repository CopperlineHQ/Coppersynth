//! The front panel: the SC-55-shaped face of the engine.
//!
//! Everything the LCD shows is composed here, in the library -- the host
//! draws glass and buttons and forwards presses, and never invents a
//! character of its own. The layout follows the Sound Canvas: ALL and
//! MUTE with their lamps, eight left/right pairs, the sixteen-column bar
//! matrix with a dot of peak hold, and letters and dot pictures a game
//! writes over sysex.
//!
//! What the real unit keeps in menus (master tune, LCD contrast, bar
//! display types, bulk dumps, Micro Edit) is not modelled; the panel is
//! for playing games at, not servicing.

use crate::engine::{Engine, Monitor, Mt32Mode, PartSetting, DEMO_SONGS, PARTS};

/// One side of a left/right pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
}

/// The eight left/right pairs, as the fascia groups them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pair {
    Part,
    Instrument,
    Level,
    Pan,
    Reverb,
    Chorus,
    KeyShift,
    MidiCh,
}

/// A semantic press. The host's pointer mechanics (latching one button
/// to press another, as a mouse must) resolve to these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    All,
    Mute,
    Arrow(Pair, Dir),
    /// Both halves of a pair together.
    Both(Pair),
    /// ALL and MUTE together: monitor.
    Monitor,
}

/// Text or a picture the engine took off the wire for the display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    /// A Displayed Letter: it takes the name line and stays until a
    /// button sends it away.
    Text(String),
    /// A Displayed Dot Data frame, rows top to bottom, bit `c` = column.
    Picture([u16; 16]),
}

/// What the glass shows. Strings come composed and clipped; the host
/// renders them and adds nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// The PART field: `01`..`16`, `ALL`, or empty while a message or a
    /// start screen owns the line.
    pub part: String,
    /// The INST number field: `001`..`128`, or empty.
    pub instrument: String,
    /// The name area: instrument name, message text, or a prompt.
    pub name: String,
    /// A second, smaller line under the name; the boot splash's
    /// version and date. Empty almost always.
    pub subtitle: String,
    pub level: String,
    pub pan: String,
    pub reverb: String,
    pub chorus: String,
    pub key_shift: String,
    pub midi_ch: String,
    /// Sixteen columns, left to right; bit `r` = the dot `r` rows up
    /// from the bottom of the matrix.
    pub bars: [u16; PARTS],
    pub all_led: bool,
    pub mute_led: bool,
    /// Whether the MUTE lamp should blink -- the monitor's sign; it
    /// overrides the steady lamp while it stands.
    pub mute_blink: bool,
    /// Whether MT-32 translation is on, for a badge if the host wears
    /// one.
    pub translating: bool,
}

/// Something the panel cannot do alone and asks the host to mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelRequest {
    /// MT-32 mode was switched at the fascia; the host should carry the
    /// choice into its own options so a power cycle keeps it.
    Mt32Mode(Mt32Mode),
    /// A factory reset was confirmed at the fascia: the host owns the
    /// bank files, so the host puts the built-in one back.
    ResetSoundfont,
}

/// What the panel is showing over the home screen, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Home,
    /// A pair held together: its values across the parts, as bars.
    View(Pair),
    /// "Init MT-32, Sure?" -- ALL turns it on, MUTE turns it off.
    ConfirmMt32,
    /// "Init GS, Sure?" -- ALL returns the unit to the GS basic
    /// setting (system functions kept), MUTE carries on.
    ConfirmGs,
    /// "Init All, Sure?" -- ALL is the factory preset: the host puts
    /// the built-in bank back and every setting returns home. MUTE
    /// carries on.
    ConfirmAll,
    /// The second after a factory reset was confirmed: the host is
    /// putting the built-in bank back, and every button waits.
    Initializing,
    /// The undocumented screen: the credits roll until ALL or MUTE
    /// lets the boot carry on.
    Credits,
    /// The system menu (ALL lit, PART pair): ALL and MUTE walk the
    /// items, the INSTRUMENT arrows set the value live, the PART pair
    /// leaves.
    SystemMenu {
        item: usize,
    },
    /// The part menu (ALL dark, PART pair): as the system menu, and
    /// the PART arrows move between parts with the item held.
    PartMenu {
        item: usize,
    },
    /// Variation select (INSTRUMENT pair on a part): the INSTRUMENT
    /// arrows walk the banks the font offers for the part's
    /// instrument; the pair again leaves.
    VariationEdit,
    /// The unit playing to itself: ALL plays, MUTE stops, PART picks
    /// the song, ALL and MUTE together leave. Reached with both PART
    /// halves held through power-on.
    Demo {
        song: usize,
        playing: bool,
    },
}

/// The part menu: the mkII's own list, order and spelling, checked
/// against a real unit -- Part Mode is simply the first setting --
/// with the wire-only pedals appended as this unit's extras.
const PART_MENU: [(&str, PartItem); 26] = [
    ("Part Mode", PartItem::Mode),
    ("M/P Mode", PartItem::MonoPoly),
    ("Voice Rsv", PartItem::VoiceReserve),
    ("Fine Tune", PartItem::FineTune),
    ("Rx Bank Sel", PartItem::RxBank),
    ("Rx NRPN", PartItem::RxNrpn),
    ("Bend Range", PartItem::BendRange),
    ("Mod. Depth", PartItem::ModDepth),
    ("K. Range L", PartItem::KeyRangeL),
    ("K. Range H", PartItem::KeyRangeH),
    ("Velo Depth", PartItem::VeloDepth),
    ("Velo Offset", PartItem::VeloOffset),
    ("Vib. Rate", PartItem::Offset(0x08)),
    ("Vib. Depth", PartItem::Offset(0x09)),
    ("Vib. Delay", PartItem::Offset(0x0A)),
    ("Cutoff Freq", PartItem::Offset(0x20)),
    ("Resonance", PartItem::Offset(0x21)),
    ("Attack Tm.", PartItem::Offset(0x63)),
    ("Decay Tm.", PartItem::Offset(0x64)),
    ("Release Tm.", PartItem::Offset(0x66)),
    ("Modulation", PartItem::Cc(0x01)),
    ("Expression", PartItem::Cc(0x0B)),
    ("Portamento", PartItem::Switch(0x41)),
    ("Porta. Tm.", PartItem::Cc(0x05)),
    ("Sostenuto", PartItem::Switch(0x42)),
    ("Soft Pedal", PartItem::Switch(0x43)),
];

/// How a part-menu value reads, steps and prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartItem {
    /// Norm or Drum: which kind of musician the part is.
    Mode,
    /// Poly or Mono.
    MonoPoly,
    /// Voice Reserve, kept as the unit keeps it.
    VoiceReserve,
    /// Fine Tune, -12..=+12 across the RPN's semitone each way.
    FineTune,
    /// The part's reception switches.
    RxBank,
    RxNrpn,
    /// RPN 0 in semitones, -24..=+24.
    BendRange,
    /// How far the mod wheel reaches, GS factory 10.
    ModDepth,
    /// The playable window's ends, shown as note names.
    KeyRangeL,
    KeyRangeH,
    /// The velocity curve, 64/64 neutral.
    VeloDepth,
    VeloOffset,
    /// A plain controller, 0-127.
    Cc(u8),
    /// A pedal controller shown as On/Off.
    Switch(u8),
    /// A GS NRPN tone modify (msb 0x01), shown as the unit's signed
    /// offset: the wire's 14-114 printed as -50..=+50.
    Offset(u8),
}

impl PartItem {
    fn value(self, engine: &Engine, part: usize) -> i32 {
        match self {
            Self::Mode => engine.part_drums(part) as i32,
            Self::MonoPoly => engine.part_mono(part) as i32,
            Self::VoiceReserve => engine.part_voice_reserve(part) as i32,
            Self::FineTune => engine.part_fine_tune(part) as i32,
            Self::RxBank => engine.part_rx_bank(part) as i32,
            Self::RxNrpn => engine.part_rx_nrpn(part) as i32,
            Self::BendRange => engine.part_bend_range(part) as i32,
            Self::ModDepth => engine.part_mod_depth(part) as i32,
            Self::KeyRangeL => engine.part_key_range(part).0 as i32,
            Self::KeyRangeH => engine.part_key_range(part).1 as i32,
            Self::VeloDepth => engine.part_velo_sens(part).0 as i32,
            Self::VeloOffset => engine.part_velo_sens(part).1 as i32,
            Self::Cc(cc) | Self::Switch(cc) => engine.part_cc_value(part, cc) as i32,
            Self::Offset(lsb) => engine.part_nrpn_wire(part, 0x01, lsb) as i32 - 64,
        }
    }

    fn set(self, engine: &mut Engine, part: usize, value: i32) {
        match self {
            Self::Mode => engine.set_part_drums(part, value != 0),
            Self::MonoPoly => engine.set_part_mono(part, value != 0),
            Self::VoiceReserve => engine.set_part_voice_reserve(part, value as u8),
            Self::FineTune => engine.set_part_fine_tune(part, value as i8),
            Self::RxBank => engine.set_part_rx_bank(part, value != 0),
            Self::RxNrpn => engine.set_part_rx_nrpn(part, value != 0),
            Self::BendRange => engine.set_part_bend_range(part, value as i8),
            Self::ModDepth => engine.set_part_mod_depth(part, value as u8),
            Self::KeyRangeL => {
                let (_, hi) = engine.part_key_range(part);
                engine.set_part_key_range(part, (value as u8).min(hi), hi);
            }
            Self::KeyRangeH => {
                let (lo, _) = engine.part_key_range(part);
                engine.set_part_key_range(part, lo, (value as u8).max(lo));
            }
            Self::VeloDepth => {
                let (_, offset) = engine.part_velo_sens(part);
                engine.set_part_velo_sens(part, value as u8, offset);
            }
            Self::VeloOffset => {
                let (depth, _) = engine.part_velo_sens(part);
                engine.set_part_velo_sens(part, depth, value as u8);
            }
            Self::Cc(cc) => engine.send_part_cc(part, cc, value as u8),
            Self::Switch(cc) => engine.send_part_cc(part, cc, if value != 0 { 127 } else { 0 }),
            Self::Offset(lsb) => engine.send_part_nrpn(part, 0x01, lsb, (value + 64) as u8),
        }
    }

    /// The value's travel, in its own display units.
    fn range(self) -> (i32, i32) {
        match self {
            Self::Mode | Self::MonoPoly | Self::Switch(_) | Self::RxBank | Self::RxNrpn => (0, 1),
            Self::VoiceReserve => (0, 28),
            Self::FineTune => (-12, 12),
            Self::BendRange => (-24, 24),
            Self::ModDepth | Self::VeloDepth | Self::VeloOffset | Self::Cc(_) => (0, 127),
            Self::KeyRangeL | Self::KeyRangeH => (0, 127),
            Self::Offset(_) => (-50, 50),
        }
    }

    /// The value as the glass prints it.
    fn print(self, value: i32) -> String {
        match self {
            Self::Mode => if value != 0 { "Drum" } else { "Norm" }.to_string(),
            Self::MonoPoly => if value != 0 { "Mono" } else { "Poly" }.to_string(),
            Self::Switch(_) => if value >= 64 { "On" } else { "Off" }.to_string(),
            Self::RxBank | Self::RxNrpn => if value != 0 { "On" } else { "Off" }.to_string(),
            Self::BendRange | Self::FineTune => shift_label(value as i8),
            Self::KeyRangeL | Self::KeyRangeH => note_name(value as u8),
            Self::Offset(_) => shift_label(value.clamp(-50, 50) as i8),
            _ => value.to_string(),
        }
    }

    /// A switch steps between its two states; everything else walks a
    /// notch at a time.
    fn stepped(self, value: i32, step: i32) -> i32 {
        let (lo, hi) = self.range();
        match self {
            Self::Switch(_) => {
                if step > 0 {
                    127
                } else {
                    0
                }
            }
            _ => (value + step).clamp(lo, hi),
        }
    }
}

/// The system menu: the unit's own list, less the LCD contrast a real
/// panel needs and an emulated one does not.
const SYSTEM_MENU: [(&str, SystemItem); 9] = [
    ("M. Tune", SystemItem::MasterTune),
    ("Reverb", SystemItem::ReverbType),
    ("Chorus", SystemItem::ChorusType),
    ("Display", SystemItem::Display),
    ("Peak Hold", SystemItem::PeakHold),
    ("Rx Inst Chg", SystemItem::RxInstChg),
    ("Rx SysEx", SystemItem::RxSysEx),
    ("Rx GS Reset", SystemItem::RxGsReset),
    ("Back Up", SystemItem::BackUp),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemItem {
    MasterTune,
    ReverbType,
    ChorusType,
    Display,
    PeakHold,
    RxInstChg,
    RxSysEx,
    RxGsReset,
    BackUp,
}

/// The reverb characters' names, 0-7 on the wire's macro order.
const REVERB_TYPES: [&str; 8] = [
    "Room1",
    "Room2",
    "Room3",
    "Hall1",
    "Hall2",
    "Plate",
    "Delay",
    "Pan Delay",
];

impl SystemItem {
    fn value(self, engine: &Engine) -> i32 {
        match self {
            Self::MasterTune => engine.master_tune_tenths() as i32,
            Self::ReverbType => engine.reverb_type() as i32,
            Self::ChorusType => engine.chorus_type().index() as i32,
            Self::Display => engine.display_type() as i32,
            Self::PeakHold => engine.peak_hold() as i32,
            Self::RxInstChg => engine.rx_inst_chg() as i32,
            Self::RxSysEx => engine.rx_sysex() as i32,
            Self::RxGsReset => engine.rx_gs_reset() as i32,
            Self::BackUp => engine.backup() as i32,
        }
    }

    fn set(self, engine: &mut Engine, value: i32) {
        match self {
            Self::MasterTune => engine.set_master_tune_tenths(value as u16),
            Self::ReverbType => engine.set_reverb_type(value as u8),
            Self::ChorusType => {
                engine.set_chorus_type(crate::synth::ChorusType::from_index(value as u8))
            }
            Self::Display => engine.set_display_type(value as u8),
            Self::PeakHold => engine.set_peak_hold(value as u8),
            Self::RxInstChg => engine.set_rx_inst_chg(value != 0),
            Self::RxSysEx => engine.set_rx_sysex(value != 0),
            Self::RxGsReset => engine.set_rx_gs_reset(value != 0),
            Self::BackUp => engine.set_backup(value != 0),
        }
    }

    fn range(self) -> (i32, i32) {
        match self {
            Self::MasterTune => (4153, 4662),
            Self::ReverbType | Self::ChorusType => (0, 7),
            Self::Display => (1, 8),
            Self::PeakHold => (0, 3),
            _ => (0, 1),
        }
    }

    fn print(self, value: i32) -> String {
        match self {
            Self::MasterTune => format!("{}.{}", value / 10, value % 10),
            Self::ReverbType => REVERB_TYPES[value.clamp(0, 7) as usize].to_string(),
            Self::ChorusType => crate::synth::ChorusType::from_index(value as u8)
                .label()
                .to_string(),
            Self::Display => format!("Type{value}"),
            Self::PeakHold => {
                if value == 0 {
                    "Off".to_string()
                } else {
                    format!("Type{value}")
                }
            }
            _ => if value != 0 { "On" } else { "Off" }.to_string(),
        }
    }
}

/// The menu line as the unit composes it: the item's name at the
/// left, the value right-justified so its last character sits in the
/// twentieth column.
fn menu_line(name: &str, value: &str) -> String {
    let label = format!(">{name}:");
    let pad = NAME_COLS.saturating_sub(label.len() + value.len());
    format!("{label}{}{value}", " ".repeat(pad))
}

/// The note name for a key number, spaced as the unit prints it:
/// C -1 to G 9.
fn note_name(key: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = key as i32 / 12 - 1;
    format!("{}{octave}", NAMES[key as usize % 12])
}

/// A message on the name line.
#[derive(Debug, Clone)]
enum Message {
    /// A game's letter: stays until a button sends it away, scrolling
    /// round when it is long.
    Letter {
        text: Vec<char>,
        started: Option<u64>,
    },
    /// The panel's own notice: says its piece and goes.
    Notice { text: String, started: Option<u64> },
}

/// The boot line: the greeting, then the soundfont introducing itself
/// by its own name.
const GREETING_MS: u64 = 1500;
const BANK_MS: u64 = 1500;
/// How long the unit says Initializing... while a confirmed factory
/// reset puts the built-in bank back.
const INIT_MS: u64 = 1000;
/// What the credits boot scrolls across the name line.
const CREDITS: &str = "COPPERSYNTH made with love by Lee Hobson (hobbo91). \
This work stands on the foundation of RustySynth and MeltySynth by \
Nobuaki Tanaka. The GeneralUser GS SoundFont by S. Christian Collins. \
The MT-32 translation layer is thanks to the ScummVM project. \
Thank you for using Coppersynth :)";
/// How long a notice stays up.
const NOTICE_MS: u64 = 2000;
/// A long letter rests, steps a column at a time, rests on its tail,
/// and comes round again.
const SCROLL_START_MS: u64 = 600;
const SCROLL_STEP_MS: u64 = 300;
const SCROLL_TAIL_MS: u64 = 1500;
/// How long a dot picture holds the matrix past its last frame.
const PICTURE_MS: u64 = 3000;
/// The prompt's lamps flash about once a second.
const FLASH_MS: u64 = 500;
/// The rest between demo songs.
const DEMO_GAP_MS: u64 = 3000;
/// The text area's width in characters -- a full SF2 preset name,
/// which the spec caps at twenty.
pub const NAME_COLS: usize = 20;
/// The bar matrix: sixteen columns of sixteen dots.
pub const BAR_ROWS: u32 = 16;
/// The power-on show, played on the matrix from the moment the unit
/// wakes and never interrupted by the meters: a sparkle condenses
/// into the letters, they pulse, burst, give way to the framed badge,
/// and dissolve to the resting baseline -- the hardware's own boot
/// choreography, wearing this unit's initials.
const BOOT_SHOW_MS: u64 = 3_850;
/// The letters CS as column masks, twelve rows tall, C left, S right.
const BOOT_CS: [u16; PARTS] = [
    0x0000, 0x0000, 0x1FF8, 0x300C, 0x300C, 0x300C, 0x3C3C, 0x0000, 0x0000, 0x1F18, 0x318C, 0x318C,
    0x318C, 0x38FC, 0x0000, 0x0000,
];
/// The same letters squeezed to their spines, for the pulse.
const BOOT_CS_NARROW: [u16; PARTS] = [
    0x0000, 0x0000, 0x0000, 0x1FF8, 0x300C, 0x3C3C, 0x0000, 0x0000, 0x0000, 0x0000, 0x1F18, 0x318C,
    0x38FC, 0x0000, 0x0000, 0x0000,
];
/// The badge: double borders top and bottom, two pillars standing in
/// the frame.
const BOOT_BADGE: [u16; PARTS] = [
    0x0000, 0xC003, 0xC003, 0xFFFF, 0xFFFF, 0xFFFF, 0xC003, 0xC003, 0xC003, 0xC003, 0xFFFF, 0xFFFF,
    0xFFFF, 0xC003, 0xC003, 0x0000,
];

/// A deterministic scatter: whether dot (`c`, `r`) sparkles under
/// `salt`, one dot in `density`.
fn boot_scatter(c: usize, r: u32, salt: u64, density: u64) -> bool {
    let mut x = ((c as u64) << 32) ^ ((r as u64) << 8) ^ salt.wrapping_mul(0x9E3779B97F4A7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51AFD7ED558CCD);
    x ^= x >> 33;
    x.is_multiple_of(density)
}

/// Each dot's own moment within a phase, spread deterministically.
fn boot_moment(c: usize, r: u32, from: u64, span: u64) -> u64 {
    let mut x = ((c as u64) << 16) ^ (r as u64).wrapping_mul(0x2545F4914F6CDD1D);
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 32;
    from + x % span
}

/// The show's frame for `elapsed` milliseconds since the wake.
fn boot_show_bars(elapsed: u64) -> [u16; PARTS] {
    let mut bars = [0u16; PARTS];
    let e = elapsed;
    for (c, bar) in bars.iter_mut().enumerate() {
        for r in 0..BAR_ROWS {
            let bit = 1u16 << r;
            let lit = match e {
                // A breath on the resting baseline.
                0..=69 => r == 0,
                // The sparkle condenses into the letters: each of
                // their dots arrives at its own moment, while stray
                // dots glitter and thin out.
                70..=999 => {
                    let target = BOOT_CS[c] & bit != 0;
                    if target {
                        e >= boot_moment(c, r, 150, 800)
                    } else {
                        let density = 6 + (e - 70) / 120;
                        boot_scatter(c, r, e / 70, density)
                    }
                }
                // The letters stand, squeeze to their spines, and
                // stand again.
                1_000..=1_599 => BOOT_CS[c] & bit != 0,
                1_600..=1_749 => BOOT_CS_NARROW[c] & bit != 0,
                1_750..=1_899 => BOOT_CS[c] & bit != 0,
                // The burst: the letters' dots scatter and die.
                1_900..=2_149 => {
                    let density = 3 + (e - 1_900) / 60;
                    boot_scatter(c, r, e / 70, density)
                }
                // The badge sweeps in from the left and holds.
                2_150..=3_249 => BOOT_BADGE[c] & bit != 0 && e >= 2_150 + c as u64 * 12,
                // The dissolve: each badge dot dies at its own moment,
                // the last glitter thinning behind it.
                3_250..=3_699 => {
                    let badge = BOOT_BADGE[c] & bit != 0;
                    (badge && e < boot_moment(c, r, 3_250, 400)) || boot_scatter(c, r, e / 70, 24)
                }
                // And the resting baseline, ready for the meters.
                _ => r == 0,
            };
            if lit {
                *bar |= bit;
            }
        }
    }
    bars
}
/// How fast a bar falls once the sound under it has, full scale per
/// millisecond; the peak dot holds, then falls a row at a time.
const BAR_FALL_PER_MS: f32 = 0.006;
const PEAK_HOLD_MS: u64 = 600;
const PEAK_FALL_MS: u64 = 45;
#[derive(Debug, Clone, Copy, Default)]
struct PeakDot {
    row: u32,
    held_at: u64,
}

/// The panel. It owns nothing audible: every value it shows is read
/// from the engine when the screen is composed, and every edit goes
/// straight in.
#[derive(Debug)]
pub struct FrontPanel {
    all: bool,
    part: usize,
    mode: Mode,
    boot_started: Option<u64>,
    boot_done: bool,
    /// When the Initializing... screen went up.
    init_started: Option<u64>,
    /// When the credits started rolling, for their scroll clock.
    credits_started: Option<u64>,
    message: Option<Message>,
    picture: Option<([u16; PARTS], Option<u64>)>,
    /// When the playing demo song ran out, while the gap rests.
    demo_ended: Option<u64>,
    /// The matrix as displayed: live level with a fall, and a peak dot.
    shown: [f32; PARTS],
    peaks: [PeakDot; PARTS],
    last_tick: Option<u64>,
}

impl Default for FrontPanel {
    fn default() -> Self {
        Self {
            all: false,
            part: 0,
            mode: Mode::Home,
            boot_started: None,
            boot_done: false,
            init_started: None,
            credits_started: None,
            message: None,
            picture: None,
            demo_ended: None,
            shown: [0.0; PARTS],
            peaks: [PeakDot::default(); PARTS],
            last_tick: None,
        }
    }
}

impl FrontPanel {
    /// The buttons held while the power came on, read the way the unit
    /// reads its own fascia at start-up.
    pub fn power_on_held(&mut self, engine: &mut Engine, held: &[Button]) {
        let is =
            |want: &[Button]| held.len() == want.len() && want.iter().all(|b| held.contains(b));
        if is(&[Button::Both(Pair::Part)]) {
            // Both PART halves: demo mode, armed on song one and
            // waiting for ALL, MUTE lamp lit, exactly as the unit
            // arrives in it. The demo songs are GS songs: the unit
            // formats itself to the basic setting on the way in, and
            // MIDI IN stays shut for the whole visit.
            engine.gs_reset();
            engine.set_wire_closed(true);
            self.mode = Mode::Demo {
                song: 0,
                playing: false,
            };
            self.boot_done = true;
        } else if is(&[Button::Arrow(Pair::Instrument, Dir::Left)]) {
            // A unit switched on into a service screen skips its boot
            // line; the question is the greeting.
            self.mode = Mode::ConfirmMt32;
            self.boot_done = true;
        } else if is(&[Button::Arrow(Pair::Instrument, Dir::Right)]) {
            // The other half of the pair asks the GS question.
            self.mode = Mode::ConfirmGs;
            self.boot_done = true;
        } else if is(&[Button::Both(Pair::Instrument)]) {
            // The whole pair asks the factory question.
            self.mode = Mode::ConfirmAll;
            self.boot_done = true;
        } else if is(&[Button::Both(Pair::MidiCh), Button::Both(Pair::Instrument)]) {
            // Undocumented, as the tradition demands: the credits roll
            // until ALL or MUTE lets the boot go on.
            self.mode = Mode::Credits;
            self.boot_done = true;
        }
    }

    /// The power on its way out: demo mode does not survive the
    /// switch. Leaving the demo by any door -- the exit combo or the
    /// power itself -- returns the unit to the GS basic setting, or
    /// the demo songs' own housekeeping would go into the battery as
    /// if it were yours.
    pub fn power_off(&mut self, engine: &mut Engine) {
        if matches!(self.mode, Mode::Demo { .. }) {
            engine.demo_stop();
            engine.gs_reset();
            engine.set_wire_closed(false);
            self.mode = Mode::Home;
        }
    }

    /// Put the panel into the Initializing... hold, for a host whose
    /// bank swap rebuilds the unit around the new font: the freshly
    /// attached panel shows the hold's second, then the ordinary boot
    /// runs -- exactly as if it had survived the swap.
    pub fn begin_initializing(&mut self) {
        self.mode = Mode::Initializing;
        self.boot_done = true;
        self.init_started = None;
    }

    /// Whether a confirm screen owns the glass -- the host holds its
    /// latching gestures back while one does, so the flashing lamps
    /// keep their one meaning. The menus are not in this set: their
    /// own grammar leans on the latched pairs.
    pub fn in_edit(&self) -> bool {
        matches!(
            self.mode,
            Mode::ConfirmMt32 | Mode::ConfirmGs | Mode::ConfirmAll | Mode::Initializing
        )
    }

    /// A press. Anything the panel cannot mirror alone comes back as a
    /// request for the host.
    pub fn button(&mut self, engine: &mut Engine, b: Button) -> Option<PanelRequest> {
        // The credits hold the glass until ALL or MUTE lets the boot
        // carry on; every other button stays quiet.
        if self.mode == Mode::Credits {
            if matches!(b, Button::All | Button::Mute) {
                self.mode = Mode::Home;
                self.credits_started = None;
                self.boot_done = false;
                self.boot_started = None;
            }
            return None;
        }
        // While the unit says Initializing..., it means it.
        if self.mode == Mode::Initializing {
            return None;
        }
        // The GS prompt: ALL returns the unit to the GS basic setting,
        // MUTE carries on as it stands.
        if self.mode == Mode::ConfirmGs {
            return match b {
                Button::All => {
                    engine.gs_reset();
                    self.mode = Mode::Home;
                    self.notice("GS Initialized");
                    None
                }
                Button::Mute => {
                    self.mode = Mode::Home;
                    None
                }
                _ => None,
            };
        }
        // The factory prompt: ALL initialises everything (the host
        // swaps the bank back while the screen holds), MUTE carries on.
        if self.mode == Mode::ConfirmAll {
            return match b {
                Button::All => {
                    engine.factory_reset();
                    self.mode = Mode::Initializing;
                    Some(PanelRequest::ResetSoundfont)
                }
                Button::Mute => {
                    self.mode = Mode::Home;
                    None
                }
                _ => None,
            };
        }
        // The menus: ALL and MUTE walk the items, the INSTRUMENT
        // arrows set the value live -- exactly as the values apply on
        // the unit -- and the PART pair leaves. In the part menu the
        // PART arrows move between parts with the item held.
        if let Mode::SystemMenu { item } = self.mode {
            match b {
                Button::All => {
                    self.mode = Mode::SystemMenu {
                        item: item.saturating_sub(1),
                    };
                }
                Button::Mute => {
                    self.mode = Mode::SystemMenu {
                        item: (item + 1).min(SYSTEM_MENU.len() - 1),
                    };
                }
                Button::Arrow(Pair::Instrument, dir) => {
                    let (_, kind) = SYSTEM_MENU[item];
                    let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                    let (lo, hi) = kind.range();
                    kind.set(engine, (kind.value(engine) + step).clamp(lo, hi));
                }
                Button::Both(Pair::Part) => self.mode = Mode::Home,
                _ => {}
            }
            return None;
        }
        if let Mode::PartMenu { item } = self.mode {
            // ALL walks back toward Part Mode at the head, MUTE walks
            // forward; neither wraps -- checked against a real unit.
            match b {
                Button::All => {
                    self.mode = Mode::PartMenu {
                        item: item.saturating_sub(1),
                    };
                }
                Button::Mute => {
                    self.mode = Mode::PartMenu {
                        item: (item + 1).min(PART_MENU.len() - 1),
                    };
                }
                Button::Arrow(Pair::Instrument, dir) => {
                    let (_, kind) = PART_MENU[item];
                    let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                    kind.set(
                        engine,
                        self.part,
                        kind.stepped(kind.value(engine, self.part), step),
                    );
                }
                Button::Arrow(Pair::Part, dir) => {
                    let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                    self.part = (self.part as i32 + step).rem_euclid(PARTS as i32) as usize;
                }
                Button::Both(Pair::Part) => self.mode = Mode::Home,
                _ => {}
            }
            return None;
        }
        // Variation select: the INSTRUMENT arrows walk the banks the
        // font offers for the part's instrument, sounding as they go;
        // the pair again puts the plain number back on the glass.
        if self.mode == Mode::VariationEdit {
            match b {
                Button::Arrow(Pair::Instrument, dir) => {
                    let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                    if let Some(bank) = engine.neighbour_variation(self.part, step) {
                        engine.set_part_variation(self.part, bank);
                    }
                }
                Button::Both(Pair::Instrument) | Button::Both(Pair::Part) => {
                    self.mode = Mode::Home;
                }
                _ => {}
            }
            return None;
        }
        // The MT-32 prompt takes ALL as on and MUTE as off, and ignores
        // the rest.
        if self.mode == Mode::ConfirmMt32 {
            return match b {
                Button::All => {
                    self.mode = Mode::Home;
                    engine.set_mt32_mode(Mt32Mode::On);
                    self.notice("MT-32 Enabled");
                    Some(PanelRequest::Mt32Mode(Mt32Mode::On))
                }
                Button::Mute => {
                    self.mode = Mode::Home;
                    engine.set_mt32_mode(Mt32Mode::Off);
                    self.notice("MT-32 Disabled");
                    Some(PanelRequest::Mt32Mode(Mt32Mode::Off))
                }
                _ => None,
            };
        }
        // The demo transport: ALL plays from the top, MUTE stops, the
        // PART arrows pick the song -- switching mid-song plays on.
        if let Mode::Demo { song, playing } = self.mode {
            match b {
                Button::Monitor => {
                    engine.demo_stop();
                    // Leaving is the mirror of arriving: the GS basic
                    // setting again, and MIDI IN opens back up.
                    engine.gs_reset();
                    engine.set_wire_closed(false);
                    self.mode = Mode::Home;
                    self.demo_ended = None;
                    return None;
                }
                Button::All => {
                    engine.demo_play(song);
                    self.mode = Mode::Demo {
                        song,
                        playing: true,
                    };
                    self.demo_ended = None;
                }
                Button::Mute => {
                    engine.demo_stop();
                    self.mode = Mode::Demo {
                        song,
                        playing: false,
                    };
                    self.demo_ended = None;
                }
                Button::Arrow(Pair::Part, dir) => {
                    let count = DEMO_SONGS.len() as i32;
                    let step = match dir {
                        Dir::Left => -1,
                        Dir::Right => 1,
                    };
                    let song = (song as i32 + step).rem_euclid(count) as usize;
                    if playing {
                        engine.demo_play(song);
                    }
                    self.mode = Mode::Demo { song, playing };
                    self.demo_ended = None;
                }
                _ => {}
            }
            return None;
        }
        // A game's letter stays until a button sends it away; that
        // press is spent on the dismissal.
        if matches!(self.message, Some(Message::Letter { .. })) {
            self.message = None;
            return None;
        }
        // A solo is a passing state: MUTE lets it go and the press is
        // spent; any other button lets it go on the way to its own
        // meaning, so the fascia is never stuck listening. The one
        // exception is the PART pair, which carries the solo along to
        // the part it selects.
        if engine.monitor() != Monitor::Off {
            if matches!(b, Button::Mute | Button::Monitor) {
                engine.set_monitor(Monitor::Off);
                return None;
            }
            if !matches!(b, Button::Arrow(Pair::Part, _)) {
                engine.set_monitor(Monitor::Off);
            }
        }
        match b {
            Button::All => self.all = !self.all,
            Button::Mute => self.press_mute(engine),
            Button::Monitor => self.press_monitor(engine),
            // The PART pair opens the menus: the system menu with ALL
            // lit, the part menu with it dark.
            Button::Both(Pair::Part) => {
                self.mode = if self.all {
                    Mode::SystemMenu { item: 0 }
                } else {
                    Mode::PartMenu { item: 0 }
                };
            }
            // The INSTRUMENT pair opens variation select on a part.
            Button::Both(Pair::Instrument) if !self.all => {
                self.mode = Mode::VariationEdit;
            }
            Button::Both(pair) => self.toggle_view(pair),
            Button::Arrow(pair, dir) => self.press_arrow(engine, pair, dir),
        }
        None
    }

    /// The VOLUME knob, 0 at the bottom of its travel to 1 at the top.
    /// A pot after the DAC, exactly as on the unit.
    pub fn volume(&mut self, engine: &mut Engine, value: f32) {
        engine.set_output_gain(value);
    }

    /// Text and pictures the engine took off the wire.
    pub fn feed(&mut self, feed: Feed) {
        match feed {
            Feed::Text(text) => {
                self.message = Some(Message::Letter {
                    text: text.chars().take(32).collect(),
                    started: None,
                });
            }
            Feed::Picture(rows) => {
                // Rows arrive top to bottom; the matrix is addressed as
                // columns with bit 0 at the bottom.
                let mut columns = [0u16; PARTS];
                for (r, row) in rows.iter().enumerate() {
                    for (c, column) in columns.iter_mut().enumerate() {
                        if row & (1 << c) != 0 {
                            *column |= 1 << (15 - r);
                        }
                    }
                }
                // The deadline runs from the next draw, so a stream of
                // frames animates without ever timing out mid-stream.
                self.picture = Some((columns, None));
            }
        }
    }

    /// Compose the glass.
    pub fn screen(&mut self, engine: &mut Engine, now_ms: u64) -> Screen {
        // The Initializing... hold, once it has had its second, lets
        // the unit boot as on any morning -- onto the built-in bank
        // the host has just put back.
        if self.mode == Mode::Initializing {
            let since = *self.init_started.get_or_insert(now_ms);
            if now_ms.saturating_sub(since) >= INIT_MS {
                self.mode = Mode::Home;
                self.init_started = None;
                self.boot_done = false;
                self.boot_started = None;
            }
        }
        let bars = self.compose_bars(engine, now_ms);
        let mut screen = self.home_screen(engine, bars);
        if let Some((line, subtitle)) = self.boot_line(engine, now_ms) {
            screen.part = String::new();
            screen.instrument = String::new();
            screen.name = line;
            screen.subtitle = subtitle;
            // Nothing behind the boot line has a value to show.
            dash_values(&mut screen);
            return finished(screen);
        }
        match self.mode {
            Mode::ConfirmMt32 | Mode::ConfirmGs | Mode::ConfirmAll => {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = match self.mode {
                    Mode::ConfirmMt32 => "Init MT-32, Sure?",
                    Mode::ConfirmGs => "Init GS, Sure?",
                    _ => "Init All, Sure?",
                }
                .to_string();
                // ALL says yes and MUTE says no; their lamps flash to
                // say the question is theirs.
                let flash_on = (now_ms / FLASH_MS).is_multiple_of(2);
                screen.all_led = flash_on;
                screen.mute_led = flash_on;
            }
            Mode::Initializing => {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = "Initializing...".to_string();
                dash_values(&mut screen);
            }
            Mode::SystemMenu { item } => {
                let (name, kind) = SYSTEM_MENU[item];
                screen.part = "ALL".to_string();
                screen.instrument = String::new();
                screen.name = menu_line(name, &kind.print(kind.value(engine)));
                dash_values(&mut screen);
                screen.all_led = true;
            }
            Mode::PartMenu { item } => {
                let (name, kind) = PART_MENU[item];
                screen.part = format!("{:02}", self.part + 1);
                screen.instrument = String::new();
                screen.name = menu_line(name, &kind.print(kind.value(engine, self.part)));
                dash_values(&mut screen);
                screen.all_led = false;
            }
            Mode::VariationEdit => {
                // The instrument field wears the variation number and
                // the name its slash, exactly the unit's own display.
                let view = engine.part_view(self.part);
                screen.part = format!("{:02}", self.part + 1);
                screen.instrument = format!("{:03}", engine.part_bank(self.part));
                screen.name = format!("/{}", view.name).chars().take(NAME_COLS).collect();
            }
            Mode::Credits => {
                screen.part = String::new();
                screen.instrument = String::new();
                // A little lightshow while the credits roll -- the live
                // meters wait for the boot. Three acts take five-second
                // turns: a travelling wave, a breathing two-wave
                // interference shimmer, and a blob bouncing to and fro.
                // All of it is a pure function of the clock, so replays
                // draw the same show.
                let t = now_ms as f32 / 1000.0;
                let act = (now_ms / 5_000) % 3;
                for (c, bar) in screen.bars.iter_mut().enumerate() {
                    let x = c as f32;
                    let h = match act {
                        0 => 8.5 + 7.5 * (t * 5.24 - x * 0.45).sin(),
                        1 => {
                            let breathe = 4.5 + 3.0 * (t * 1.4).sin();
                            8.5 + breathe
                                * (0.6 * (t * 5.2 - x * 0.45).sin()
                                    + 0.4 * (x * 0.8 - t * 8.8).sin())
                        }
                        _ => {
                            let sweep = (t * 0.4).fract();
                            let pos = if sweep < 0.5 {
                                sweep * 2.0
                            } else {
                                2.0 - sweep * 2.0
                            } * 15.0;
                            let arc = (t * 2.4).sin().abs();
                            let d = (x - pos).abs();
                            1.0 + (15.0 * arc - d * d * 1.8).max(0.0)
                        }
                    };
                    let height = h.round().clamp(1.0, 16.0) as u32;
                    *bar = ((1u32 << height) - 1) as u16;
                }
                let since = *self.credits_started.get_or_insert(now_ms);
                let text: Vec<char> = CREDITS.chars().collect();
                screen.name = scroll_line(&text, since, now_ms);
                // The version and date ride under the roll, exactly as
                // the documented print reads.
                screen.subtitle = version_line();
                dash_values(&mut screen);
                // ALL or MUTE lets the boot go on; their lamps flash
                // to say so.
                let flash_on = (now_ms / FLASH_MS).is_multiple_of(2);
                screen.all_led = flash_on;
                screen.mute_led = flash_on;
            }
            Mode::Demo { song, playing } => {
                // A finished song rests, then the chain moves on -- or
                // with one song, round it comes again.
                if playing && engine.demo_finished() {
                    let since = *self.demo_ended.get_or_insert(now_ms);
                    if now_ms.saturating_sub(since) >= DEMO_GAP_MS {
                        let song = (song + 1) % DEMO_SONGS.len();
                        engine.demo_play(song);
                        self.mode = Mode::Demo {
                            song,
                            playing: true,
                        };
                        self.demo_ended = None;
                    }
                }
                let Mode::Demo { song, playing } = self.mode else {
                    unreachable!()
                };
                screen.part = format!("S-{}", song + 1);
                screen.instrument = String::new();
                screen.name = DEMO_SONGS[song].chars().take(NAME_COLS).collect();
                dash_values(&mut screen);
                screen.all_led = playing;
                screen.mute_led = !playing;
            }
            Mode::Home | Mode::View(_) => {
                if let Some(text) = self.message_line(now_ms) {
                    screen.part = String::new();
                    screen.instrument = String::new();
                    screen.name = text;
                }
            }
        }
        finished(screen)
    }

    /// The boot line while it runs -- the greeting, then the
    /// soundfont's own name.
    fn boot_line(&mut self, engine: &Engine, now_ms: u64) -> Option<(String, String)> {
        if self.boot_done {
            return None;
        }
        let started = *self.boot_started.get_or_insert(now_ms);
        let mut elapsed = now_ms.saturating_sub(started);
        if elapsed < GREETING_MS {
            return Some(("COPPERSYNTH".to_string(), String::new()));
        }
        elapsed -= GREETING_MS;
        if elapsed < BANK_MS {
            let bank: String = engine.bank_name().chars().take(NAME_COLS).collect();
            if !bank.is_empty() {
                return Some((bank, String::new()));
            }
        }
        self.boot_done = true;
        None
    }

    // --- buttons ---------------------------------------------------------

    fn press_mute(&mut self, engine: &mut Engine) {
        if self.all {
            // Mute everything, or let everything go.
            let any_open = (0..PARTS).any(|p| !engine.part_muted(p));
            for p in 0..PARTS {
                engine.set_part_mute(p, any_open);
            }
        } else {
            let muted = engine.part_muted(self.part);
            engine.set_part_mute(self.part, !muted);
        }
    }

    fn press_monitor(&mut self, engine: &mut Engine) {
        let want = if self.all {
            Monitor::All
        } else {
            Monitor::Solo(self.part)
        };
        if engine.monitor() == want {
            engine.set_monitor(Monitor::Off);
        } else {
            engine.set_monitor(want);
        }
    }

    fn toggle_view(&mut self, pair: Pair) {
        self.mode = if self.mode == Mode::View(pair) {
            Mode::Home
        } else {
            Mode::View(pair)
        };
    }

    fn press_arrow(&mut self, engine: &mut Engine, pair: Pair, dir: Dir) {
        let step = |v: i32| match dir {
            Dir::Left => v - 1,
            Dir::Right => v + 1,
        };
        if self.all {
            // ALL turns the setting for every part at once, stepping
            // from wherever the shown part stands.
            let base = self.part;
            match pair {
                Pair::Level => {
                    let v = (engine.part_setting(base, PartSetting::Level) + step(0)).clamp(0, 127);
                    for p in 0..PARTS {
                        engine.set_part_level(p, v as u8);
                    }
                }
                Pair::Pan => {
                    let v = (engine.part_setting(base, PartSetting::Pan).max(1) + step(0))
                        .clamp(1, 127);
                    for p in 0..PARTS {
                        engine.set_part_pan(p, v as u8);
                    }
                }
                Pair::Reverb => {
                    let v =
                        (engine.part_setting(base, PartSetting::Reverb) + step(0)).clamp(0, 127);
                    for p in 0..PARTS {
                        engine.set_part_reverb(p, v as u8);
                    }
                }
                Pair::Chorus => {
                    let v =
                        (engine.part_setting(base, PartSetting::Chorus) + step(0)).clamp(0, 127);
                    for p in 0..PARTS {
                        engine.set_part_chorus(p, v as u8);
                    }
                }
                Pair::KeyShift => {
                    let v =
                        (engine.part_setting(base, PartSetting::KeyShift) + step(0)).clamp(-24, 24);
                    for p in 0..PARTS {
                        engine.set_part_key_shift(p, v as i8);
                    }
                }
                // With ALL lit the MIDI CH arrows turn the Device ID,
                // 1-32, shown in the MIDI CH cell -- the unit's own
                // arrangement.
                Pair::MidiCh => {
                    engine.set_device_id((engine.device_id() as i32 + step(0)).clamp(1, 32) as u8);
                }
                Pair::Part | Pair::Instrument => {}
            }
            return;
        }
        let part = self.part;
        let view = engine.part_view(part);
        match pair {
            Pair::Part => {
                self.part = (part as i32 + step(0)).rem_euclid(PARTS as i32) as usize;
                // The solo travels with the selection, as the unit's
                // monitor does.
                if let Monitor::Solo(_) = engine.monitor() {
                    engine.set_monitor(Monitor::Solo(self.part));
                }
            }
            Pair::Instrument => {
                let next = if view.drums {
                    // Kits are a list, and the list is the font's.
                    engine.neighbour_kit(part, step(0)).unwrap_or(0)
                } else {
                    // Melodic numbers never shift: a hole in a sparse
                    // font stays a numbered slot, shown as Empty.
                    (view.instrument as i32 + step(0)).rem_euclid(128) as u8
                };
                engine.set_part_instrument(part, next);
            }
            Pair::Level => {
                engine.set_part_level(part, (view.level as i32 + step(0)).clamp(0, 127) as u8);
            }
            Pair::Pan => {
                // 0 is the wire's "random"; the panel steps the placed
                // range, L63 to R63.
                let v = (view.pan.max(1) as i32 + step(0)).clamp(1, 127);
                engine.set_part_pan(part, v as u8);
            }
            Pair::Reverb => {
                engine.set_part_reverb(part, (view.reverb as i32 + step(0)).clamp(0, 127) as u8);
            }
            Pair::Chorus => {
                engine.set_part_chorus(part, (view.chorus as i32 + step(0)).clamp(0, 127) as u8);
            }
            Pair::KeyShift => {
                engine.set_part_key_shift(
                    part,
                    (view.key_shift as i32 + step(0)).clamp(-24, 24) as i8,
                );
            }
            Pair::MidiCh => {
                // 1..16 then Off, round it goes.
                let at = match view.rx_channel {
                    Some(c) => c as i32,
                    None => PARTS as i32,
                };
                let at = (at + step(0)).rem_euclid(PARTS as i32 + 1);
                engine.set_part_rx_channel(part, (at < PARTS as i32).then_some(at as u8));
            }
        }
    }

    fn notice(&mut self, text: &str) {
        self.message = Some(Message::Notice {
            text: text.to_string(),
            started: None,
        });
    }

    // --- the glass -------------------------------------------------------

    fn home_screen(&self, engine: &Engine, bars: [u16; PARTS]) -> Screen {
        let monitoring = engine.monitor() != Monitor::Off;
        if self.all {
            // Each value reads across all sixteen parts: the value when
            // they agree, and a shrug when they do not.
            let uniform = |setting: PartSetting| -> Option<i32> {
                let first = engine.part_setting(0, setting);
                (1..PARTS)
                    .all(|p| engine.part_setting(p, setting) == first)
                    .then_some(first)
            };
            let show = |setting: PartSetting, label: fn(i32) -> String| {
                uniform(setting).map(label).unwrap_or_else(|| "---".into())
            };
            return Screen {
                part: "ALL".to_string(),
                instrument: String::new(),
                // The unit introduces the bank it is playing from.
                name: engine.bank_name().chars().take(NAME_COLS).collect(),
                subtitle: String::new(),
                level: show(PartSetting::Level, |v| v.to_string()),
                pan: show(PartSetting::Pan, |v| pan_label(v as u8)),
                reverb: show(PartSetting::Reverb, |v| v.to_string()),
                chorus: show(PartSetting::Chorus, |v| v.to_string()),
                key_shift: show(PartSetting::KeyShift, |v| shift_label(v as i8)),
                // The MIDI CH cell serves the Device ID with ALL lit,
                // as on the unit -- its arrows turn it directly.
                midi_ch: engine.device_id().to_string(),
                bars,
                all_led: true,
                mute_led: (0..PARTS).all(|p| engine.part_muted(p)),
                mute_blink: monitoring,
                translating: engine.translating(),
            };
        }
        let view = engine.part_view(self.part);
        let name: String = if view.drums {
            // A drum set wears the unit's asterisk.
            format!("*{}", view.name)
        } else {
            // A variation wears its mark: + for banks 1-126, # for the
            // MT-32 map on 127, a capital bare.
            match engine.part_bank(self.part) {
                0 => view.name.clone(),
                127 => format!("#{}", view.name),
                _ => format!("+{}", view.name),
            }
        };
        Screen {
            part: format!("{:02}", self.part + 1),
            instrument: format!("{:03}", view.instrument as u16 + 1),
            name: name.chars().take(NAME_COLS).collect(),
            subtitle: String::new(),
            level: view.level.to_string(),
            pan: pan_label(view.pan),
            reverb: view.reverb.to_string(),
            chorus: view.chorus.to_string(),
            key_shift: shift_label(view.key_shift),
            midi_ch: match view.rx_channel {
                Some(c) => format!("{:02}", c + 1),
                // A part receiving nothing shows an empty channel, the
                // way every other empty value on the glass reads.
                None => "---".to_string(),
            },
            bars,
            all_led: false,
            mute_led: view.muted,
            mute_blink: monitoring,
            translating: engine.translating(),
        }
    }

    /// The matrix: live levels with a fall and a peak dot, a parameter
    /// staircase while a pair view is up, or the picture a game sent.
    fn compose_bars(&mut self, engine: &Engine, now_ms: u64) -> [u16; PARTS] {
        // The power-on show owns the matrix from the moment the unit
        // wakes; nothing on the wire interrupts it.
        if let Some(started) = self.boot_started {
            let elapsed = now_ms.saturating_sub(started);
            if elapsed < BOOT_SHOW_MS {
                return boot_show_bars(elapsed);
            }
        }
        // A picture owns the matrix while it is fresh.
        if let Some((columns, started)) = &mut self.picture {
            let held = columns.to_owned();
            let since = *started.get_or_insert(now_ms);
            if now_ms.saturating_sub(since) < PICTURE_MS {
                return held;
            }
            self.picture = None;
        }
        if let Mode::View(pair) = self.mode {
            return self.value_bars(engine, pair);
        }
        let elapsed = self
            .last_tick
            .map(|t| now_ms.saturating_sub(t))
            .unwrap_or(0);
        self.last_tick = Some(now_ms);
        let live = engine.part_activity();
        let display = engine.display_type();
        let hold = engine.peak_hold();
        let mut bars = [0u16; PARTS];
        for p in 0..PARTS {
            // Perceptual lift, then a fall no faster than the eye.
            let target = (live[p].sqrt() * 1.4).min(1.0);
            let fallen = self.shown[p] - BAR_FALL_PER_MS * elapsed as f32;
            self.shown[p] = target.max(fallen).max(0.0);
            let rows = (self.shown[p] * (BAR_ROWS - 1) as f32).round() as u32;
            let peak = &mut self.peaks[p];
            if rows >= peak.row {
                *peak = PeakDot {
                    row: rows,
                    held_at: now_ms,
                };
            } else if now_ms.saturating_sub(peak.held_at) > PEAK_HOLD_MS {
                // Held its moment; what happens next is the peak-hold
                // style: fall a row at a time, wink out, or float away.
                match hold {
                    2 => peak.row = rows,
                    3 => {
                        peak.row = if peak.row >= BAR_ROWS - 1 {
                            rows
                        } else {
                            peak.row + 1
                        };
                    }
                    _ => peak.row = peak.row.saturating_sub(1).max(rows),
                }
                peak.held_at = now_ms.saturating_sub(PEAK_HOLD_MS.saturating_sub(PEAK_FALL_MS));
            }
            // The baseline dot is the part being there at all; muting
            // switches it off, which is how the unit marks a mute.
            let peak_dot = (hold != 0 && peak.row > 0).then_some(peak.row);
            bars[p] = style_column(display, rows, !engine.part_muted(p), peak_dot);
        }
        bars
    }

    /// A pair held together shows its values across the parts -- the
    /// staircase, worn in the chosen display style.
    fn value_bars(&self, engine: &Engine, pair: Pair) -> [u16; PARTS] {
        let display = engine.display_type();
        let mut bars = [0u16; PARTS];
        for (p, bar) in bars.iter_mut().enumerate() {
            let height = match pair {
                Pair::Level => scaled(engine.part_setting(p, PartSetting::Level) as u32, 127),
                Pair::Pan => scaled(
                    (engine.part_setting(p, PartSetting::Pan).max(1) - 1) as u32,
                    126,
                ),
                Pair::Reverb => scaled(engine.part_setting(p, PartSetting::Reverb) as u32, 127),
                Pair::Chorus => scaled(engine.part_setting(p, PartSetting::Chorus) as u32, 127),
                Pair::KeyShift => scaled(
                    (engine.part_setting(p, PartSetting::KeyShift) + 24) as u32,
                    48,
                ),
                Pair::Instrument => scaled(engine.part_view(p).instrument as u32, 127),
                Pair::MidiCh => match engine.part_view(p).rx_channel {
                    Some(c) => c as u32 + 1,
                    None => 0,
                },
                Pair::Part => 0,
            };
            if height > 0 {
                *bar = style_column(display, height - 1, true, None);
            }
        }
        bars
    }

    /// The visible slice of whatever message is up, if one is.
    fn message_line(&mut self, now_ms: u64) -> Option<String> {
        match self.message.as_mut()? {
            Message::Notice { text, started } => {
                let since = *started.get_or_insert(now_ms);
                if now_ms.saturating_sub(since) >= NOTICE_MS {
                    self.message = None;
                    return None;
                }
                Some(text.clone())
            }
            Message::Letter { text, started } => {
                let since = *started.get_or_insert(now_ms);
                Some(scroll_line(text, since, now_ms))
            }
        }
    }
}

/// The visible window of a line, scrolling when it is long. Round and
/// round: rest at the head, step through, rest on the tail, and begin
/// again.
fn scroll_line(text: &[char], since: u64, now_ms: u64) -> String {
    let len = text.len();
    if len <= NAME_COLS {
        return text.iter().collect();
    }
    let steps = (len - NAME_COLS) as u64;
    let cycle = SCROLL_START_MS + steps * SCROLL_STEP_MS + SCROLL_TAIL_MS;
    let at = now_ms.saturating_sub(since) % cycle;
    let offset = if at < SCROLL_START_MS {
        0
    } else {
        ((at - SCROLL_START_MS) / SCROLL_STEP_MS).min(steps)
    };
    text[offset as usize..offset as usize + NAME_COLS]
        .iter()
        .collect()
}

/// The version, and the day its commit was made, stamped at build
/// time.
fn version_line() -> String {
    let date = env!("COPPERSYNTH_RELEASE_DATE");
    if date.is_empty() {
        format!("v{}", env!("CARGO_PKG_VERSION"))
    } else {
        format!("v{} {date}", env!("CARGO_PKG_VERSION"))
    }
}

/// A dash in every segment: what a value field shows when there is no
/// value to show.
fn dash_values(screen: &mut Screen) {
    for field in [
        &mut screen.level,
        &mut screen.pan,
        &mut screen.reverb,
        &mut screen.chorus,
        &mut screen.key_shift,
        &mut screen.midi_ch,
    ] {
        *field = "---".to_string();
    }
}

/// The last word on any screen: a blank part or instrument slot on a
/// powered unit reads as dashes too, like every other empty segment.
/// (Demo mode's `S-1` and the home screen's numbers pass untouched.)
fn finished(mut screen: Screen) -> Screen {
    if screen.part.is_empty() {
        screen.part = "---".to_string();
    }
    if screen.instrument.is_empty() {
        screen.instrument = "---".to_string();
    }
    screen
}

/// Bar height 1..=16 for a value against its full scale.
fn scaled(value: u32, full: u32) -> u32 {
    1 + value * (BAR_ROWS - 1) / full
}

/// One column of the matrix in one of the unit's eight display types:
/// 1 bars, 2 a single segment, 3 and 4 the same hung from the top,
/// 5-8 the first four in negative. The baseline dot marks the part
/// being there at all, and the peak dot rides where the style puts it.
fn style_column(display: u8, rows: u32, baseline: bool, peak: Option<u32>) -> u16 {
    let top = BAR_ROWS - 1;
    let flip = |row: u32| top - row.min(top);
    let mut column: u16 = 0;
    match (display.clamp(1, 8) - 1) % 4 {
        // Bars up from the floor.
        0 => {
            if rows > 0 {
                column |= (1u32 << (rows + 1)).wrapping_sub(1) as u16;
            }
            if baseline {
                column |= 1;
            }
            if let Some(p) = peak {
                column |= 1 << p.min(top);
            }
        }
        // A single segment at the level -- one block, rising and
        // falling alone; a separate peak dot would read as a second
        // note.
        1 => {
            if rows > 0 || baseline {
                column |= 1 << rows.min(top);
            }
        }
        // Bars hung from the ceiling.
        2 => {
            if rows > 0 {
                for r in 0..=rows.min(top) {
                    column |= 1 << flip(r);
                }
            }
            if baseline {
                column |= 1 << top;
            }
            if let Some(p) = peak {
                column |= 1 << flip(p);
            }
        }
        // A single segment hung from the ceiling, likewise alone.
        _ => {
            if rows > 0 || baseline {
                column |= 1 << flip(rows);
            }
        }
    }
    if display >= 5 {
        column = !column;
    }
    column
}

/// Pan as the unit prints it: L63 to R63 around 0, and the wire's 0 as
/// the random placement it means on this hardware.
fn pan_label(pan: u8) -> String {
    match pan as i32 - 64 {
        0 => "0".to_string(),
        p if pan == 0 => {
            let _ = p;
            "Rnd".to_string()
        }
        p if p < 0 => format!("L{}", -p),
        p => format!("R{p}"),
    }
}

/// Key shift with its sign, 0 bare.
fn shift_label(shift: i8) -> String {
    match shift {
        0 => "0".to_string(),
        s if s < 0 => s.to_string(),
        s => format!("+{s}"),
    }
}
