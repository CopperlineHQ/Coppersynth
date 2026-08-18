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
    /// An arrow pressed with MUTE latched down: the service edits
    /// (MIDI CH pair: device ID; CHORUS pair: chorus type).
    MuteArrow(Pair, Dir),
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
    /// "Init SoundFont,Sure?" -- ALL puts the built-in bank back,
    /// MUTE carries on with the one loaded.
    ConfirmFont,
    /// The second after a factory reset was confirmed: the host is
    /// putting the built-in bank back, and every button waits.
    Initializing,
    /// The undocumented screen: the credits roll until ALL or MUTE
    /// lets the boot carry on.
    Credits,
    /// "Device ID: <n>" -- the MIDI CH arrows cycle 1-32, ALL commits,
    /// MUTE cancels. Reached with MUTE latched and a MIDI CH arrow.
    EditDeviceId {
        pending: u8,
    },
    /// "Chorus Type: <n>" -- the CHORUS arrows cycle 0-8 and each
    /// selection sounds at once for auditioning; ALL keeps it, MUTE
    /// puts the original back. Reached with MUTE latched and a CHORUS
    /// arrow.
    EditChorusType {
        pending: u8,
        original: u8,
    },
    /// The part-parameter editor: INSTRUMENT arrows browse the
    /// settings, LEVEL arrows set 0-127 (sounding at once), PART
    /// arrows move between parts. ALL keeps everything, MUTE puts the
    /// whole snapshot back.
    EditPartParams {
        param: usize,
        all: bool,
    },
    /// The unit playing to itself: ALL plays, MUTE stops, PART picks
    /// the song. Reached with both PART halves held through power-on.
    Demo {
        song: usize,
        playing: bool,
    },
}

/// The per-part parameters the fascia has no pair for -- the CC and
/// GS-NRPN settings a game would drive over the wire -- browsable in
/// the part-parameter editor (MUTE latched under an INSTRUMENT
/// arrow). Every value is the wire's own 0-127; the relative tone
/// modifies sit at 64 when neutral.
const PART_PARAMS: [(&str, PartParam); 12] = [
    ("Portamento Time", PartParam::Cc(0x05)),
    ("Portamento", PartParam::Cc(0x41)),
    ("Sostenuto", PartParam::Cc(0x42)),
    ("Soft Pedal", PartParam::Cc(0x43)),
    ("Vibrato Rate", PartParam::Nrpn(0x01, 0x08)),
    ("Vibrato Depth", PartParam::Nrpn(0x01, 0x09)),
    ("Vibrato Delay", PartParam::Nrpn(0x01, 0x0A)),
    ("Cutoff", PartParam::Nrpn(0x01, 0x20)),
    ("Resonance", PartParam::Nrpn(0x01, 0x21)),
    ("EG Attack", PartParam::Nrpn(0x01, 0x63)),
    ("EG Decay", PartParam::Nrpn(0x01, 0x64)),
    ("EG Release", PartParam::Nrpn(0x01, 0x66)),
];

/// How a part parameter reaches the engine: a plain controller, or a
/// GS NRPN by its select pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartParam {
    Cc(u8),
    Nrpn(u8, u8),
}

impl PartParam {
    fn read(self, engine: &Engine, part: usize) -> u8 {
        match self {
            Self::Cc(cc) => engine.part_cc_value(part, cc),
            Self::Nrpn(msb, lsb) => engine.part_nrpn_wire(part, msb, lsb),
        }
    }

