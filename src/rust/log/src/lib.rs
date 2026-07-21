//! rev-log — the observation log: every significant action leaves a record,
//! cheaply enough that nobody is tempted to switch it off, in a place the user
//! can look at (eng-01 §9).
//!
//! **Not a developer console.** `log`/`tracing` solve a different problem; this
//! is a user-facing feature with a database, a retention policy, and eventually
//! a viewer. Messages are therefore prose, not codes.
//!
//! **Not for the real-time thread.** The audio callback cannot allocate, so it
//! cannot format a string. It pushes fixed-size POD records onto its own ring
//! (rev-engine) and the app thread formats them and calls in here. That is why
//! `rev-engine` does not depend on this crate (eng-01 §14) — and why bundled
//! SQLite stays out of the audio engine's dependency tree.
//!
//! ```no_run
//! # fn main() -> Result<(), rev_log::LogError> {
//! let log = rev_log::Log::open_default()?;
//! log.info(rev_log::creator::APP, "Revision started");
//! # Ok(()) }
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;

pub mod error;
pub mod place;
mod sink;

pub use error::LogError;
pub use place::{data_directory, default_log_path};

/// Bound on the channel between a caller and the writer thread. Beyond this,
/// records are dropped and counted (§9.3) — bounded, because unbounded means
/// allocating without limit while the disk is slow.
const CHANNEL: usize = 8_192;

/// How serious a record is. Four levels, not syslog's eight: the distinctions
/// syslog draws above `Error` are about paging an operator at 3am, which is not
/// a thing this application does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    /// Per-block, per-event detail. A firehose by design — switch it on for one
    /// creator when you are looking for something.
    Trace = 0,
    /// The default. What happened, at the granularity a curious user would want.
    Info = 1,
    /// Something is degraded but proceeding: a dropped record, a device that
    /// offers no input, a schedule that arrived late.
    Warn = 2,
    /// Something failed.
    Error = 3,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// The creator vocabulary: dotted, subsystem first.
///
/// Constants rather than free strings so the set stays enumerable — a viewer
/// filters by these, and a typo that silently creates a new creator would be
/// invisible.
pub mod creator {
    pub const APP: &str = "app";
    pub const LOG: &str = "log";
    pub const UI: &str = "ui";
    pub const UI_TRANSPORT: &str = "ui.transport";
    pub const STORE: &str = "store";
    pub const ENGINE: &str = "engine";
    pub const ENGINE_STREAM: &str = "engine.stream";
    pub const ENGINE_TRANSPORT: &str = "engine.transport";
    pub const ENGINE_SCHED: &str = "engine.sched";
    pub const ENGINE_TIMING: &str = "engine.timing";
}

/// One record, as it crosses to the writer thread.
pub(crate) struct Entry {
    ts: i64,
    creator: &'static str,
    level: Level,
    text: String,
    detail: Option<String>,
    keep: bool,
}

struct Inner {
    tx: Option<SyncSender<sink::Msg>>,
    handle: Option<JoinHandle<()>>,
    /// Shared with the writer thread, which reports and clears it — hence its
    /// own `Arc` rather than a borrow out of this one, which the writer holding
    /// a clone would make impossible.
    dropped: Arc<AtomicU64>,
    threshold: AtomicU8,
    session_id: i64,
}

/// A handle on the log. Cheap to clone; hand one to anything that has something
/// to say.
#[derive(Clone)]
pub struct Log(Arc<Inner>);

impl Log {
    /// Open the log in the OS application-data directory.
    pub fn open_default() -> Result<Log, LogError> {
        Log::open(&place::default_log_path()?)
    }

    /// Open a specific file. Created if absent; joined if present — this is one
    /// forever file across every run, not one file per session (§9.4).
    pub fn open(path: &std::path::Path) -> Result<Log, LogError> {
        // Debug builds echo to stderr as well as to the database, so the log is
        // watchable *while* something is being brought up — which is when the
        // code is least trustworthy and a live trace is worth most (§9.7).
        Log::open_with(path, cfg!(debug_assertions))
    }

