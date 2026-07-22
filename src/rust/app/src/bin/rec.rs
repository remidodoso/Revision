//! `rev-rec` — the capture path, end to end and headless (rec-01).
//!
//! Arm an empty track, run the transport, play a MIDI keyboard for a few
//! seconds, and the notes are journaled where they were played (R-807, R-810).
//! Then locate to zero and replay them — the round trip, proved without a single
//! Control Bar pixel. The record button and light are ui-04's; this is the
//! mechanism they will drive.
//!
//! ```text
//! cargo run -p rev-app --bin rev-rec
//! cargo run -p rev-app --bin rev-rec -- --seconds 12 --tuning 16-ET
//! cargo run -p rev-app --bin rev-rec -- --overdub --project take.revision
//! ```
//!
//! **It needs a keyboard and a device.** With neither it still runs — it simply
//! records silence — because the point being demonstrated is the plumbing, and
//! the plumbing is the same whether or not a key is pressed. `--project FILE`
//! keeps the take on disk, so it can later be opened in `rev-roll` (Tier A) to
//! *see* the notes that were just played.
//!
//! The instrument is the same plucked sawtooth `rev-mhall` plays; the timbre is
//! not what is being shown here.

use std::time::{Duration, Instant};

use rev_app::audio::Audio;
use rev_app::midi::Keys;
use rev_app::record::{Mode, Recorder};
use rev_core::phrase::{PhraseSpec, TempoPoint, TrackSpec};
use rev_core::tick::{Tick, bpm_to_usec_per_quarter};
use rev_core::{Command as ModelCommand, PhraseId, TrackId};
use rev_dsp::BakeSpec;
use rev_engine::driver::Request;
use rev_engine::{Chunk, ChunkHandle, Patch, SampleTime, What};
use rev_log::{Log, creator};
use rev_midi::NoteHz;
use rev_sched::{Compiler, TempoMap};
use rev_store::{Project, StoreError, query};

struct Args {
    bpm: f64,
    tuning: String,
    seconds: f64,
    overdub: bool,
    voices: usize,
    device: Option<String>,
    project: Option<String>,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            bpm: 120.0,
            tuning: String::from("12-ET"),
            seconds: 8.0,
            overdub: false,
            voices: 16,
            device: None,
            project: None,
        }
    }
}

