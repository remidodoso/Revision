//! `cargo xtask tmon` — the transaction-monitor kill test (R-202/R-808/R-1504).
//!
//! The one claim the whole store-primary design exists to make is that a
//! `kill -9` at any moment loses no committed gesture. This proves it against a
//! real, ungraceful death — not a clean shutdown that flushes on the way out.
//!
//! **Shape.** A parent process spawns a child (this same binary, `tmon --child
//! <project>`) that records into a fresh project by journaling one `RecordBatch`
//! gesture per note — exactly what `Recorder::flush` does — and prints each
//! **acknowledged** commit to stdout *after* the store returns `Ok`. The parent
//! reads those acknowledgements, and once enough have landed, **hard-kills the
//! child** with `Child::kill()` (SIGKILL on unix, `TerminateProcess` on Windows):
//! no unwinding, no destructors, no flush. Then it reopens the project and
//! asserts:
//!
//!   1. the database is **not corrupt** (`PRAGMA integrity_check`);
//!   2. **every acknowledged commit survived** — the track holds at least as many
//!      notes as the child last acknowledged;
//!   3. **nothing partial survived** — every surviving row is a well-formed note,
//!      and the count of notes equals the count of `record_batch` journal
//!      gestures, so no gesture left half of itself behind.
//!
//! The claim is scoped to a **process** kill, which is what WAL plus
//! committed-transaction atomicity guarantee. Power-loss durability is a stronger
//! and separate property (it turns on `synchronous`/fsync timing) and is not what
//! this tests — named so the gate is not mistaken for more than it is.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command as OsCommand, Stdio};

use rev_core::phrase::{EventSpec, PhraseSpec, TrackSpec};
use rev_core::tick::{PPQ, Tick};
use rev_core::{Command, PhraseId, TrackId};
use rev_store::{Project, query};

/// How many acknowledged commits the parent waits for before killing. Enough
/// that the child is deep in a steady stream of transactions — the moment a
/// buffering bug would lose the most.
const TARGET: u64 = 300;

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    match args {
        ["--child", path] => child(Path::new(path)),
        [] => parent(root),
        _ => Err("usage: cargo xtask tmon    (internal: tmon --child <project>)".to_string()),
    }
}

/// The parent: spawn, watch, kill, reopen, assert.
fn parent(root: &Path) -> Result<(), String> {
    // A throwaway project under the workspace target dir — a real file, because
    // WAL durability is a property of a real file, not `:memory:`.
    let dir = root.join("target").join("tmon");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    let project = dir.join(format!("kill_{}.revision", std::process::id()));
    // A stale file from a previous crashed run would poison the test.
    let _ = std::fs::remove_file(&project);
    let _ = std::fs::remove_file(project.with_extension("revision-wal"));
    let _ = std::fs::remove_file(project.with_extension("revision-shm"));

    let exe = std::env::current_exe().map_err(|e| format!("cannot find own path: {e}"))?;
    let mut child = OsCommand::new(exe)
        .args(["tmon", "--child"])
        .arg(&project)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot spawn child: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or("child has no stdout".to_string())?;

    // Read acknowledgements until the child has committed enough, then kill it
    // where it stands — mid-stream, very likely mid-transaction.
    let mut acknowledged = 0u64;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| format!("reading child: {e}"))?;
        if let Ok(n) = line.trim().parse::<u64>() {
            acknowledged = n;
            if acknowledged >= TARGET {
                break;
            }
        }
    }

    child
        .kill()
        .map_err(|e| format!("cannot kill child: {e}"))?;
    let _ = child.wait();

    if acknowledged < TARGET {
        return Err(format!(
            "child died before acknowledging {TARGET} commits (saw {acknowledged}); the test proves nothing"
        ));
    }

    let survivors = verify(&project, acknowledged)?;
    // Clean up only on success; a failure leaves the evidence on disk.
    let _ = std::fs::remove_file(&project);
    let _ = std::fs::remove_file(project.with_extension("revision-wal"));
    let _ = std::fs::remove_file(project.with_extension("revision-shm"));

    println!(
        "tmon: killed the child after {acknowledged} acknowledged commits; \
         reopened clean, {survivors} notes durable — nothing acknowledged was lost"
    );
    Ok(())
}

