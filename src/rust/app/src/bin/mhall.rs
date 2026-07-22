//! `rev-mhall` — the first musical sound.
//!
//! Builds "Mary Had a Little Lamb" into a throwaway project, compiles it through
//! the tempo map and the tuning, and plays it — or renders it to a file.
//!
//! ```text
//! cargo run -p rev-app --bin rev-mhall
//! cargo run -p rev-app --bin rev-mhall -- --tuning 16-ET
//! cargo run -p rev-app --bin rev-mhall -- --bpm 90 --render mhall.wav
//! ```
//!
//! **The instrument is a plucked sawtooth, not a harpsichord.** The PADsynth
//! bake is dsp-02's and does not exist yet, so this reads a synthetic table.
//! What is being demonstrated is that stored material becomes sound at the right
//! pitches and the right samples; the timbre arrives later.
//!
//! The tune is built here rather than taken from `rev-testkit`, which is test
//! scaffolding and is never shipped (coding standard, "Tests").

use std::time::{Duration, Instant};

use rev_app::audio::Audio;
use rev_core::tick::Tick;

use rev_core::{PhraseId, TrackId};
use rev_dsp::BakeSpec;
use rev_engine::driver::{Request, offline};
use rev_engine::{Chunk, ChunkHandle, Command, Format, Patch, SampleTime, What, session};
use rev_log::{Log, creator};
use rev_sched::{Compiler, TempoMap};
use rev_store::{Project, query};

use rev_app::mhall::{MHALL, build};

/// What the command line said. Plain parsing: this is a bring-up binary, and a
/// dependency for six flags would be a dependency for six flags.
struct Args {
    bpm: f64,
    tuning: String,
    voices: usize,
    device: Option<String>,
    render: Option<String>,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            bpm: 120.0,
            tuning: String::from("12-ET"),
            voices: 16,
            device: None,
            render: None,
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
            "--voices" => args.voices = number(&flag, argument.next()) as usize,
            "--device" => args.device = Some(want(&flag, argument.next())),
            "--render" => args.render = Some(want(&flag, argument.next())),
            "--help" | "-h" => {
                println!(
                    "rev-mhall [--bpm N] [--tuning 12-ET|16-ET|JI] [--voices N] [--device NAME] [--render FILE.wav]"
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
    match value {
        Some(value) => value,
        None => {
            eprintln!("{flag} wants a value");
            std::process::exit(2);
        }
    }
}

fn number(flag: &str, value: Option<String>) -> f64 {
    match value.and_then(|v| v.parse().ok()) {
        Some(value) => value,
        None => {
            eprintln!("{flag} wants a number");
            std::process::exit(2);
        }
    }
}

fn main() {
    let args = parse();
    let log = Log::open_default().unwrap_or_else(|error| {
        eprintln!("rev-mhall: no log ({error}); continuing without one");
        Log::hush()
    });

    // A throwaway project on disk, removed when we are done: the point is the
    // sound, not the file.
    let directory = std::env::temp_dir().join(format!("revision_mhall_{}", std::process::id()));
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("rev-mhall: cannot make a working directory: {error}");
        std::process::exit(1);
    }
    let path = directory.join("mhall.revision");

    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut project = Project::create(&path)?;
        let (arrangement, track) = build(&mut project, args.bpm, &args.tuning)?;
        log.info(
            creator::APP,
            format!(
                "MHALL built: {} notes at {} bpm in {}",
                MHALL.len(),
                args.bpm,
                args.tuning
            ),
        );

        // Seconds of music: 32 quarters plus room for the last note to ring out.
        let seconds = 32.0 * 60.0 / args.bpm + 3.0;

        match &args.render {
            Some(file) => render(&log, &project, arrangement, track, seconds, &args, file)?,
            None => play(&log, &project, arrangement, track, seconds, &args)?,
        }
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&directory);
    if let Err(error) = outcome {
        eprintln!("rev-mhall: {error}");
        std::process::exit(1);
    }
}