fn parse() -> Args {
    let mut args = Args::default();
    let mut argument = std::env::args().skip(1);
    while let Some(flag) = argument.next() {
        match flag.as_str() {
            "--bpm" => args.bpm = number(&flag, argument.next()),
            "--tuning" => args.tuning = want(&flag, argument.next()),
            "--seconds" => args.seconds = number(&flag, argument.next()),
            "--overdub" => args.overdub = true,
            "--voices" => args.voices = number(&flag, argument.next()) as usize,
            "--device" => args.device = Some(want(&flag, argument.next())),
            "--project" => args.project = Some(want(&flag, argument.next())),
            "--help" | "-h" => {
                println!(
                    "rev-rec [--bpm N] [--tuning 12-ET|16-ET|JI] [--seconds N] [--overdub] \
                     [--voices N] [--device NAME] [--project FILE.revision]"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument {other:?}; try --help");
                std::process::exit(2);
            }
        }
    }
    args
}

fn want(flag: &str, value: Option<String>) -> String {
    value.unwrap_or_else(|| {
        eprintln!("{flag} wants a value");
        std::process::exit(2);
    })
}

fn number(flag: &str, value: Option<String>) -> f64 {
    value.and_then(|v| v.parse().ok()).unwrap_or_else(|| {
        eprintln!("{flag} wants a number");
        std::process::exit(2);
    })
}

/// An empty take: one arrangement phrase carrying the tempo and the tuning, and
/// one track to record onto. Recorded notes land as direct events on the track
/// (R-807), resolved through the arrangement's tuning at replay.
fn build(project: &mut Project, bpm: f64, tuning: &str) -> Result<(PhraseId, TrackId), StoreError> {
    let tuning_id = query::tuning_by_name(project.reader(), tuning)?.map(|t| t.id);
    if tuning_id.is_none() {
        eprintln!("no tuning named {tuning:?}; using the project default");
    }
    project.gesture(|g| {
        let bar = rev_core::tick::PPQ * 4;
        let mut arrangement = PhraseSpec::new("Take", Tick(bar * 8));
        arrangement.tuning_id = tuning_id;
        let arrangement = match g.exec(ModelCommand::CreatePhrase {
            id: None,
            phrase: arrangement,
        })? {
            ModelCommand::CreatePhrase { id: Some(id), .. } => id,
            _ => unreachable!(),
        };
        g.exec(ModelCommand::SetTempo {
            phrase_id: arrangement,
            point: vec![TempoPoint {
                at_tick: Tick::ZERO,
                usec_per_quarter: bpm_to_usec_per_quarter(bpm),
            }],
        })?;
        let track = match g.exec(ModelCommand::CreateTrack {
            id: None,
            track: TrackSpec::new(arrangement, "Track 1", 0),
        })? {
            ModelCommand::CreateTrack { id: Some(id), .. } => id,
            _ => unreachable!(),
        };
        Ok((arrangement, track))
    })
}

/// Resolve a tuning to a note→Hz snapshot, so a played key sounds the pitch the
/// tuning says (R-312) — the same resolution the roll draws through.
fn snapshot(project: &Project, tuning: &str) -> NoteHz {
    query::tuning_by_name(project.reader(), tuning)
        .ok()
        .flatten()
        .and_then(|t| {
            query::latest_materialized_instance(project.reader(), t.id)
                .ok()
                .flatten()
        })
        .and_then(|inst| {
            query::materialized_tuning(project.reader(), inst)
                .ok()
                .flatten()
        })
        .map(|t| NoteHz::from_tuning(&t))
        .unwrap_or_else(NoteHz::silent)
}

fn main() {
    let args = parse();
    let log = Log::open_default().unwrap_or_else(|error| {
        eprintln!("rev-rec: no log ({error}); continuing without one");
        Log::hush()
    });

    // A named project stays; an unnamed one is a throwaway removed at the end.
    let (path, scratch) = match &args.project {
        Some(file) => (std::path::PathBuf::from(file), None),
        None => {
            let dir = std::env::temp_dir().join(format!("revision_rec_{}", std::process::id()));
            if let Err(error) = std::fs::create_dir_all(&dir) {
                eprintln!("rev-rec: cannot make a working directory: {error}");
                std::process::exit(1);
            }
            (dir.join("take.revision"), Some(dir))
        }
    };

    let outcome = run(&args, &log, &path);

    if let Some(dir) = scratch {
        let _ = std::fs::remove_dir_all(dir);
    }
    if let Err(error) = outcome {
        eprintln!("rev-rec: {error}");
        std::process::exit(1);
    }
}

fn run(args: &Args, log: &Log, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut project = Project::create(path)?;
    let (arrangement, track) = build(&mut project, args.bpm, &args.tuning)?;
    log.info(
        creator::APP,
        format!(
            "take ready: empty track at {} bpm in {}",
            args.bpm, args.tuning
        ),
    );

    let request = Request {
        name: args.device.clone(),
        ..Request::default()
    };
    let voices = args.voices;
    let mut audio = Audio::open_with(log.clone(), &request, |format| {
        rev_app::pad::instrument(
            Patch::harpington(),
            &BakeSpec::harpington(),
            voices,
            format.sample_rate,
        )
    });
    if !audio.is_audible() {
        eprintln!("rev-rec: no audio device — recording will be silent (try rev-tone --list)");
    }
    let rate = audio.sample_rate();

    // The input fork, stamped against the engine's own clock origin (rec-01 §3).
    let origin = audio.origin();
    let snapshot = snapshot(&project, &args.tuning);
    let mut keys = match audio.take_thru() {
        Some(thru) => Keys::new(thru, snapshot, origin),
        None => unreachable!("audio always yields its thru sender once"),
    };

    // The recorder places notes through the same tempo map the compiler uses.
    let point: Vec<(Tick, i64)> = query::tempo_point(project.reader(), arrangement)?
        .into_iter()
        .map(|p| (p.at_tick, p.usec_per_quarter))
        .collect();
    let mut recorder = Recorder::new(track, TempoMap::new(point, rate));
    let mode = if args.overdub {
        Mode::Overdub
    } else {
        Mode::Replace
    };
    recorder.arm(mode);

    audio.send(What::Locate(SampleTime(0)));
    audio.send(What::Start);
    println!(
        "recording for {:.0}s — play now ({})",
        args.seconds,
        if args.overdub { "overdub" } else { "replace" }
    );

    let until = Instant::now() + Duration::from_secs_f64(args.seconds);
    let mut recorded = 0usize;
    while Instant::now() < until {
        audio.pump();
        keys.poll(log);
        keys.drain(|captured| recorder.capture(captured));
        recorder.observe(&audio.position());
        recorded += recorder.flush(&mut project)?;
        std::thread::sleep(Duration::from_millis(10));
    }

    let held = recorder.disarm();
    audio.send(What::AllNotesOff);
    audio.send(What::Stop);
    audio.pump();
    // One last flush for anything completed in the final frame.
    recorded += recorder.flush(&mut project)?;
    if held > 0 {
        println!("{held} note(s) still held at stop were dropped (never finished)");
    }
    println!("recorded {recorded} note(s) onto the track");

    replay(args, log, &project, arrangement, track, rate, &mut audio)?;
    Ok(())
}

/// Locate to zero and play the take back through the schedule compiler.
fn replay(
    args: &Args,
    log: &Log,
    project: &Project,
    arrangement: PhraseId,
    track: TrackId,
    rate: u32,
    audio: &mut Audio,
) -> Result<(), Box<dyn std::error::Error>> {
    let realized = query::realized(project.reader(), track)?;
    if realized.is_empty() {
        println!("nothing to replay");
        return Ok(());
    }
    let last = realized
        .iter()
        .map(|e| e.at_tick.get() + e.dur_tick.get())
        .max()
        .unwrap_or(0);
    let seconds = last as f64 / rev_core::tick::PPQ as f64 * 60.0 / args.bpm + 2.0;

    let point: Vec<(Tick, i64)> = query::tempo_point(project.reader(), arrangement)?
        .into_iter()
        .map(|p| (p.at_tick, p.usec_per_quarter))
        .collect();
    let mut compiler = Compiler::new(TempoMap::new(point, rate), vec![track]);
    let frames = (seconds * f64::from(rate)) as u64;
    let chunk = compiler.chunk(project, SampleTime(0), SampleTime(frames))?;
    if compiler.unplayable() > 0 {
        eprintln!(
            "rev-rec: {} note(s) fell outside their tuning",
            compiler.unplayable()
        );
    }
    log.info(
        creator::APP,
        format!("replaying {} note(s)", chunk.note.len()),
    );
    println!("replaying {} note(s), {:.1}s", chunk.note.len(), seconds);

    audio.send(What::TakeChunk(ChunkHandle::new(Chunk {
        from: chunk.from,
        to: chunk.to,
        note: chunk.note,
    })));
    audio.send(What::Locate(SampleTime(0)));
    audio.send(What::Start);

    let until = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < until {
        audio.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    audio.send(What::AllNotesOff);
    audio.send(What::Stop);
    audio.pump();
    Ok(())
}