    pub fn open_with(path: &std::path::Path, echo: bool) -> Result<Log, LogError> {
        let writer = sink::Writer::open(path, echo)?;
        let session_id = writer.session_id();
        let (tx, rx) = sync_channel(CHANNEL);
        let dropped = Arc::new(AtomicU64::new(0));

        let counter = Arc::clone(&dropped);
        let handle = std::thread::Builder::new()
            .name("rev-log".into())
            .spawn(move || writer.run(rx, &counter))
            .expect("spawn log writer");

        Ok(Log(Arc::new(Inner {
            tx: Some(tx),
            handle: Some(handle),
            dropped,
            threshold: AtomicU8::new(Level::Info as u8),
            session_id,
        })))
    }

    /// A log that discards everything. For tests, and for the degraded case
    /// where no application-data directory exists — running without a log is
    /// worse, never fatal.
    pub fn hush() -> Log {
        Log(Arc::new(Inner {
            tx: None,
            handle: None,
            dropped: Arc::new(AtomicU64::new(0)),
            threshold: AtomicU8::new(Level::Error as u8 + 1),
            session_id: 0,
        }))
    }

    /// Which run of the application this handle writes as.
    pub fn session_id(&self) -> i64 {
        self.0.session_id
    }

    /// Records below this level are discarded before they are formatted.
    ///
    /// Formatting is the expensive part, so [`Log::enabled`] exists to be called
    /// *first* by anything whose message costs real work to build.
    pub fn set_threshold(&self, level: Level) {
        self.0.threshold.store(level as u8, Ordering::Relaxed);
    }

    pub fn enabled(&self, level: Level) -> bool {
        self.0.tx.is_some() && level as u8 >= self.0.threshold.load(Ordering::Relaxed)
    }

    pub fn trace(&self, creator: &'static str, text: impl Into<String>) {
        self.record(creator, Level::Trace, text, None, false);
    }

    pub fn info(&self, creator: &'static str, text: impl Into<String>) {
        self.record(creator, Level::Info, text, None, false);
    }

    pub fn warn(&self, creator: &'static str, text: impl Into<String>) {
        self.record(creator, Level::Warn, text, None, false);
    }

    pub fn error(&self, creator: &'static str, text: impl Into<String>) {
        self.record(creator, Level::Error, text, None, false);
    }

    /// A record with structured detail alongside the prose. `detail` is JSON;
    /// the column is nullable so structure can arrive later without a migration.
    pub fn detail(
        &self,
        creator: &'static str,
        level: Level,
        text: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.record(creator, level, text, Some(detail.into()), false);
    }

    /// A record exempt from pruning — what turns a rolling buffer into
    /// something you can file a bug from.
    pub fn keep(&self, creator: &'static str, level: Level, text: impl Into<String>) {
        self.record(creator, level, text, None, true);
    }

    fn record(
        &self,
        creator: &'static str,
        level: Level,
        text: impl Into<String>,
        detail: Option<String>,
        keep: bool,
    ) {
        if !self.enabled(level) {
            return;
        }
        let Some(tx) = self.0.tx.as_ref() else {
            return;
        };
        let entry = Entry {
            ts: sink::now_micros(),
            creator,
            level,
            text: text.into(),
            detail,
            keep,
        };
        if let Err(error) = tx.try_send(sink::Msg::Write(entry)) {
            sink::note_send_failure(&error, &self.0.dropped);
        }
    }

    /// Block until everything sent so far is on disk.
    ///
    /// Not for ordinary use — logging is a side channel and callers should not
    /// wait on it. Tests need it, and so will "attach the log to a bug report".
    pub fn flush(&self) {
        let Some(tx) = self.0.tx.as_ref() else {
            return;
        };
        let (ack, done) = std::sync::mpsc::channel();
        if tx.send(sink::Msg::Flush(ack)).is_ok() {
            let _ = done.recv();
        }
    }

    /// Records dropped because the channel was full, since the last report.
    pub fn dropped(&self) -> u64 {
        self.0.dropped.load(Ordering::Relaxed)
    }
}

// Unit tests for the writer live beside it (`sink/test.rs`); the public API is
// exercised end to end from `tests/it`, because opening a real file is the
// thing worth testing.

impl Drop for Inner {
    fn drop(&mut self) {
        // Stop, then join: the writer must finish its last batch before the
        // process exits, or the most interesting records — the ones just before
        // a shutdown — are the ones that never arrive.
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(sink::Msg::Stop);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
