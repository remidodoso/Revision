//! The writer thread and the database behind it.
//!
//! One thread owns the connection. Everything else pushes onto a bounded
//! channel and returns immediately — nothing that logs ever touches SQLite, and
//! nothing that logs ever blocks (eng-01 §9.3; the same rule R-1509 states for
//! the store). When the channel is full the record is dropped and counted,
//! because an observation is not intent: blocking a caller to preserve a log
//! line inverts the priority order the whole design rests on.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TrySendError};
use std::time::Instant;

use rusqlite::{Connection, params};

use crate::error::LogError;
use crate::{Entry, Level};

/// The data definition, verbatim from `schema.sql`.
pub const DDL: &str = include_str!("schema.sql");

/// Prune when the used size passes this. Deliberately small and deliberately
/// arbitrary (eng-01 §9.4): the log will tell us the real volume, and then we
/// pick a real number.
pub const SIZE_LIMIT: u64 = 1_024 * 1_024;

/// Prune back to this fraction of the limit, so pruning is occasional rather
/// than continuous.
const PRUNE_TARGET: u64 = SIZE_LIMIT * 3 / 4;

/// Check the size every this many written records. Checking per insert would
/// cost two pragmas per row for no benefit.
const PRUNE_INTERVAL: u64 = 2_000;

/// Rows written per transaction. One transaction per record would fsync-per-line;
/// one per batch amortizes it across a burst.
const BATCH: usize = 256;

/// A pruning pass deletes at most this many oldest prunable rows at a time, and
/// repeats while still over target — bounded so a pathological `keep` set cannot
/// spin.
const PRUNE_SLICE: i64 = 4_096;
const PRUNE_PASS_MAX: usize = 8;

pub enum Msg {
    Write(Entry),
    /// Write everything queued, then acknowledge. Tests use this; so will
    /// "attach this log to a bug report".
    Flush(std::sync::mpsc::Sender<()>),
    Stop,
}

pub struct Writer {
    conn: Connection,
    session_id: i64,
    /// Wall clock at session start, for the stderr echo's relative timestamps.
    started: Instant,
    echo: bool,
    since_prune: u64,
}

impl Writer {
    pub fn open(path: &std::path::Path, echo: bool) -> Result<Writer, LogError> {
        let conn = Connection::open(path)?;

        // WAL so a second instance can write safely and so the viewer never
        // blocks the writer. NORMAL because a log may lose its last few records
        // in a power cut; the project journal may not (R-808).
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Keep the -wal file from dwarfing a 1 MB database: checkpoint every
        // 64 pages rather than the default 1000.
        conn.pragma_update(None, "wal_autocheckpoint", 64)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        conn.execute_batch(DDL)?;

        conn.execute(
            "INSERT INTO session (started, version, platform, build) VALUES (?, ?, ?, ?)",
            params![
                now_micros(),
                env!("CARGO_PKG_VERSION"),
                format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
            ],
        )?;
        let session_id = conn.last_insert_rowid();

        Ok(Writer {
            conn,
            session_id,
            started: Instant::now(),
            echo,
            since_prune: 0,
        })
    }

    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    /// The writer thread's whole life: block for one message, then take
    /// whatever else is already queued and write the lot in one transaction.
    pub fn run(mut self, rx: Receiver<Msg>, dropped: &AtomicU64) {
        let mut batch: Vec<Entry> = Vec::with_capacity(BATCH);
        loop {
            let Ok(first) = rx.recv() else {
                break; // every sender gone
            };
            let mut ack = None;
            let mut stop = false;

            match first {
                Msg::Write(entry) => batch.push(entry),
                Msg::Flush(sender) => ack = Some(sender),
                Msg::Stop => stop = true,
            }

            // Drain what is already waiting, up to a batch.
            while batch.len() < BATCH && !stop && ack.is_none() {
                match rx.try_recv() {
                    Ok(Msg::Write(entry)) => batch.push(entry),
                    Ok(Msg::Flush(sender)) => ack = Some(sender),
                    Ok(Msg::Stop) => stop = true,
                    Err(_) => break,
                }
            }

            // A gap in the log is itself an observation: report it rather than
            // leaving the reader to wonder.
            let lost = dropped.swap(0, Ordering::Relaxed);
            if lost > 0 {
                batch.push(Entry {
                    ts: now_micros(),
                    creator: crate::creator::LOG,
                    level: Level::Warn,
                    text: format!("{lost} records dropped: the log channel was full"),
                    detail: None,
                    keep: false,
                });
            }

            if !batch.is_empty() {
                self.write(&batch);
                batch.clear();
            }
            if let Some(sender) = ack {
                let _ = sender.send(());
            }
            if stop {
                break;
            }
        }
    }

