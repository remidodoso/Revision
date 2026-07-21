//! Replay: rebuild a project's model state by re-applying its history.
//!
//! Replay is *not* how a project opens — the tables are current state at every
//! commit boundary, so opening is O(1) and crash recovery is SQLite's ordinary
//! transaction atomicity. Replay exists for the other things the journal buys:
//! verifying that history is faithful (the determinism gate), time travel, and
//! "how did I get here" inspection of a generative session.

use rusqlite::Connection;

use rev_core::Command;

use crate::error::StoreError;
use crate::journal;
use crate::project::Project;

/// Re-apply `source`'s history into `target`, which must be a bare project
/// (schema only, no genesis — the genesis gesture is itself in the history).
///
/// Undo and redo markers are replayed as what they were, so a session that was
/// undone and partly redone reproduces its actual final state rather than the
/// state it would have had without them.
pub fn replay(source: &Connection, target: &mut Project) -> Result<(), StoreError> {
    for entry in journal::entry(source)? {
        let batch: Vec<Command> = match entry.kind.as_str() {
            journal::KIND_COMMAND => journal::redo_payload_for_entry(source, entry.seq)?,
            journal::KIND_UNDO => journal::undo_payload(source, expect_target(&entry)?)?,
            journal::KIND_REDO => journal::redo_payload(source, expect_target(&entry)?)?,
            // The CHECK constraint admits nothing else.
            _ => Vec::new(),
        };
        target.apply_unjournaled(batch)?;
    }
    Ok(())
}

fn expect_target(entry: &journal::Entry) -> Result<i64, StoreError> {
    entry.target_gesture.ok_or(StoreError::NotFound {
        kind: "journal target_gesture",
        id: entry.seq,
    })
}
