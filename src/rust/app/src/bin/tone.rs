//! `rev-tone` — the headless first sound.
//!
//! Opens a device, makes a tone, prints what it did. No window, no widgets, no
//! node graph, no schedule compiler. If this is silent, the problem is the
//! device, the driver, the ring or the clock — and *knowing which* is the whole
//! reason it exists before the Control Bar is wired to anything.
//!
//! ```text
//! cargo run -p rev-app --bin rev-tone
//! cargo run -p rev-app --bin rev-tone -- --list
//! cargo run -p rev-app --bin rev-tone -- --device "Speakers" --hz 220 --seconds 3
//! cargo run -p rev-app --bin rev-tone -- --render tone.wav
//! ```

use std::time::{Duration, Instant};

use rev_app::audio::Audio;
use rev_engine::driver::{Device, Offline, Request, offline};
use rev_engine::{Command, Format, SampleTime, What, session};
use rev_log::{Level, Log, creator};

struct Args {
    hz: f32,
    gain: f32,
    seconds: f64,
    device: Option<String>,
    render: Option<String>,
    list: bool,
    trace: bool,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            // A4 at 440, because the first thing to check is whether it is the
            // pitch you expected — a wrong sample rate is audible as a wrong
            // note, which is a better diagnostic than any log line.
            hz: 440.0,
            gain: 0.25,
            seconds: 2.0,
            device: None,
            render: None,
            list: false,
            trace: false,
        }
    }
}

fn parse() -> Args {
    let mut args = Args::default();
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--list" => args.list = true,
            "--trace" => args.trace = true,
            "--hz" => args.hz = next_number(&mut argv, "--hz") as f32,
            "--gain" => args.gain = next_number(&mut argv, "--gain") as f32,
            "--seconds" => args.seconds = next_number(&mut argv, "--seconds"),
            "--device" => args.device = argv.next(),
            "--render" => args.render = argv.next(),
            "--help" | "-h" => {
                println!(
                    "rev-tone [--list] [--device NAME] [--hz N] [--gain N] \
                     [--seconds N] [--render FILE.wav] [--trace]"
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

fn next_number(argv: &mut impl Iterator<Item = String>, flag: &str) -> f64 {
    match argv.next().and_then(|value| value.parse().ok()) {
        Some(number) => number,
        None => {
            eprintln!("{flag} wants a number");
            std::process::exit(2);
        }
    }
}

fn main() {
    let args = parse();

    if args.list {
        for device in Device::list() {
            println!("{device}");
        }
        return;
    }

    // Running without a log is degraded, not fatal — so a log that cannot open
    // is reported and replaced by one that discards.
    let log = match Log::open_default() {
        Ok(log) => log,
        Err(error) => {
            eprintln!("rev-tone: no log ({error}); continuing without one");
            Log::hush()
        }
    };
    if args.trace {
        log.set_threshold(Level::Trace);
    }

    if let Some(path) = &args.render {
        render_to_file(&log, &args, path);
        return;
    }

    play(log, &args);
}

/// The live path: a device, a callback, and a tone.
fn play(log: Log, args: &Args) {
    let request = Request {
        name: args.device.clone(),
        ..Request::default()
    };
    let mut audio = Audio::open(log, &request);
    if !audio.is_audible() {
        eprintln!("rev-tone: no device — nothing to hear. Try --list.");
        std::process::exit(1);
    }

    audio.log().info(
        creator::APP,
        format!(
            "tone: {} Hz at gain {} for {} s",
            args.hz, args.gain, args.seconds
        ),
    );
    audio.send(What::ToneOn {
        hz: args.hz,
        gain: args.gain,
    });

    // Pump while it sounds: the app half of the seam is not optional, and a
    // program that never drains would eventually fill the observation ring and
    // start losing records — which is exactly what the drop counter would tell
    // us, and exactly the bug worth catching in the simplest possible program.
    let until = Instant::now() + Duration::from_secs_f64(args.seconds);
    while Instant::now() < until {
        audio.pump();
        std::thread::sleep(Duration::from_millis(10));
    }

    // Let the gain ramp reach zero before the stream closes, or the last thing
    // heard is a click — which would be indistinguishable from a real defect.
    audio.send(What::ToneOff);
    let fade = Instant::now() + Duration::from_millis(100);
    while Instant::now() < fade {
        audio.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    audio.pump();

    let seen = audio.position();
    println!(
        "{} blocks, {} xruns, worst callback {} us, clock at {:.3} s",
        seen.block,
        seen.xrun,
        seen.callback_worst_us,
        seen.at.seconds(seen.sample_rate),
    );
}

/// The offline path: the same engine, no device. Renders twice and compares, so
/// the bit-identity gate (R-1402) is exercised by hand as well as by tests.
fn render_to_file(log: &Log, args: &Args, path: &str) {
    let rate = 48_000;
    let frames = (args.seconds * f64::from(rate)) as usize;

    let render = || {
        let (mut app, rt) = session();
        let mut offline = Offline::new(Format::stereo(rate, 480), rt);
        app.send(Command::now(What::ToneOn {
            hz: args.hz,
            gain: args.gain,
        }))
        .expect("send");
        app.send(Command::at(
            SampleTime(frames.saturating_sub(2_400) as u64),
            What::ToneOff,
        ))
        .expect("send");
        offline.render(frames)
    };

    let first = render();
    let second = render();
    let identical = first
        .iter()
        .zip(&second)
        .all(|(a, b)| a.to_bits() == b.to_bits());

    match offline::write_wav(std::path::Path::new(path), &first, rate, 2) {
        Ok(()) => {
            let message = format!(
                "rendered {frames} frames to {path}; two renders {}",
                if identical {
                    "bit-identical"
                } else {
                    "DIFFER — this is a defect"
                }
            );
            log.info(creator::APP, message.clone());
            println!("{message}");
        }
        Err(error) => {
            log.error(creator::APP, format!("cannot write {path}: {error}"));
            eprintln!("rev-tone: cannot write {path}: {error}");
            std::process::exit(1);
        }
    }

    if !identical {
        std::process::exit(1);
    }
}
