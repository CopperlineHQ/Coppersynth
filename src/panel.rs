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
}

/// What the panel is showing over the home screen, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Home,
    /// A pair held together: its values across the parts, as bars.
    View(Pair),
    /// "MT-32, Sure?" -- ALL turns it on, MUTE turns it off.
    ConfirmMt32,
    /// The undocumented screen. Any press leaves it.
    Version,
    /// The unit playing to itself: ALL plays, MUTE stops, PART picks
    /// the song. Reached with both PART halves held through power-on.
    Demo {
        song: usize,
        playing: bool,
    },
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

/// The boot line: an optional version splash, the greeting, then the
/// soundfont introducing itself by its own name.
const SPLASH_MS: u64 = 3000;
const GREETING_MS: u64 = 1500;
const BANK_MS: u64 = 1500;
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
    /// Whether the boot line opens with the version splash.
    splash: bool,
    boot_started: Option<u64>,
    boot_done: bool,
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
            splash: false,
            boot_started: None,
            boot_done: false,
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
    /// reads its own fascia at start-up. (Both INSTRUMENT halves alone
    /// are the host's: they put the default soundfont back before the
    /// unit even exists.)
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
        } else if is(&[Button::Arrow(Pair::Instrument, Dir::Right)]) {
            // A unit switched on into a service screen skips its boot
            // line; the question is the greeting.
            self.mode = Mode::ConfirmMt32;
            self.boot_done = true;
        } else if is(&[Button::All, Button::Mute]) {
            // Undocumented, as the tradition demands.
            self.mode = Mode::Version;
            self.boot_done = true;
        } else if is(&[Button::Both(Pair::MidiCh), Button::Both(Pair::Instrument)]) {
            // Undocumented too: the unit says its own name and version,
            // then boots as normal.
            self.splash = true;
        }
    }

    /// A press. Anything the panel cannot mirror alone comes back as a
    /// request for the host.
    pub fn button(&mut self, engine: &mut Engine, b: Button) -> Option<PanelRequest> {
        // The version screen leaves on any press, saying nothing.
        if self.mode == Mode::Version {
            self.mode = Mode::Home;
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
            Mode::Version => {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = format!("ver{} hobbo91", env!("CARGO_PKG_VERSION"));
            }
            Mode::ConfirmMt32 => {
                screen.part = String::new();
                screen.instrument = String::new();
                screen.name = "MT-32, Sure?".to_string();
                // ALL says yes and MUTE says no; their lamps flash to
                // say the question is theirs.
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

    /// The boot line while it runs -- the greeting (wearing the version
    /// and date under it when the splash was asked for), then the
    /// soundfont's own name.
    fn boot_line(&mut self, engine: &Engine, now_ms: u64) -> Option<(String, String)> {
        if self.boot_done {
            return None;
        }
        let started = *self.boot_started.get_or_insert(now_ms);
        let mut elapsed = now_ms.saturating_sub(started);
        if self.splash {
            if elapsed < SPLASH_MS {
                // The version, and the day its commit was made, stamped
                // at build time.
                let date = env!("COPPERSYNTH_RELEASE_DATE");
                let line2 = if date.is_empty() {
                    format!("v{}", env!("CARGO_PKG_VERSION"))
                } else {
                    format!("v{} {date}", env!("CARGO_PKG_VERSION"))
                };
                return Some(("COPPERSYNTH".to_string(), line2));
            }
            elapsed -= SPLASH_MS;
        } else {
            if elapsed < GREETING_MS {
                return Some(("COPPERSYNTH".to_string(), String::new()));
            }
            elapsed -= GREETING_MS;
        }
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
                Pair::MidiCh => {
                    // The one master left on this pair: the device ID.
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
                midi_ch: engine.device_id().to_string(),
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
                let len = text.len();
                if len <= NAME_COLS {
                    return Some(text.iter().collect());
                }
                // Round and round: rest at the head, step through,
                // rest on the tail, and begin again.
                let steps = (len - NAME_COLS) as u64;
                let cycle = SCROLL_START_MS + steps * SCROLL_STEP_MS + SCROLL_TAIL_MS;
                let at = now_ms.saturating_sub(since) % cycle;
                let offset = if at < SCROLL_START_MS {
                    0
                } else {
                    ((at - SCROLL_START_MS) / SCROLL_STEP_MS).min(steps)
                };
                Some(
                    text[offset as usize..offset as usize + NAME_COLS]
                        .iter()
                        .collect(),
                )
            }
        }
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
