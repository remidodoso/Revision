//! Writer internals: the parts that are awkward to observe from outside.

use std::sync::atomic::AtomicU64;

use super::*;

/// A throwaway log file, removed on drop.
struct TempFile(std::path::PathBuf);

impl TempFile {
    fn new(tag: &str) -> TempFile {
        use std::sync::atomic::Ordering;
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "revlog_test_{}_{}_{tag}",
            std::process::id(),
            serial
        ));
        std::fs::create_dir_all(&directory).expect("temp directory");
        TempFile(directory.join("observation.revlog"))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(directory) = self.0.parent() {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

fn entry(text: &str) -> Entry {
    Entry {
        ts: now_micros(),
        creator: crate::creator::APP,
        level: Level::Info,
        text: text.to_string(),
        detail: None,
        keep: false,
    }
}

#[test]
fn session_row_is_written_at_open() {
    let file = TempFile::new("session");
    let writer = Writer::open(&file.0, false).expect("open");
    assert_eq!(writer.session_id(), 1, "first session in a fresh file");

    let count: i64 = writer
        .conn
        .query_row("SELECT count(*) FROM session", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 1);
}

#[test]
fn a_second_open_joins_the_same_file() {
    let file = TempFile::new("join");
    let first = Writer::open(&file.0, false).expect("open");
    drop(first);
    let second = Writer::open(&file.0, false).expect("reopen");

    // The point of a forever file: run two is session 2 in the same database,
    // not session 1 in a new one.
    assert_eq!(second.session_id(), 2);
}

/// Write until the log is comfortably past the prune limit, so a test exercises
/// pruning rather than the early return. Written as a loop against the measured
/// size rather than a row count, so retuning `SIZE_LIMIT` cannot silently make
/// these tests vacuous.
fn fill_past_limit(writer: &mut Writer, keep: bool) -> usize {
    let mut written = 0;
    while writer.used_bytes().expect("size") <= SIZE_LIMIT + SIZE_LIMIT / 4 {
        let batch: Vec<Entry> = (0..BATCH)
            .map(|i| {
                let mut e = entry(&format!(
                    "row {} with enough text to occupy a useful fraction of a page",
                    written + i
                ));
                e.keep = keep;
                e
            })
            .collect();
        writer.insert(&batch).expect("insert");
        written += BATCH;
    }
    written
}

#[test]
fn used_bytes_falls_after_pruning_even_though_the_file_does_not() {
    let file = TempFile::new("prune");
    let mut writer = Writer::open(&file.0, false).expect("open");
    fill_past_limit(&mut writer, false);

    let before = writer.used_bytes().expect("size");
    assert!(before > SIZE_LIMIT);

    writer.prune().expect("prune");
    let after = writer.used_bytes().expect("size");
    assert!(
        after <= PRUNE_TARGET,
        "prune should reach the target: {before} -> {after}, target {PRUNE_TARGET}"
    );

    // And the rows that survived are the *newest* ones.
    let oldest: i64 = writer
        .conn
        .query_row("SELECT min(id) FROM entry", [], |row| row.get(0))
        .expect("min");
    assert!(oldest > 1, "oldest rows are the ones deleted");
}

#[test]
fn kept_rows_survive_pruning() {
    let file = TempFile::new("keep");
    let mut writer = Writer::open(&file.0, false).expect("open");

    let mut kept = entry("this one matters");
    kept.keep = true;
    writer.insert(&[kept]).expect("insert kept");

    fill_past_limit(&mut writer, false);
    writer.prune().expect("prune");
    assert!(
        writer.used_bytes().expect("size") <= PRUNE_TARGET,
        "the prunable rows should have gone"
    );

    let survived: i64 = writer
        .conn
        .query_row("SELECT count(*) FROM entry WHERE keep = 1", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(survived, 1, "a kept row is exempt from pruning");
}

#[test]
fn pruning_a_wholly_kept_log_terminates() {
    // The pathological case the pass cap exists for: nothing is prunable, so
    // the loop must exit rather than spin against a size it cannot reduce.
    let file = TempFile::new("allkept");
    let mut writer = Writer::open(&file.0, false).expect("open");

    let written = fill_past_limit(&mut writer, true);
    assert!(writer.used_bytes().expect("size") > SIZE_LIMIT);

    writer.prune().expect("prune must return, not spin");
    let count: i64 = writer
        .conn
        .query_row("SELECT count(*) FROM entry", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, written as i64, "nothing prunable, nothing pruned");
}

#[test]
fn a_full_channel_counts_rather_than_blocks() {
    let dropped = AtomicU64::new(0);
    let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(1);
    tx.try_send(Msg::Write(entry("first"))).expect("fits");

    // Second send has nowhere to go: the rule is drop and count.
    let error = tx.try_send(Msg::Write(entry("second"))).expect_err("full");
    note_send_failure(&error, &dropped);
    assert_eq!(dropped.load(std::sync::atomic::Ordering::Relaxed), 1);

    // A disconnected channel is shutdown, not loss: it is not counted.
    drop(rx);
    let error = tx
        .try_send(Msg::Write(entry("third")))
        .expect_err("disconnected");
    note_send_failure(&error, &dropped);
    assert_eq!(dropped.load(std::sync::atomic::Ordering::Relaxed), 1);
}
