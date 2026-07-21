//! The command journal (R-205): append-only history, written in the same
//! transaction as the rows it describes.
//!
//! Undo does not rewind or delete entries — it *appends* an undo marker naming
//! the gesture it reversed. History therefore never lies, the undo stack
//! survives a restart for free, and "how did I get here" replay costs nothing
//! extra. Redo appends in turn.

use rusqlite::{Connection, OptionalExtension, params};

use rev_core::Command;

use crate::error::StoreError;
use crate::exec::now_ms;

pub const KIND_COMMAND: &str = "command";
pub const KIND_UNDO: &str = "undo";
pub const KIND_REDO: &str = "redo";

/// The next gesture number. Gestures are the undo unit; one gesture may contain
/// several commands, each getting its own journal row.
pub(crate) fn next_gesture(conn: &Connection) -> Result<i64, StoreError> {
    let highest: Option<i64> =
        conn.query_row("SELECT MAX(gesture) FROM journal", [], |r| r.get(0))?;
    Ok(highest.unwrap_or(0) + 1)
}

/// Record an executed command: the resolved form as redo, its inverse as undo.
pub(crate) fn append_command(
    conn: &Connection,
    gesture: i64,
    resolved: &Command,
    inverse: &[Command],
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO journal (gesture, kind, target_gesture, ts, command, redo, undo) \
         VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6)",
        params![
            gesture,
            KIND_COMMAND,
            now_ms(),
            resolved.name(),
            serde_json::to_string(resolved)?,
            serde_json::to_string(inverse)?,
        ],
    )?;
    Ok(())
}

/// Record that a gesture was undone or redone.
pub(crate) fn append_marker(
    conn: &Connection,
    gesture: i64,
    kind: &str,
    target_gesture: i64,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO journal (gesture, kind, target_gesture, ts, command, redo, undo) \
         VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL)",
        params![gesture, kind, target_gesture, now_ms()],
    )?;
    Ok(())
}

/// The most recent gesture that is not currently undone, if any.
///
/// "Currently undone" is a count comparison rather than a flag, so a gesture
/// can be undone and redone any number of times without the journal needing to
/// be rewritten.
pub(crate) fn next_undo(conn: &Connection) -> Result<Option<i64>, StoreError> {
    Ok(conn.query_row(
        "SELECT MAX(j.gesture) FROM journal j WHERE j.kind = 'command' \
           AND (SELECT COUNT(*) FROM journal u \
                WHERE u.kind = 'undo' AND u.target_gesture = j.gesture) \
             <= (SELECT COUNT(*) FROM journal r \
                 WHERE r.kind = 'redo' AND r.target_gesture = j.gesture)",
        [],
        |r| r.get::<_, Option<i64>>(0),
    )?)
}

/// The gesture to redo, if the redo stack is live.
///
/// **Redo undoes the last undo** — nothing else. A new command invalidates the
/// stack, so only the run of markers *after the last command entry* is live, and
/// within that run undo pushes and redo pops. The answer is the top of that stack.
///
/// The obvious shortcut — "the lowest gesture that is currently undone" — is wrong,
/// and wrong in a way that corrupts rather than refuses. After a new command
/// invalidates the stack, a later undo makes the last entry a marker again, and the
/// lowest-undone gesture reaches back *past* the new command to a stale one. Redo
/// then replays a payload whose rows the intervening history has moved, and the
/// first symptom is a `NotFound` on a row id that used to exist. Found by proptest:
/// `[AddEvent, Undo, CreatePhrase, RemoveFirstEvent, RemoveFirstEvent, CreatePhrase]`.
pub(crate) fn next_redo(conn: &Connection) -> Result<Option<i64>, StoreError> {
    let last_command: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM journal WHERE kind = 'command'",
        [],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT kind, target_gesture FROM journal \
         WHERE seq > ?1 AND kind IN ('undo', 'redo') ORDER BY seq",
    )?;
    let marker = stmt.query_map(params![last_command], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
    })?;

    // Replay the live run: each undo pushes the gesture it undid, each redo pops it.
    let mut stack: Vec<i64> = Vec::new();
    for entry in marker {
        let (kind, target) = entry?;
        match (kind.as_str(), target) {
            (KIND_UNDO, Some(g)) => stack.push(g),
            (KIND_REDO, _) => {
                stack.pop();
            }
            _ => {}
        }
    }
    Ok(stack.pop())
}

