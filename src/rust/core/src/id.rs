//! Identifier newtypes. Distinct types so a phrase id can never be passed where
//! a track id belongs; `i64` underneath because SQLite row ids are i64.
//!
//! Ids are assigned by the store's executor. Creating commands therefore carry
//! `Option<Id>`: `None` means "allocate one", `Some` means "use exactly this" —
//! which is what replay and redo pass, so reproduction is exact (core-01).

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! define_id {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => { $(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl $name {
            /// The raw row id — used when binding to SQL (rev-core has no
            /// rusqlite dependency, so conversion is the store's job).
            pub fn get(self) -> i64 {
                self.0
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    )* };
}

define_id!(
    /// A phrase: the unit of material (R-401).
    PhraseId,
    /// An event within a phrase or directly on a track (R-402).
    EventId,
    /// A track: an ordered container of events and instances (R-406).
    TrackId,
    /// A placement of a phrase in time, with its own play parameters (R-404/405).
    PhraseInstanceId,
    /// A tuning definition (R-501).
    TuningId,
    /// One materialization of a tuning — the dynamic-tuning funnel.
    MaterializedTuningInstanceId,
    /// A scale: a named subset of pitch classes or note numbers (R-509).
    ScaleId,
    /// A tempo point within a phrase's tempo map.
    TempoPointId,
);