/// Reopen the killed project and check the three assertions. Returns how many
/// notes survived.
fn verify(project: &Path, acknowledged: u64) -> Result<u64, String> {
    // 1. Not corrupt. A separate raw connection, so the check is SQLite's own.
    let conn = rusqlite::Connection::open(project)
        .map_err(|e| format!("cannot reopen {}: {e}", project.display()))?;
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| format!("integrity_check failed to run: {e}"))?;
    if integrity != "ok" {
        return Err(format!("database is corrupt after the kill: {integrity}"));
    }
    // The count of committed record_batch gestures in the journal.
    let gestures: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM journal WHERE command = 'record_batch'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("cannot count journal gestures: {e}"))?;
    drop(conn);

    // 2 and 3, through the model's own reader.
    let reopened = Project::open(project).map_err(|e| format!("cannot open project: {e}"))?;
    let (_arrangement, track) = first_track(&reopened)?;
    let events = query::event_on_track(reopened.reader(), track)
        .map_err(|e| format!("cannot read the track: {e}"))?;
    let survivors = events.len() as u64;

    // 2. Nothing acknowledged was lost.
    if survivors < acknowledged {
        return Err(format!(
            "durability violated: child acknowledged {acknowledged} commits, but only {survivors} survived the kill"
        ));
    }
    // 3. Nothing partial survived: every gesture is one whole note, so the note
    // count and the gesture count agree, and every note is well formed.
    if survivors as i64 != gestures {
        return Err(format!(
            "partial state after the kill: {survivors} note rows but {gestures} record_batch gestures — a gesture left half of itself behind"
        ));
    }
    for event in &events {
        if event.note_number.is_none() || event.dur_tick.get() < 1 {
            return Err(format!(
                "a surviving note is malformed: {:?} — a torn write slipped through",
                event.id
            ));
        }
    }
    Ok(survivors)
}

/// The child: record forever, acknowledging each commit, until it is killed.
fn child(project: &Path) -> Result<(), String> {
    let mut project = Project::create(project).map_err(|e| format!("child cannot create: {e}"))?;
    let (_arrangement, track) = build_track(&mut project)?;

    let mut stdout = std::io::stdout().lock();
    let mut n: u64 = 0;
    loop {
        n += 1;
        // One note per gesture — the recording command, one committed
        // transaction each, exactly as `Recorder::flush` writes a frame.
        let note = EventSpec::note(Tick(PPQ * n as i64), Tick(PPQ), 60, 40_000);
        project
            .apply(Command::RecordBatch {
                track_id: track,
                event: vec![note],
            })
            .map_err(|e| format!("child commit {n} failed: {e}"))?;
        // Acknowledge *after* the commit returned Ok, and flush so the parent
        // sees it promptly — a block-buffered pipe would hide the truth.
        writeln!(stdout, "{n}").map_err(|e| format!("child cannot report: {e}"))?;
        stdout
            .flush()
            .map_err(|e| format!("child cannot flush: {e}"))?;
    }
}

/// A fresh project's arrangement phrase and its one track. The phrase carries a
/// tuning (12-ET, genesis-seeded) so the take is a realistic one — its notes
/// resolve to pitches and can be drawn on the roll, not just counted.
fn build_track(project: &mut Project) -> Result<(PhraseId, TrackId), String> {
    let tuning_id = query::tuning_by_name(project.reader(), "12-ET")
        .map_err(|e| format!("child cannot find 12-ET: {e}"))?
        .map(|t| t.id);
    project
        .gesture(|g| {
            let mut phrase = PhraseSpec::new("Take", Tick(PPQ * 4 * 512));
            phrase.tuning_id = tuning_id;
            let arrangement = match g.exec(Command::CreatePhrase { id: None, phrase })? {
                Command::CreatePhrase { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            let track = match g.exec(Command::CreateTrack {
                id: None,
                track: TrackSpec::new(arrangement, "Track 1", 0),
            })? {
                Command::CreateTrack { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            Ok((arrangement, track))
        })
        .map_err(|e| format!("child cannot build a track: {e}"))
}

/// The single track a `build_track` project has — found on reopen without
/// assuming its id survived as a particular number.
fn first_track(project: &Project) -> Result<(PhraseId, TrackId), String> {
    let phrases = query::phrase_by_name(project.reader(), "Take")
        .map_err(|e| format!("cannot find the arrangement: {e}"))?;
    let arrangement = phrases.ok_or("no arrangement after reopen".to_string())?.id;
    let tracks = query::track_in_phrase(project.reader(), arrangement)
        .map_err(|e| format!("cannot find the track: {e}"))?;
    let track = tracks
        .first()
        .ok_or("no track after reopen".to_string())?
        .id;
    Ok((arrangement, track))
}
