//! The front panel: the SC-55-shaped face of the engine.
//!
//! Everything the LCD shows is composed here, in the library -- the host
//! draws glass and buttons and forwards presses, and never invents a
//! character of its own. The layout and behaviour follow the Sound
//! Canvas SC-55 owner's manual: ALL and MUTE with their lamps, eight
//! left/right pairs, the sixteen-column bar matrix with a dot of peak
//! hold, letters and dot pictures a game writes over sysex, and the
//! power-on button combinations -- including the unit's own way of
//! being told it is an MT-32 today.
//!
//! What the real unit keeps in menus (master tune, LCD contrast, bar
//! display types, bulk dumps, Micro Edit) is not modelled; the panel is
//! for playing games at, not servicing.

use crate::engine::{GmEngine, Monitor, PARTS};

/// One side of a left/right pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
}

/// The eight left/right pairs, top to bottom as the fascia stacks them.
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
    /// A Displayed Letter: up to 32 characters, scrolled when long.
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
    /// The INSTRUMENT number field: `001`..`128`, or empty.
    pub instrument: String,
    /// The name area: instrument name, message text, or a prompt.
    pub name: String,
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

/// Something the panel cannot do to the engine it is given and asks the
/// host for instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelRequest {
    /// Init All: power the unit off and on again, back to the host's
    /// configuration.
    Recycle,
}

/// The three initialisations the unit offers at power-on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Init {
    Gs,
    Mt32,
    All,
}

/// What the panel is showing over the home screen, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Home,
    /// A pair held together: its values across the parts, as bars.
    View(Pair),
    /// An init prompt: ALL executes, MUTE cancels.
    Confirm(Init),
    /// The undocumented screen. Any press leaves it.
    Version,
}

/// A Displayed Letter in flight.
#[derive(Debug, Clone)]
struct Message {
    text: Vec<char>,
    /// When it went up; set the first time it is drawn.
    started: Option<u64>,
}

