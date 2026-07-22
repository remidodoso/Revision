//! The app side of the engine seam.
//!
//! Everything here runs on the app thread, where allocating is allowed. Its job
//! is the three things the real-time side deliberately cannot do:
//!
//! - **turn observations into prose** and hand them to the log (eng-01 §9.1);
//! - **free what the engine finished with**, over the return ring (§4.4);
//! - **hold the device**, so that exactly one session is audible at a time.
//!
//! Running without a device is degraded, never fatal: the session still exists,
//! commands still succeed, and the failure is recorded rather than thrown.

use rev_engine::driver::{Device, Request};
use rev_engine::{Command, EngineSession, Level, Position, ThruSender, What, session_with_thru};
use rev_log::{Log, creator};

pub struct Audio {
    session: EngineSession,
    /// `None` when no device could be opened. The application still runs.
    device: Option<Device>,
    /// The MIDI thread's end of the thru ring, handed to a `rev-midi` fork when
    /// live input is opened. `take_thru` moves it out, once (midi-02).
    thru: Option<ThruSender>,
    log: Log,
    sample_rate: u32,
    /// Commands the ring refused. Counted rather than swallowed: a command is
    /// intent, and losing one is a real event (eng-01 §4.1).
    refused: u64,
}

impl Audio {
    /// Open a stream and start it. **The stream opens once and never stops** —
    /// silence is written while the transport is stopped (eng-01 §11.5).
    pub fn open(log: Log, request: &Request) -> Audio {
        Audio::open_with(log, request, |_| None)
    }

    /// Open with an instrument, built once the device's format is known.
    pub fn open_with(
        log: Log,
        request: &Request,
        instrument: impl FnOnce(rev_engine::Format) -> Option<rev_engine::Instrument>,
    ) -> Audio {
        let (app, rt, thru) = session_with_thru();
        match Device::open_with(request, rt, instrument) {
            Ok(device) => {
                let report = device.report().clone();
                log.info(creator::ENGINE_STREAM, report.summary());
                if !report.duplex {
                    // Recorded, not silent: R-301 wants duplex, and this device
                    // has no input to give (eng-01 §11.3).
                    log.info(
                        creator::ENGINE_STREAM,
                        "output only: the selected device offers no input",
                    );
                }
                Audio {
                    session: app,
                    sample_rate: report.format.sample_rate,
                    device: Some(device),
                    thru: Some(thru),
                    log,
                    refused: 0,
                }
            }
            Err(error) => {
                log.error(creator::ENGINE_STREAM, format!("no audio: {error}"));
                let available = Device::list();
                if available.is_empty() {
                    log.info(creator::ENGINE_STREAM, "no output devices at all");
                } else {
                    log.info(
                        creator::ENGINE_STREAM,
                        format!("devices available: {}", available.join(", ")),
                    );
                }
                Audio {
                    session: app,
                    device: None,
                    thru: Some(thru),
                    log,
                    sample_rate: 48_000,
                    refused: 0,
                }
            }
        }
    }

    pub fn is_audible(&self) -> bool {
        self.device.is_some()
    }

    /// Take the thru sender — the MIDI thread's end of the ring the engine
    /// drains. Moved out once, into a `rev-midi` fork, when live input opens.
    pub fn take_thru(&mut self) -> Option<ThruSender> {
        self.thru.take()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The clock origin the engine measures its correlation pairs from. Hand it
    /// to `Keys::new` so live input is stamped against the same zero the engine
    /// publishes — the shared origin recording needs (rec-01 §3).
    pub fn origin(&self) -> std::time::Instant {
        self.session.origin()
    }

    pub fn log(&self) -> &Log {
        &self.log
    }

    /// Send at the next block boundary — the live path.
    pub fn send(&mut self, what: What) {
        self.send_command(Command::now(what));
    }

    pub fn send_command(&mut self, command: Command) {
        if let Err(refused) = self.session.send(command) {
            self.refused += 1;
            self.log.warn(
                creator::ENGINE,
                format!(
                    "command refused, the ring is full: {:?} ({} so far)",
                    refused.0.what, self.refused
                ),
            );
        }
    }

    pub fn position(&self) -> Position {
        self.session.position()
    }

    /// Drain the engine and free what it returned. Call every UI frame.
    ///
    /// This is the whole app-side half of the seam: formatting happens here,
    /// where it is allowed to allocate, and the only place engine-side
    /// allocations are freed is `collect`.
    pub fn pump(&mut self) {
        let rate = self.sample_rate;
        let log = self.log.clone();
        self.session.drain_obs(|obs| {
            log.record_engine(obs.creator.as_str(), level_of(obs.level), obs.render(rate));
        });
        self.session.collect();
    }
}

fn level_of(level: Level) -> rev_log::Level {
    match level {
        Level::Trace => rev_log::Level::Trace,
        Level::Info => rev_log::Level::Info,
        Level::Warn => rev_log::Level::Warn,
        Level::Error => rev_log::Level::Error,
    }
}

/// The engine's creator names are `&'static str` constants on its side too, so
/// they cross without allocating a `String` for the creator column.
trait EngineRecord {
    fn record_engine(&self, creator: &str, level: rev_log::Level, text: String);
}

impl EngineRecord for Log {
    fn record_engine(&self, creator: &str, level: rev_log::Level, text: String) {
        // The engine's creators are a closed set, so they map onto the log's
        // constants rather than allocating a name per record.
        let creator = match creator {
            "engine.stream" => creator::ENGINE_STREAM,
            "engine.transport" => creator::ENGINE_TRANSPORT,
            "engine.sched" => creator::ENGINE_SCHED,
            "engine.timing" => creator::ENGINE_TIMING,
            _ => creator::ENGINE,
        };
        match level {
            rev_log::Level::Trace => self.trace(creator, text),
            rev_log::Level::Info => self.info(creator, text),
            rev_log::Level::Warn => self.warn(creator, text),
            rev_log::Level::Error => self.error(creator, text),
        }
    }
}
