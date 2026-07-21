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
use rev_core::phrase::{
    Container, EventSpec, InstanceContainer, PhraseInstanceSpec, PhraseSpec, TempoPoint, TrackSpec,
};
use rev_core::tick::{PPQ, Tick, bpm_to_usec_per_quarter};
use rev_core::{Command as ModelCommand, PhraseId, TrackId};
use rev_dsp::BakeSpec;
use rev_engine::driver::{Request, offline};
use rev_engine::{Chunk, ChunkHandle, Command, Format, Patch, SampleTime, What, session};
use rev_log::{Log, creator};
use rev_sched::{Compiler, TempoMap};
use rev_store::{Project, StoreError, query};

/// The tune: (note number, length in quarters), 12-ET.
#[rustfmt::skip]
const MHALL: &[(i32, i64)] = &[
    (64, 1), (62, 1), (60, 1), (62, 1),
    (64, 1), (64, 1), (64, 2),
    (62, 1), (62, 1), (62, 2),
    (64, 1), (67, 1), (67, 2),
    (64, 1), (62, 1), (60, 1), (62, 1),
    (64, 1), (64, 1), (64, 1), (64, 1),
    (62, 1), (62, 1), (64, 1), (62, 1),
    (60, 4),
];

/// A mezzo-forte in the 16-bit velocity domain (R-402).
const MEZZO_FORTE: i32 = 49_152;

struct Args {
    bpm: f64,
    tuning: String,
    render: Option<String>,
    device: Option<String>,
    voices: usize,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            bpm: 120.0,
            tuning: "12-ET".to_string(),
            render: None,
            device: None,
            voices: 16,
        }
    }
}

fn parse() -> Args {
    let mut args = Args::default();
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--bpm" => args.bpm = number(&mut argv, "--bpm"),
            "--voices" => args.voices = number(&mut argv, "--voices") as usize,
            "--tuning" => args.tuning = argv.next().unwrap_or(args.tuning),
            "--render" => args.render = argv.next(),
            "--device" => args.device = argv.next(),
            "--help" | "-h" => {
                println!(
                    "rev-mhall [--bpm N] [--tuning 12-ET|16-ET|JI] [--voices N] \
                     [--device NAME] [--render FILE.wav]"
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

fn number(argv: &mut impl Iterator<Item = String>, flag: &str) -> f64 {
    match argv.next().and_then(|v| v.parse().ok()) {
        Some(n) => n,
        None => {
            eprintln!("{flag} wants a number");
            std::process::exit(2);
        }
    }
}

/// Build the tune into a project. One phrase, one arrangement, one instance.
fn build(project: &mut Project, bpm: f64, tuning: &str) -> Result<(PhraseId, TrackId), StoreError> {
    let tuning_id = query::tuning_by_name(project.reader(), tuning)?.map(|t| t.id);
    if tuning_id.is_none() {
        eprintln!("no tuning named {tuning:?}; using the project default");
    }

    project.gesture(|g| {
        let bar = PPQ * 4;
        let mut melody = PhraseSpec::new("Mary Had a Little Lamb", Tick(bar * 8));
        melody.tuning_id = tuning_id;
        let melody = match g.exec(ModelCommand::CreatePhrase {
            id: None,
            phrase: melody,
        })? {
            ModelCommand::CreatePhrase { id: Some(id), .. } => id,
            _ => unreachable!(),
        };

        let mut at = Tick::ZERO;
        let mut event = Vec::with_capacity(MHALL.len());
        for &(note, quarters) in MHALL {
            let duration = Tick(PPQ * quarters);
            event.push(EventSpec::note(at, duration, note, MEZZO_FORTE));
            at = Tick(at.get() + duration.get());
        }
        g.exec(ModelCommand::AddEvent {
            container: Container::Phrase(melody),
            event,
        })?;

        let arrangement = match g.exec(ModelCommand::CreatePhrase {
            id: None,
            phrase: PhraseSpec::new("Arrangement", Tick(bar * 8)),
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
            track: TrackSpec::new(arrangement, "Melody", 0),
        })? {
            ModelCommand::CreateTrack { id: Some(id), .. } => id,
            _ => unreachable!(),
        };
        g.exec(ModelCommand::CreatePhraseInstance {
            id: None,
            phrase_instance: PhraseInstanceSpec::new(
                melody,
                InstanceContainer::Track(track),
                Tick::ZERO,
            ),
        })?;

        Ok((arrangement, track))
    })
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