/// How long the greeting holds the glass at power-on.
const GREETING_MS: u64 = 1500;
/// How long a letter that fits stays up.
const LETTER_HOLD_MS: u64 = 3000;
/// A long letter rests, then steps one column at a time.
const SCROLL_START_MS: u64 = 600;
const SCROLL_STEP_MS: u64 = 300;
/// And holds its tail before the display falls back.
const SCROLL_TAIL_MS: u64 = 2000;
/// How long a dot picture holds the matrix past its last frame.
const PICTURE_MS: u64 = 3000;
/// The text area's width in characters; longer letters scroll by.
pub const NAME_COLS: usize = 16;
/// The bar matrix: sixteen columns of sixteen dots.
pub const BAR_ROWS: u32 = 16;
/// How fast a bar falls once the sound under it has, full scale per
/// second, and how long the peak dot holds before it falls too.
const BAR_FALL_PER_MS: f32 = 0.004;
const PEAK_HOLD_MS: u64 = 600;
/// The ten drum sets, by program number, as the manual lists them.
const DRUM_SETS: [u8; 10] = [0, 8, 16, 24, 25, 32, 40, 48, 56, 127];

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
    /// The greeting: pending until first drawn, then timed out.
    greeting_started: Option<u64>,
    greeting_done: bool,
    message: Option<Message>,
    picture: Option<([u16; PARTS], Option<u64>)>,
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
            greeting_started: None,
            greeting_done: false,
            message: None,
            picture: None,
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
        self.mode = if is(&[Button::Both(Pair::Part)]) {
            // The unit inits and then offers ROM play; there are no
            // songs in this one, so the init is what remains.
            Mode::Confirm(Init::Gs)
        } else if is(&[Button::Arrow(Pair::Instrument, Dir::Right)]) {
            Mode::Confirm(Init::Gs)
        } else if is(&[Button::Arrow(Pair::Instrument, Dir::Left)]) {
            Mode::Confirm(Init::Mt32)
        } else if is(&[Button::Both(Pair::Instrument)]) {
            Mode::Confirm(Init::All)
        } else if is(&[Button::All, Button::Mute]) {
            // Undocumented, as the tradition demands.
            Mode::Version
        } else {
            Mode::Home
        };
    }

    /// A press. Anything the panel cannot reach on the engine comes
    /// back as a request.
    pub fn button(&mut self, engine: &mut GmEngine, b: Button) -> Option<PanelRequest> {
        // The version screen leaves on any press, saying nothing.
        if self.mode == Mode::Version {
            self.mode = Mode::Home;
            return None;
        }
        // A prompt takes ALL as yes and MUTE as no, and ignores the
        // rest, as the unit's confirmations do.
        if let Mode::Confirm(init) = self.mode {
            return match b {
                Button::All => {
                    self.mode = Mode::Home;
                    self.run_init(engine, init)
                }
                Button::Mute => {
                    self.mode = Mode::Home;
                    None
                }
                _ => None,
            };
        }
        match b {
            Button::All => self.all = !self.all,
            Button::Mute => self.press_mute(engine),
            Button::Monitor => self.press_monitor(engine),
            Button::Both(pair) => self.toggle_view(pair),
            Button::Arrow(pair, dir) => self.press_arrow(engine, pair, dir),
        }
        None
    }

    /// The VOLUME knob, 0 at the bottom of its travel to 1 at the top.
    /// A pot after the DAC, exactly as on the unit.
    pub fn volume(&mut self, engine: &mut GmEngine, value: f32) {
        engine.set_output_gain(value);
    }

    /// Text and pictures the engine took off the wire.
    pub fn feed(&mut self, feed: Feed) {
        match feed {
            Feed::Text(text) => {
                self.message = Some(Message {
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
    pub fn screen(&mut self, engine: &GmEngine, now_ms: u64) -> Screen {
        let bars = self.compose_bars(engine, now_ms);
        let mut screen = self.home_screen(engine, bars);
        // The greeting holds the whole line first.
        if !self.greeting_done {
            let started = *self.greeting_started.get_or_insert(now_ms);
            if now_ms.saturating_sub(started) < GREETING_MS {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = "COPPERSYNTH".to_string();
                return screen;
            }
            self.greeting_done = true;
        }
        match self.mode {
            Mode::Version => {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = format!("ver{} hobbo91", env!("CARGO_PKG_VERSION"));
            }
            Mode::Confirm(init) => {
                screen.part = String::new();
                screen.instrument = String::new();
                // The manual's own prompts, spacing included.
                screen.name = match init {
                    Init::Gs => "Init GS, Sure?",
                    Init::Mt32 => "Init MT-32,Sure?",
                    Init::All => "Init All, Sure?",
                }
                .to_string();
            }
            Mode::Home | Mode::View(_) => {
                if let Some(text) = self.letter_window(now_ms) {
                    screen.part = String::new();
                    screen.instrument = String::new();
                    screen.name = text;
                }
            }
        }
        screen
    }

    // --- buttons ---------------------------------------------------------

    fn press_mute(&mut self, engine: &mut GmEngine) {
        if self.all {
            // Mute everything, or let everything go.
            let any_open = (0..PARTS).any(|p| !engine.part_view(p).muted);
            for p in 0..PARTS {
                engine.set_part_mute(p, any_open);
            }
        } else {
            let muted = engine.part_view(self.part).muted;
            engine.set_part_mute(self.part, !muted);
        }
    }

    fn press_monitor(&mut self, engine: &mut GmEngine) {
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

    fn press_arrow(&mut self, engine: &mut GmEngine, pair: Pair, dir: Dir) {
        let step = |v: i32| match dir {
            Dir::Left => v - 1,
            Dir::Right => v + 1,
        };
        if self.all {
            match pair {
                // The manual's ALL assignments: LEVEL is the master
                // volume, PAN and KEY SHIFT their master values, the
                // effect pairs the return levels, and MIDI CH -- the
                // device ID.
                Pair::Level => {
                    let v = (engine.master_volume_cc() as i32 + step(0)).clamp(0, 127);
                    engine.set_master_volume_cc(v as u8);
                }
                Pair::Pan => {
                    let v = (engine.master_pan() as i32 + step(0)).clamp(1, 127);
                    engine.set_master_pan(v as u8);
                }
                Pair::Reverb => {
                    let v = (engine.master_reverb() as i32 + step(0)).clamp(0, 127);
                    engine.set_master_reverb(v as u8);
                }
                Pair::Chorus => {
                    let v = (engine.master_chorus() as i32 + step(0)).clamp(0, 127);
                    engine.set_master_chorus(v as u8);
                }
                Pair::KeyShift => {
                    let v = (engine.master_key_shift() as i32 + step(0)).clamp(-24, 24);
                    engine.set_master_key_shift(v as i8);
                }
                Pair::MidiCh => {
                    let v = (engine.device_id() as i32 + step(0)).clamp(1, 32);
                    engine.set_device_id(v as u8);
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
                // Monitoring follows the selection, as the unit's does.
                if let Monitor::Solo(_) = engine.monitor() {
                    engine.set_monitor(Monitor::Solo(self.part));
                }
            }
            Pair::Instrument => {
                if view.drums {
                    // Drum parts step among the ten sets.
                    let at = DRUM_SETS
                        .iter()
                        .position(|&s| s == view.instrument)
                        .unwrap_or(0);
                    let at = (at as i32 + step(0)).rem_euclid(DRUM_SETS.len() as i32);
                    engine.set_part_instrument(part, DRUM_SETS[at as usize]);
                } else {
                    let v = (view.instrument as i32 + step(0)).rem_euclid(128);
                    engine.set_part_instrument(part, v as u8);
                }
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

    fn run_init(&mut self, engine: &mut GmEngine, init: Init) -> Option<PanelRequest> {
        match init {
            Init::Gs => {
                engine.init_gs();
                None
            }
            Init::Mt32 => {
                engine.init_mt32();
                None
            }
            // Factory settings live in the host's configuration, so the
            // full reset is a power cycle.
            Init::All => Some(PanelRequest::Recycle),
        }
    }

    // --- the glass -------------------------------------------------------

    fn home_screen(&self, engine: &GmEngine, bars: [u16; PARTS]) -> Screen {
        let monitoring = engine.monitor() != Monitor::Off;
        if self.all {
            return Screen {
                part: "ALL".to_string(),
                instrument: String::new(),
                name: "- Coppersynth -".to_string(),
                level: engine.master_volume_cc().to_string(),
                pan: pan_label(engine.master_pan()),
                reverb: engine.master_reverb().to_string(),
                chorus: engine.master_chorus().to_string(),
                key_shift: shift_label(engine.master_key_shift()),
                midi_ch: engine.device_id().to_string(),
                bars,
                all_led: true,
                mute_led: (0..PARTS).all(|p| engine.part_view(p).muted),
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
            level: view.level.to_string(),
            pan: pan_label(view.pan),
            reverb: view.reverb.to_string(),
            chorus: view.chorus.to_string(),
            key_shift: shift_label(view.key_shift),
            midi_ch: match view.rx_channel {
                Some(c) => format!("{:02}", c + 1),
                None => "Off".to_string(),
            },
            bars,
            all_led: false,
            // Lit while a muted part is selected; blinking is the
            // host's affair through `mute_led` over time -- the panel
            // keeps it steady and monitor state readable.
            mute_led: view.muted || monitoring,
            translating: engine.translating(),
        }
    }

    /// The matrix: live levels with a fall and a peak dot, a parameter
    /// staircase while a pair view is up, or the picture a game sent.
    fn compose_bars(&mut self, engine: &GmEngine, now_ms: u64) -> [u16; PARTS] {
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
                peak.row = peak.row.saturating_sub(1);
                peak.held_at = now_ms;
            }
            // The baseline dot is the part being there at all; muting
            // switches it off, which is how the unit marks a mute.
            if !engine.part_view(p).muted {
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
    /// manual's staircase.
    fn value_bars(&self, engine: &GmEngine, pair: Pair) -> [u16; PARTS] {
        let mut bars = [0u16; PARTS];
        for (p, bar) in bars.iter_mut().enumerate() {
            let view = engine.part_view(p);
            let height = match pair {
                Pair::Level => scaled(view.level as u32, 127),
                Pair::Pan => scaled(view.pan.max(1) as u32 - 1, 126),
                Pair::Reverb => scaled(view.reverb as u32, 127),
                Pair::Chorus => scaled(view.chorus as u32, 127),
                Pair::KeyShift => scaled((view.key_shift + 24) as u32, 48),
                Pair::Instrument => scaled(view.instrument as u32, 127),
                Pair::MidiCh => match view.rx_channel {
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

    /// The visible slice of the letter, scrolled if it is long, or
    /// `None` once it has run its course.
    fn letter_window(&mut self, now_ms: u64) -> Option<String> {
        let message = self.message.as_mut()?;
        let started = *message.started.get_or_insert(now_ms);
        let elapsed = now_ms.saturating_sub(started);
        let len = message.text.len();
        if len <= NAME_COLS {
            if elapsed >= LETTER_HOLD_MS {
                self.message = None;
                return None;
            }
            return Some(message.text.iter().collect());
        }
        let steps = (len - NAME_COLS) as u64;
        let offset = if elapsed < SCROLL_START_MS {
            0
        } else {
            ((elapsed - SCROLL_START_MS) / SCROLL_STEP_MS).min(steps)
        };
        if offset == steps && elapsed > SCROLL_START_MS + steps * SCROLL_STEP_MS + SCROLL_TAIL_MS {
            self.message = None;
            return None;
        }
        Some(
            message.text[offset as usize..offset as usize + NAME_COLS]
                .iter()
                .collect(),
        )
    }
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
