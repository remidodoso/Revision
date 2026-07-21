//! Errors raised by pure model logic. Anything needing a database lives in
//! rev-store's `StoreError`, which wraps this.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("tuning '{name}' is kind '{kind}' but {missing}")]
    TuningIncomplete {
        name: String,
        kind: &'static str,
        missing: &'static str,
    },

    #[error(
        "tuning '{name}' is periodic with {expected} notes per period but has {found} canonical note rows"
    )]
    TuningNoteCount {
        name: String,
        expected: i32,
        found: usize,
    },

    #[error("tuning '{name}' canonical notes are not contiguous from the anchor")]
    TuningNoteGap { name: String },

    #[error("tuning '{name}' has no notes to materialize")]
    TuningEmpty { name: String },

    #[error("tuning '{name}' produced a non-increasing frequency at note {note}: {freq}")]
    TuningNotMonotone { name: String, note: i32, freq: f64 },

    #[error("tuning '{name}' produced a non-finite or non-positive frequency at note {note}")]
    TuningBadFrequency { name: String, note: i32 },

    #[error("ratio {num}/{den} is not positive")]
    BadRatio { num: i64, den: i64 },

    #[error("scale '{name}' is periodic with modulus {modulus} but its mask contains {value}")]
    ScaleMaskOutOfRange {
        name: String,
        modulus: i32,
        value: i32,
    },

    #[error("scale '{name}' has an empty mask")]
    ScaleEmpty { name: String },
}
