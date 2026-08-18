#![allow(dead_code)]

/// The selectable chorus characters, one effect unit for all parts --
/// exactly as on the hardware, where the parts choose only how much
/// they send into it (CC93). Types 1-3 and 6-8 are the SC-55's own
/// macros, run through the measured unit-to-DSP conversions
/// (Nuked-SC55 via EmuSC): pre-delay = (1 + 6n)/32000 s, sweep span =
/// 10n/32000 s, rate ~= 0.116n + 0.11 Hz, feedback = (n >> 1)/64.
/// Types 4-5 (Celeste) are the lighter shimmer of the later GM2 list:
/// small detune, quicker sweep, no feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChorusType {
    Off,
    Chorus1,
    Chorus2,
    Chorus3,
    Celeste1,
    Celeste2,
    Flanger,
    FeedbackChorus,
    ShortDelay,
}

impl ChorusType {
    /// The type for a fascia index 0-8; out of range folds to the
    /// power-on default.
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Off,
            1 => Self::Chorus1,
            2 => Self::Chorus2,
            4 => Self::Celeste1,
            5 => Self::Celeste2,
            6 => Self::Flanger,
            7 => Self::FeedbackChorus,
            8 => Self::ShortDelay,
            _ => Self::Chorus3,
        }
    }

    pub fn index(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Chorus1 => 1,
            Self::Chorus2 => 2,
            Self::Chorus3 => 3,
            Self::Celeste1 => 4,
            Self::Celeste2 => 5,
            Self::Flanger => 6,
            Self::FeedbackChorus => 7,
            Self::ShortDelay => 8,
        }
    }

    /// The name the glass shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Chorus1 => "Chorus 1",
            Self::Chorus2 => "Chorus 2",
            Self::Chorus3 => "Chorus 3",
            Self::Celeste1 => "Celeste 1",
            Self::Celeste2 => "Celeste 2",
            Self::Flanger => "Flanger",
            Self::FeedbackChorus => "Feedback Ch",
            Self::ShortDelay => "Short Delay",
        }
    }

    /// (delay s, depth s, rate Hz, feedback gain) for the DSP. The
    /// Roland macro values (delay, depth, rate, feedback in unit steps)
    /// go through the conversions in the type's doc comment; Off keeps
    /// harmless numbers and a zero return in the mixer.
    pub(crate) fn params(self) -> (f64, f64, f64, f32) {
        // (pre-delay units, depth units, rate units, feedback units)
        let (delay_n, depth_n, rate_n, feedback_n) = match self {
            Self::Off => (80.0, 19.0, 3.0, 0),
            // Chorus 1: shallow and slow, barely a doubling.
            Self::Chorus1 => (112.0, 5.0, 3.0, 0),
            // Chorus 2: quicker, still light.
            Self::Chorus2 => (80.0, 19.0, 9.0, 5),
            // Chorus 3: the power-on default, measured off the unit.
            Self::Chorus3 => (80.0, 19.0, 3.0, 8),
            // Celeste: light detuned shimmer, no feedback.
            Self::Celeste1 => (48.0, 10.0, 6.0, 0),
            Self::Celeste2 => (32.0, 14.0, 10.0, 0),
            // Flanger: short sweep, heavy feedback, the jet.
            Self::Flanger => (127.0, 5.0, 1.0, 112),
            Self::FeedbackChorus => (127.0, 24.0, 2.0, 64),
            Self::ShortDelay => (127.0, 0.0, 0.0, 80),
        };
        let delay = (1.0 + 6.0 * delay_n) / 32_000.0;
        let depth = (10.0 * depth_n / 2.0) / 32_000.0;
        // Rate 0 (Short Delay) still needs a table; the sweep is zero
        // wide, so the frequency only sizes it.
        let rate = f64::max(0.116 * rate_n + 0.11, 0.11);
        let feedback = (feedback_n >> 1) as f32 / 64.0;
        (delay, depth, rate, feedback)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct Chorus {
    buffer_l: Vec<f32>,
    buffer_r: Vec<f32>,

    delay_table: Vec<f32>,

    buffer_index: usize,

    delay_table_index_l: usize,
    delay_table_index_r: usize,

    /// Wet fed back into the line; Chorus 3 runs a whisper (4/64),
    /// the flanger a mouthful.
    feedback: f32,
}

impl Chorus {
    /// A chorus running the given type's character.
    pub(crate) fn of_type(sample_rate: i32, chorus_type: ChorusType) -> Self {
        let (delay, depth, rate, feedback) = chorus_type.params();
        let mut chorus = Chorus::new(sample_rate, delay, depth, rate);
        chorus.feedback = feedback;
        chorus
    }

    pub(crate) fn new(sample_rate: i32, delay: f64, depth: f64, frequency: f64) -> Self {
        let buffer_l = vec![0_f32; ((sample_rate as f64) * (delay + depth)) as usize + 2];
        let buffer_r = vec![0_f32; ((sample_rate as f64) * (delay + depth)) as usize + 2];

        // A triangle sweep, as the unit's own DSP runs it: the tap walks
        // the span at a constant rate and turns round, rather than
        // easing sinusoidally through it.
        let mut delay_table = vec![0_f32; ((sample_rate as f64) / frequency).round() as usize];
        let delay_table_length = delay_table.len();
        for (t, input) in delay_table.iter_mut().enumerate().take(delay_table_length) {
            let phase = (t as f64) / (delay_table_length as f64);
            let triangle = 1.0 - 4.0 * (phase - 0.5).abs();
            *input = ((sample_rate as f64) * (delay + depth * triangle)) as f32;
        }

        let buffer_index: usize = 0;

        // The two taps sweep in opposition -- one rises while the other
        // falls -- which is what spreads the image.
        let delay_table_index_l: usize = 0;
        let delay_table_index_r: usize = delay_table_length / 2;

        Self {
            buffer_l,
            buffer_r,
            delay_table,
            buffer_index,
            delay_table_index_l,
            delay_table_index_r,
            feedback: 0.0625,
        }
    }

    pub(crate) fn process(
        &mut self,
        input_left: &[f32],
        input_right: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
    ) {
        let buffer_length = self.buffer_l.len();
        let delay_table_length = self.delay_table.len();
        let output_length = output_left.len();

        for t in 0..output_length {
            {
                let mut position =
                    self.buffer_index as f64 - self.delay_table[self.delay_table_index_l] as f64;
                if position < 0.0 {
                    position += buffer_length as f64;
                }

                let index1 = position as usize;
                let mut index2 = index1 + 1;
                if index2 == buffer_length {
                    index2 = 0;
                }

                let x1 = self.buffer_l[index1] as f64;
                let x2 = self.buffer_l[index2] as f64;
                let a = position - index1 as f64;
                output_left[t] = (x1 + a * (x2 - x1)) as f32;

                self.delay_table_index_l += 1;
                if self.delay_table_index_l == delay_table_length {
                    self.delay_table_index_l = 0;
                }
            }

            {
                let mut position =
                    self.buffer_index as f64 - self.delay_table[self.delay_table_index_r] as f64;
                if position < 0.0 {
                    position += buffer_length as f64;
                }

                let index1 = position as usize;
                let mut index2 = index1 + 1;
                if index2 == buffer_length {
                    index2 = 0;
                }

                let x1 = self.buffer_r[index1] as f64;
                let x2 = self.buffer_r[index2] as f64;
                let a = position - index1 as f64;
                output_right[t] = (x1 + a * (x2 - x1)) as f32;

                self.delay_table_index_r += 1;
                if self.delay_table_index_r == delay_table_length {
                    self.delay_table_index_r = 0;
                }
            }

            self.buffer_l[self.buffer_index] = input_left[t] + self.feedback * output_left[t];
            self.buffer_r[self.buffer_index] = input_right[t] + self.feedback * output_right[t];
            self.buffer_index += 1;
            if self.buffer_index == buffer_length {
                self.buffer_index = 0;
            }
        }
    }

    pub(crate) fn mute(&mut self) {
        let buffer_length = self.buffer_l.len();

        for t in 0..buffer_length {
            self.buffer_l[t] = 0_f32;
        }

        for t in 0..buffer_length {
            self.buffer_r[t] = 0_f32;
        }
    }
}
