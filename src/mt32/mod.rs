//! MT-32 fluency for the GM engine: the data an MT-32 -> GM translation
//! stands on. The translation itself -- the byte-stream machine that
//! rewrites a game's MIDI as it plays -- builds on these tables.

pub mod tables;
pub mod translator;