/// The inverse commands for a gesture, ordered to undo it: each entry's
/// inverses in order, with the entries themselves walked backwards.
pub(crate) fn undo_payload(conn: &Connection, gesture: i64) -> Result<Vec<Command>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT undo FROM journal WHERE gesture = ?1 AND kind = 'command' ORDER BY seq DESC",
    )?;
    let rows = statement.query_map([gesture], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for text in rows {
        let batch: Vec<Command> = serde_json::from_str(&text?)?;
        out.extend(batch);
    }
    Ok(out)
}

/// The resolved commands for a gesture, in the order they were applied.
pub(crate) fn redo_payload(conn: &Connection, gesture: i64) -> Result<Vec<Command>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT redo FROM journal WHERE gesture = ?1 AND kind = 'command' ORDER BY seq ASC",
    )?;
    let rows = statement.query_map([gesture], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for text in rows {
        out.push(serde_json::from_str(&text?)?);
    }
    Ok(out)
}

/// The resolved command recorded by a single journal entry.
pub(crate) fn redo_payload_for_entry(
    conn: &Connection,
    seq: i64,
) -> Result<Vec<Command>, StoreError> {
    let text: Option<String> = conn
        .query_row("SELECT redo FROM journal WHERE seq = ?1", [seq], |r| {
            r.get(0)
        })
        .optional()?;
    match text {
        Some(text) => Ok(vec![serde_json::from_str(&text)?]),
        None => Ok(Vec::new()),
    }
}

/// One journal entry, as replay walks it.
pub struct Entry {
    pub seq: i64,
    pub gesture: i64,
    pub kind: String,
    pub target_gesture: Option<i64>,
}

/// The whole history in order — the input to replay and to "how did I get
/// here" inspection.
pub fn entry(conn: &Connection) -> Result<Vec<Entry>, StoreError> {
    let mut statement =
        conn.prepare("SELECT seq, gesture, kind, target_gesture FROM journal ORDER BY seq")?;
    let rows = statement.query_map([], |r| {
        Ok(Entry {
            seq: r.get(0)?,
            gesture: r.get(1)?,
            kind: r.get(2)?,
            target_gesture: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// How many gestures are available to undo and redo — what an editor's menu
/// wants to know.
pub fn depth(conn: &Connection) -> Result<(usize, usize), StoreError> {
    let undoable: i64 = conn.query_row(
        "SELECT COUNT(*) FROM (SELECT DISTINCT j.gesture FROM journal j WHERE j.kind = 'command' \
           AND (SELECT COUNT(*) FROM journal u \
                WHERE u.kind = 'undo' AND u.target_gesture = j.gesture) \
             <= (SELECT COUNT(*) FROM journal r \
                 WHERE r.kind = 'redo' AND r.target_gesture = j.gesture))",
        [],
        |r| r.get(0),
    )?;
    let redoable: i64 = conn.query_row(
        "SELECT COUNT(*) FROM (SELECT DISTINCT j.gesture FROM journal j WHERE j.kind = 'command' \
           AND (SELECT COUNT(*) FROM journal u \
                WHERE u.kind = 'undo' AND u.target_gesture = j.gesture) \
             > (SELECT COUNT(*) FROM journal r \
                WHERE r.kind = 'redo' AND r.target_gesture = j.gesture))",
        [],
        |r| r.get(0),
    )?;
    Ok((undoable as usize, redoable as usize))
}
