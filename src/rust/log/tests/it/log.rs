//! The public surface, end to end against a real file.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rev_log::{Level, Log, creator};
use rusqlite::Connection;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("revlog_it_{}_{}", std::process::id(), serial));
        std::fs::create_dir_all(&directory).expect("temp directory");
        TempDir(directory)
    }

    fn file(&self) -> PathBuf {
        self.0.join("observation.revlog")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Read the log back the way a viewer would.
fn rows(path: &std::path::Path) -> Vec<(String, i64, String)> {
    let conn = Connection::open(path).expect("open for reading");
    let mut stmt = conn
        .prepare("SELECT creator, level, text FROM entry ORDER BY id")
        .expect("prepare");
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows")
}

#[test]
fn records_survive_to_disk_in_order() {
    let dir = TempDir::new();
    let log = Log::open_with(&dir.file(), false).expect("open");

    log.info(creator::APP, "started");
    log.warn(creator::ENGINE_STREAM, "no input on this device");
    log.error(creator::ENGINE, "device lost");
    log.flush();

    let rows = rows(&dir.file());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0], ("app".into(), 1, "started".into()));
    assert_eq!(
        rows[1],
        ("engine.stream".into(), 2, "no input on this device".into())
    );
    assert_eq!(rows[2], ("engine".into(), 3, "device lost".into()));
}

#[test]
fn trace_is_off_by_default_and_switchable() {
    let dir = TempDir::new();
    let log = Log::open_with(&dir.file(), false).expect("open");

    assert!(!log.enabled(Level::Trace), "Info is the default threshold");
    log.trace(creator::ENGINE_SCHED, "not recorded");

    log.set_threshold(Level::Trace);
    assert!(log.enabled(Level::Trace));
    log.trace(creator::ENGINE_SCHED, "recorded");
    log.flush();

    let rows = rows(&dir.file());
    assert_eq!(rows.len(), 1, "only the record made after the switch");
    assert_eq!(rows[0].2, "recorded");
}

#[test]
fn one_file_carries_many_sessions() {
    let dir = TempDir::new();

    let first = Log::open_with(&dir.file(), false).expect("open");
    assert_eq!(first.session_id(), 1);
    first.info(creator::APP, "run one");
    drop(first);

    let second = Log::open_with(&dir.file(), false).expect("reopen");
    assert_eq!(second.session_id(), 2, "a session is a column, not a file");
    second.info(creator::APP, "run two");
    second.flush();

    // The cross-session question that per-session files cannot answer.
    let conn = Connection::open(dir.file()).expect("open");
    let sessions: i64 = conn
        .query_row("SELECT count(DISTINCT session_id) FROM entry", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(sessions, 2);
}

#[test]
fn detail_carries_json_beside_the_prose() {
    let dir = TempDir::new();
    let log = Log::open_with(&dir.file(), false).expect("open");
    log.detail(
        creator::ENGINE_STREAM,
        Level::Info,
        "stream open: 48000 Hz, 480 frames",
        r#"{"sample_rate":48000,"block":480}"#,
    );
    log.flush();

    let conn = Connection::open(dir.file()).expect("open");
    let detail: Option<String> = conn
        .query_row("SELECT detail FROM entry ORDER BY id LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("row");
    assert_eq!(
        detail.as_deref(),
        Some(r#"{"sample_rate":48000,"block":480}"#)
    );
}

#[test]
fn a_hushed_log_discards_everything_and_never_fails() {
    let log = Log::hush();
    assert!(!log.enabled(Level::Error));
    log.error(creator::APP, "into the void");
    log.flush();
    assert_eq!(log.dropped(), 0, "discarding is not dropping");
}

#[test]
fn shutdown_writes_the_last_batch() {
    let dir = TempDir::new();
    let log = Log::open_with(&dir.file(), false).expect("open");
    log.info(creator::APP, "the last thing that happened");
    // No flush: dropping the handle must still land the record, because the
    // records just before a shutdown are the interesting ones.
    drop(log);

    let rows = rows(&dir.file());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].2, "the last thing that happened");
}
