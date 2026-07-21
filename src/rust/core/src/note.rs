//! The pitch datum (R-002): a **note number** — a signed integer position in a
//! tuning. Signed because a tuning's anchor is a convention (builtins anchor 60
//! at middle C) and dense tunings run well below it; negative note numbers are
//! ordinary, not an error.
//!
//! A **pitch class** is a note number reduced modulo notes-per-period, and
//! exists only for periodic tunings. A **degree** is a position within a scale —
//! it is not this type. (Requirements §6 Definitions.)

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NoteNumber(pub i32);

impl NoteNumber {
    pub fn get(self) -> i32 {
        self.0
    }

    /// The pitch class of this note in a tuning of `note_per_period` notes.
    ///
    /// Uses euclidean remainder, never `%` — note numbers are signed, and bare
    /// `%` yields negative classes below the anchor (the coding standard's
    /// Arithmetic law). Panics if `note_per_period` is not positive.
    pub fn pitch_class(self, note_per_period: i32) -> i32 {
        assert!(
            note_per_period > 0,
            "pitch class requires a positive modulus, got {note_per_period}"
        );
        self.0.rem_euclid(note_per_period)
    }

    /// How many whole periods this note sits above the anchor (negative below).
    pub fn period_index(self, anchor: NoteNumber, note_per_period: i32) -> i32 {
        assert!(
            note_per_period > 0,
            "period index requires a positive modulus, got {note_per_period}"
        );
        (self.0 - anchor.0).div_euclid(note_per_period)
    }

    pub fn offset(self, by: i32) -> NoteNumber {
        NoteNumber(self.0 + by)
    }
}

impl From<i32> for NoteNumber {
    fn from(value: i32) -> Self {
        NoteNumber(value)
    }
}

impl fmt::Display for NoteNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod test;