    fn write(self, engine: &mut Engine, part: usize, value: u8) {
        match self {
            Self::Cc(cc) => engine.send_part_cc(part, cc, value),
            Self::Nrpn(msb, lsb) => engine.send_part_nrpn(part, msb, lsb, value),
        }
    }
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
    /// Every part's parameters as they stood when the part-parameter
    /// editor opened; MUTE restores the lot.
    part_param_snapshot: Option<Box<[[u8; PART_PARAMS.len()]; PARTS]>>,
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
            part_param_snapshot: None,
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
    pub fn power_on_held(&mut self, held: &[Button]) {
        let is =
            |want: &[Button]| held.len() == want.len() && want.iter().all(|b| held.contains(b));
        if is(&[Button::Both(Pair::Part)]) {
            // Both PART halves: demo mode, armed on song one and
            // waiting for ALL, MUTE lamp lit, exactly as the unit
            // arrives in it.
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
            // The other half of the pair asks the factory question.
            self.mode = Mode::ConfirmFont;
            self.boot_done = true;
        } else if is(&[Button::Both(Pair::MidiCh), Button::Both(Pair::Instrument)]) {
            // Undocumented, as the tradition demands: the credits roll
            // until ALL or MUTE lets the boot go on.
            self.mode = Mode::Credits;
            self.boot_done = true;
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

    /// Whether an edit or confirm screen owns the glass -- the host
    /// holds its latching gestures back while one does.
    pub fn in_edit(&self) -> bool {
        matches!(
            self.mode,
            Mode::ConfirmMt32
                | Mode::ConfirmFont
                | Mode::Initializing
                | Mode::EditDeviceId { .. }
                | Mode::EditChorusType { .. }
                | Mode::EditPartParams { .. }
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
        // The factory prompt: ALL initialises (the host swaps the bank
        // while the screen holds), MUTE carries on with the one loaded.
        if self.mode == Mode::ConfirmFont {
            return match b {
                Button::All => {
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
        // The service edits: the opening pair's arrows cycle the value,
        // ALL commits, MUTE cancels, everything else waits.
        if let Mode::EditDeviceId { pending } = self.mode {
            match b {
                Button::Arrow(Pair::MidiCh, dir) => {
                    let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                    let next = (pending as i32 - 1 + step).rem_euclid(32) + 1;
                    self.mode = Mode::EditDeviceId {
                        pending: next as u8,
                    };
                }
                Button::All => {
                    engine.set_device_id(pending);
                    self.mode = Mode::Home;
                    self.notice(&format!("Device ID {pending}"));
                }
                Button::Mute => self.mode = Mode::Home,
                _ => {}
            }
            return None;
        }
        if let Mode::EditChorusType { pending, original } = self.mode {
            match b {
                Button::Arrow(Pair::Chorus, dir) => {
                    let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                    let next = (pending as i32 + step).rem_euclid(9) as u8;
                    // The selection sounds at once, so the ear can
                    // choose; ALL keeps it, MUTE puts the original back.
                    engine.set_chorus_type(crate::synth::ChorusType::from_index(next));
                    self.mode = Mode::EditChorusType {
                        pending: next,
                        original,
                    };
                }
                Button::All => {
                    engine.set_chorus_type(crate::synth::ChorusType::from_index(pending));
                    self.mode = Mode::Home;
                    self.notice("Chorus params saved");
                }
                Button::Mute => {
                    engine.set_chorus_type(crate::synth::ChorusType::from_index(original));
                    self.mode = Mode::Home;
                }
                _ => {}
            }
            return None;
        }
        if let Mode::EditPartParams { param, all } = self.mode {
            let (_, kind) = PART_PARAMS[param];
            match b {
                Button::Arrow(Pair::Instrument, dir) => {
                    let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                    let next = (param as i32 + step).rem_euclid(PART_PARAMS.len() as i32);
                    self.mode = Mode::EditPartParams {
                        param: next as usize,
                        all,
                    };
                }
                Button::Arrow(Pair::Level, dir) => {
                    let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                    // The wire's own range: 0-127 for the controllers,
                    // 14-114 (the chart's 0EH-72H) for the relative
                    // tone modifies.
                    let (lo, hi) = match kind {
                        PartParam::Cc(_) => (0, 127),
                        PartParam::Nrpn(..) => (14, 114),
                    };
                    let value = (kind.read(engine, self.part) as i32 + step).clamp(lo, hi);
                    // Sounding at once, so the ear can judge it -- on
                    // every part when ALL stands.
                    if all {
                        for part in 0..PARTS {
                            kind.write(engine, part, value as u8);
                        }
                    } else {
                        kind.write(engine, self.part, value as u8);
                    }
                }
                Button::Arrow(Pair::Part, dir) => {
                    // A PART press snaps out of ALL first; after that it
                    // walks the parts as ever.
                    if all {
                        self.all = false;
                        self.mode = Mode::EditPartParams { param, all: false };
                    } else {
                        let step: i32 = if dir == Dir::Left { -1 } else { 1 };
                        self.part = (self.part as i32 + step).rem_euclid(PARTS as i32) as usize;
                    }
                }
                Button::All => {
                    self.part_param_snapshot = None;
                    self.mode = Mode::Home;
                    self.notice("Part params saved");
                }
                Button::Mute => {
                    if let Some(snapshot) = self.part_param_snapshot.take() {
                        for (part, values) in snapshot.iter().enumerate() {
                            for (i, &value) in values.iter().enumerate() {
                                let (_, kind) = PART_PARAMS[i];
                                if kind.read(engine, part) != value {
                                    kind.write(engine, part, value);
                                }
                            }
                        }
                    }
                    self.mode = Mode::Home;
                }
                _ => {}
            }
            return None;
        }
        // MUTE latched under an arrow opens the service edits, seeded
        // on what is in force; elsewhere the gesture means nothing.
        if let Button::MuteArrow(pair, _) = b {
            match pair {
                Pair::MidiCh => {
                    self.mode = Mode::EditDeviceId {
                        pending: engine.device_id(),
                    };
                }
                Pair::Chorus => {
                    let original = engine.chorus_type().index();
                    self.mode = Mode::EditChorusType {
                        pending: original,
                        original,
                    };
                }
                Pair::Instrument => {
                    // Everything as it stands, for MUTE to put back.
                    let mut snapshot = Box::new([[0u8; PART_PARAMS.len()]; PARTS]);
                    for (part, values) in snapshot.iter_mut().enumerate() {
                        for (i, value) in values.iter_mut().enumerate() {
                            *value = PART_PARAMS[i].1.read(engine, part);
                        }
                    }
                    self.part_param_snapshot = Some(snapshot);
                    // Entered with ALL lit, the edits land on all
                    // sixteen parts at once.
                    self.mode = Mode::EditPartParams {
                        param: 0,
                        all: self.all,
                    };
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
        match b {
            Button::All => self.all = !self.all,
            Button::Mute => self.press_mute(engine),
            Button::Monitor => self.press_monitor(engine),
            Button::Both(pair) => self.toggle_view(pair),
            Button::Arrow(pair, dir) => self.press_arrow(engine, pair, dir),
            // Handled (or dismissed) before this match; nothing to do.
            Button::MuteArrow(..) => {}
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
            Mode::ConfirmMt32 | Mode::ConfirmFont => {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = if self.mode == Mode::ConfirmMt32 {
                    "Init MT-32, Sure?".to_string()
                } else {
                    "Init SoundFont,Sure?".to_string()
                };
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
            Mode::EditDeviceId { pending } => {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = format!("Device ID: {pending}");
                dash_values(&mut screen);
                let flash_on = (now_ms / FLASH_MS).is_multiple_of(2);
                screen.all_led = flash_on;
                screen.mute_led = flash_on;
            }
            Mode::EditPartParams { param, all } => {
                let (name, kind) = PART_PARAMS[param];
                screen.part = if all {
                    "ALL".to_string()
                } else {
                    format!("{:02}", self.part + 1)
                };
                screen.instrument = String::new();
                screen.name = format!("{name}: {}", kind.read(engine, self.part));
                dash_values(&mut screen);
                let flash_on = (now_ms / FLASH_MS).is_multiple_of(2);
                screen.all_led = flash_on;
                screen.mute_led = flash_on;
            }
            Mode::EditChorusType { pending, .. } => {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = format!("Chorus Type: {pending}");
                // The type's name rides the second line, so the number
                // means something.
                screen.subtitle = crate::synth::ChorusType::from_index(pending)
                    .label()
                    .to_string();
                dash_values(&mut screen);
                let flash_on = (now_ms / FLASH_MS).is_multiple_of(2);
                screen.all_led = flash_on;
                screen.mute_led = flash_on;
            }
            Mode::Credits => {
                screen.part = String::new();
                screen.instrument = String::new();
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
        // PART together is the unit's menu system, which is not here;
        // the value pairs toggle their bar view.
        if pair == Pair::Part {
            return;
        }
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
                // A receive channel is each part's own; there is no
                // "every part at once" to step, so the pair sits inert
                // with its value dashed.
                Pair::Part | Pair::Instrument | Pair::MidiCh => {}
            }
            return;
        }
        let part = self.part;
        let view = engine.part_view(part);
        match pair {
            Pair::Part => {
                self.part = (part as i32 + step(0)).rem_euclid(PARTS as i32) as usize;
                // Monitoring follows the selection, as the unit's does.
                if let Monitor::Solo(_) = engine.monitor() {
                    engine.set_monitor(Monitor::Solo(self.part));
                }
            }
            Pair::Instrument => {
                let next = if view.drums {
                    // Kits are a list, and the list is the font's.
                    engine.neighbour_kit(step(0)).unwrap_or(0)
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
                // No single channel speaks for sixteen parts.
                midi_ch: "---".to_string(),
                bars,
                all_led: true,
                mute_led: (0..PARTS).all(|p| engine.part_muted(p)),
                translating: engine.translating(),
            };
        }
        let view = engine.part_view(self.part);
        let name: String = if view.drums {
            // A drum set wears the unit's asterisk.
            format!("*{}", view.name)
        } else {
            view.name.clone()
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
            mute_led: view.muted || monitoring,
            translating: engine.translating(),
        }
    }

    /// The matrix: live levels with a fall and a peak dot, a parameter
    /// staircase while a pair view is up, or the picture a game sent.
    fn compose_bars(&mut self, engine: &Engine, now_ms: u64) -> [u16; PARTS] {
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
                // Held its moment; now it falls a row at a time until
                // it lands.
                peak.row = peak.row.saturating_sub(1).max(rows);
                peak.held_at = now_ms.saturating_sub(PEAK_HOLD_MS.saturating_sub(PEAK_FALL_MS));
            }
            // The baseline dot is the part being there at all; muting
            // switches it off, which is how the unit marks a mute.
            if !engine.part_muted(p) {
                bars[p] |= 1;
            }
            if rows > 0 {
                bars[p] |= (1u32 << (rows + 1)).wrapping_sub(1) as u16;
            }
            if peak.row > 0 {
                bars[p] |= 1 << peak.row.min(BAR_ROWS - 1);
            }
        }
        bars
    }

    /// A pair held together shows its values across the parts -- the
    /// staircase.
    fn value_bars(&self, engine: &Engine, pair: Pair) -> [u16; PARTS] {
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
                *bar = (1u32 << height).wrapping_sub(1) as u16;
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