    fn write(&mut self, batch: &[Entry]) {
        if self.echo {
            for entry in batch {
                let t = self.started.elapsed().as_secs_f64();
                eprintln!(
                    "[{t:9.3}] {:<5} {}: {}",
                    entry.level.as_str(),
                    entry.creator,
                    entry.text
                );
            }
        }

        // A failed write is not reportable to anyone — the caller is long gone
        // and reporting it through the log would recurse. Echo it and continue:
        // losing the log must never take the application with it.
        if let Err(error) = self.insert(batch) {
            eprintln!("rev-log: cannot write {} records: {error}", batch.len());
            return;
        }

        self.since_prune += batch.len() as u64;
        if self.since_prune >= PRUNE_INTERVAL {
            self.since_prune = 0;
            if let Err(error) = self.prune() {
                eprintln!("rev-log: prune failed: {error}");
            }
        }
    }

    fn insert(&mut self, batch: &[Entry]) -> Result<(), rusqlite::Error> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO entry (session_id, ts, creator, level, text, detail, keep)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )?;
            for entry in batch {
                stmt.execute(params![
                    self.session_id,
                    entry.ts,
                    entry.creator,
                    entry.level as i64,
                    entry.text,
                    entry.detail,
                    i64::from(entry.keep),
                ])?;
            }
        }
        tx.commit()
    }

    /// Delete oldest prunable rows until the used size is back under target.
    ///
    /// **Used size, not file size.** Deleting rows does not shrink a SQLite
    /// file — freed pages go on the freelist and are reused — so `page_count`
    /// alone would stay high after a prune and every subsequent check would
    /// prune again. `page_count - freelist_count` is what actually falls.
    /// Not shrinking is the intent: growth stops, and `VACUUM` never stalls the
    /// writer.
    fn prune(&mut self) -> Result<(), rusqlite::Error> {
        for _ in 0..PRUNE_PASS_MAX {
            if self.used_bytes()? <= PRUNE_TARGET {
                return Ok(());
            }
            let deleted = self.conn.execute(
                "DELETE FROM entry WHERE id IN
                   (SELECT id FROM entry WHERE keep = 0 ORDER BY id LIMIT ?)",
                params![PRUNE_SLICE],
            )?;
            if deleted == 0 {
                return Ok(()); // everything left is kept; nothing more to do
            }
        }
        Ok(())
    }

    fn used_bytes(&self) -> Result<u64, rusqlite::Error> {
        let page_count: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let free: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        let page_size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        Ok(((page_count - free).max(0) * page_size) as u64)
    }
}

/// Wall clock in unix microseconds. Monotonicity is not assumed: the stored `id`
/// is the ordering, `ts` is for humans.
pub fn now_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0)
}

/// Classify a failed send. Split out so the counting rule is stated once.
pub fn note_send_failure(error: &TrySendError<Msg>, dropped: &AtomicU64) {
    match error {
        // Full: the expected case, and the one the design chooses. Count it.
        TrySendError::Full(_) => {
            dropped.fetch_add(1, Ordering::Relaxed);
        }
        // Disconnected: the writer is gone (shutdown). Nothing to count.
        TrySendError::Disconnected(_) => {}
    }
}

#[cfg(test)]
mod test;
