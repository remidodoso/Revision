//! rev-core — the pure model: event, phrase, phrase_instance, track, tick/tempo,
//! tuning, scale, and the command vocabulary. No I/O, no platform dependencies;
//! the Vision-layer semantics live here. Compiled native for desktop and to WASM
//! for the web family member (R-104). Pitch is note-number-native through the
//! tuning seam (R-002); time is integer ticks at 5040 ppq (R-003).

pub mod command;
pub mod error;
pub mod id;
pub mod note;
pub mod phrase;
pub mod scale;
pub mod tick;
pub mod tuning;

pub use command::Command;
pub use error::CoreError;
pub use id::{
    EventId, MaterializedTuningInstanceId, PhraseId, PhraseInstanceId, ScaleId, TrackId, TuningId,
};
pub use note::NoteNumber;
pub use tick::{PPQ, Tick};