/// Compile the arrangement into one chunk covering the whole tune.
fn compile(
    project: &Project,
    arrangement: PhraseId,
    track: TrackId,
    seconds: f64,
    sample_rate: u32,
) -> Result<Chunk, Box<dyn std::error::Error>> {
    let point: Vec<(Tick, i64)> = query::tempo_point(project.reader(), arrangement)?
        .into_iter()
        .map(|p| (p.at_tick, p.usec_per_quarter))
        .collect();
    let mut compiler = Compiler::new(TempoMap::new(point, sample_rate), vec![track]);
    let frames = (seconds * f64::from(sample_rate)) as u64;
    let chunk = compiler.chunk(project, SampleTime(0), SampleTime(frames))?;
    if compiler.unplayable() > 0 {
        eprintln!(
            "rev-mhall: {} notes fell outside their tuning",
            compiler.unplayable()
        );
    }
    Ok(chunk)
}

fn play(
    log: &Log,
    project: &Project,
    arrangement: PhraseId,
    track: TrackId,
    seconds: f64,
    args: &Args,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = Request {
        name: args.device.clone(),
        ..Request::default()
    };
    let voices = args.voices;
    let mut audio = Audio::open_with(log.clone(), &request, |format| {
        let rate = format.sample_rate;
        rev_app::pad::instrument(Patch::harpington(), &BakeSpec::harpington(), voices, rate)
    });
    if !audio.is_audible() {
        return Err("no audio device — try rev-tone --list".into());
    }
    let rate = audio.sample_rate();

    let chunk = compile(project, arrangement, track, seconds, rate)?;
    println!("{} notes, {:.1} s", chunk.note.len(), seconds);
    audio.send(What::TakeChunk(ChunkHandle::new(chunk)));
    audio.send(What::Start);

    let until = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < until {
        audio.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    audio.send(What::AllNotesOff);
    audio.send(What::Stop);
    audio.pump();

    let seen = audio.position();
    println!(
        "{} blocks, {} xruns, worst callback {} us",
        seen.block, seen.xrun, seen.callback_worst_us
    );
    Ok(())
}

fn render(
    log: &Log,
    project: &Project,
    arrangement: PhraseId,
    track: TrackId,
    seconds: f64,
    args: &Args,
    file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    const RATE: u32 = 48_000;
    let frames = (seconds * f64::from(RATE)) as usize;

    // Render twice and compare — R-1402's gate, exercised by hand as well as by
    // the test suite, on real material rather than a tone.
    let once = || -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let chunk = compile(project, arrangement, track, seconds, RATE)?;
        let (mut app, rt) = session();
        let mut driver = rev_engine::driver::Offline::new(Format::stereo(RATE, 512), rt);
        if let Some(instrument) = rev_app::pad::instrument(
            Patch::harpington(),
            &BakeSpec::harpington(),
            args.voices,
            RATE,
        ) {
            driver.engine().set_instrument(instrument);
        }
        app.send(Command::now(What::TakeChunk(ChunkHandle::new(chunk))))
            .map_err(|_| "the command ring refused a chunk")?;
        app.send(Command::now(What::Start))
            .map_err(|_| "the command ring refused start")?;
        Ok(driver.render(frames))
    };

    let first = once()?;
    let second = once()?;
    let identical = first
        .iter()
        .zip(&second)
        .all(|(a, b)| a.to_bits() == b.to_bits());

    offline::write_wav(std::path::Path::new(file), &first, RATE, 2)?;
    let message = format!(
        "rendered {frames} frames to {file}; two renders {}",
        if identical {
            "bit-identical"
        } else {
            "DIFFER — this is a defect"
        }
    );
    log.info(creator::APP, message.clone());
    println!("{message}");

    if !identical {
        return Err("renders differ".into());
    }
    Ok(())
}
