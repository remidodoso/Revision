//! Command execution: the only write path.
//!
//! Every command returns two things — the **resolved** form (executor-assigned
//! ids and timestamps filled in, so replay reproduces exactly) and its
//! **inverse** as further commands. The inverse being ordinary commands, rather
//! than a private undo mechanism, is what lets undo, redo and replay share one
//! machinery (core-01).

mod material;
mod tuning;

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use rev_core::Command;

use crate::error::StoreError;

/// Milliseconds since the Unix epoch, UTC. Stored as an integer rather than
/// formatted text: sortable, compact, and no calendar arithmetic in the store.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Serialize a JSON value for a TEXT column.
pub(crate) fn json_text(value: &serde_json::Value) -> Result<String, StoreError> {
    Ok(serde_json::to_string(value)?)
}

pub(crate) fn optional_json_text(
    value: &Option<serde_json::Value>,
) -> Result<Option<String>, StoreError> {
    match value {
        Some(v) => Ok(Some(serde_json::to_string(v)?)),
        None => Ok(None),
    }
}

pub(crate) fn missing(kind: &'static str, id: i64) -> StoreError {
    StoreError::NotFound { kind, id }
}

/// Apply one command, returning `(resolved, inverse)`.
pub(crate) fn execute(
    conn: &Connection,
    command: Command,
) -> Result<(Command, Vec<Command>), StoreError> {
    match command {
        Command::SetMeta { key, value } => tuning::set_meta(conn, key, value),
        Command::CreateTuning { id, tuning: spec } => tuning::create_tuning(conn, id, spec),
        Command::RemoveTuning { id } => tuning::remove_tuning(conn, id),
        Command::SetTuningNote { tuning_id, note } => {
            tuning::set_tuning_note(conn, tuning_id, note)
        }
        Command::RemoveTuningNote {
            tuning_id,
            note_number,
        } => tuning::remove_tuning_note(conn, tuning_id, note_number),
        Command::MaterializeTuning { id, tuning_id, ts } => {
            tuning::materialize_tuning(conn, id, tuning_id, ts)
        }
        Command::RemoveMaterializedTuning { id } => tuning::remove_materialized_tuning(conn, id),
        Command::CreateScale { id, scale: spec } => tuning::create_scale(conn, id, spec),
        Command::RemoveScale { id } => tuning::remove_scale(conn, id),

        Command::CreatePhrase { id, phrase: spec } => material::create_phrase(conn, id, spec),
        Command::RemovePhrase { id } => material::remove_phrase(conn, id),
        Command::SetPhrase { id, patch } => material::set_phrase(conn, id, patch),
        Command::CreateTrack { id, track: spec } => material::create_track(conn, id, spec),
        Command::RemoveTrack { id } => material::remove_track(conn, id),
        Command::AddEvent { container, event } => material::add_event(conn, container, event),
        Command::RemoveEvent { id } => material::remove_event(conn, id),
        Command::CreatePhraseInstance {
            id,
            phrase_instance: spec,
        } => material::create_phrase_instance(conn, id, spec),
        Command::RemovePhraseInstance { id } => material::remove_phrase_instance(conn, id),
        Command::SetPhraseInstanceParam { id, patch } => {
            material::set_phrase_instance_param(conn, id, patch)
        }
        Command::SetTempo { phrase_id, point } => material::set_tempo(conn, phrase_id, point),
        Command::RecordBatch { track_id, event } => material::record_batch(conn, track_id, event),
    }
}
