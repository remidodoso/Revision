//! Note numbers to frequencies.
//!
//! **The one place this conversion happens.** The compiled schedule, the live
//! MIDI path, and the piano roll all need it (R-312, R-941), and if they each
//! did it themselves they would eventually disagree — inaudibly, until it is a
//! bug report about a note that looks right and sounds wrong.
//!
//! Materializing a tuning means reading a table out of the project, so it is
//! cached by id. A project has a handful of tunings even when its material is
//! mixed (R-418), so the cache is small and lives as long as the compiler does.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use rev_core::NoteNumber;
use rev_core::id::TuningId;
use rev_core::tuning::MaterializedTuning;
use rev_store::{Project, query};

use crate::error::SchedError;

/// Materialized tunings, kept for as long as the compiler runs.
#[derive(Default)]
pub struct TuneCache {
    tuning: HashMap<i64, MaterializedTuning>,
}

impl TuneCache {
    pub fn new() -> TuneCache {
        TuneCache::default()
    }

    /// Drop everything. Called when a gesture touches a tuning — the store's
    /// hooks are registered for exactly this kind of invalidation, and a whole
    /// flush is right because the alternative is tracking which tuning changed in
    /// order to save re-reading a table of a few hundred rows.
    pub fn clear(&mut self) {
        self.tuning.clear();
    }

    /// The frequency of a note number in a tuning, or `None` if the tuning does
    /// not reach that far.
    ///
    /// Out of range is a real answer, not an error: a tuning with 24 notes
    /// genuinely has nothing to say about note 400. The caller decides what to do
    /// about it, and the caller records it.
    /// Takes the project rather than a connection, so this crate never names a
    /// database type: the store owns persistence, and the compiler owns music.
    pub fn hz(
        &mut self,
        project: &Project,
        tuning_id: TuningId,
        note: NoteNumber,
    ) -> Result<Option<f64>, SchedError> {
        let key = tuning_id.get();
        let table = match self.tuning.entry(key) {
            Entry::Occupied(held) => held.into_mut(),
            Entry::Vacant(empty) => {
                let conn = project.reader();
                let latest = query::latest_materialized_instance(conn, tuning_id)?
                    .ok_or(SchedError::MissingTuning(key))?;
                let built = query::materialized_tuning(conn, latest)?
                    .ok_or(SchedError::MissingTuning(key))?;
                empty.insert(built)
            }
        };
        Ok(table.freq(note))
    }

    pub fn len(&self) -> usize {
        self.tuning.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tuning.is_empty()
    }
}
