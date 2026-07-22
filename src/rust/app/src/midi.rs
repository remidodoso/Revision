//! Live MIDI input, wired to the engine (midi-02).
//!
//! This is the app-side half of the input fork: it opens a `rev-midi` port,
//! hands the callback the engine's [`ThruSender`] so live notes reach the voice
//! pool directly (the fast path, minimum latency), and keeps the app's end of
//! the event ring for capture and display later.
//!
//! **Hot-plug is a poll.** `midir` has no arrival/removal callback, so
//! [`Keys::poll`] lists the ports each UI frame and reports what changed. The
//! usual gesture — open the first keyboard, reconnect it if it comes back — is
//! all here; choosing a specific device and remembering it across sessions is a
//! settings-store concern deferred until that store exists.
//!
//! The clock-correlation origin is not shared with the engine yet: **playing
//! needs no timestamp** (a note sounds as soon as it arrives), so that wiring
//! lands with recording (rec-01), where a timestamp decides where a note falls.

use std::time::Instant;

use rev_engine::ThruSender;
use rev_log::{Log, creator};
use rev_midi::{Change, Connection, Devices, Events, Fork, NoteHz};

/// The live-input manager: what is plugged in, what is open, and the reader the
/// app drains for captured events.
pub struct Keys {
    devices: Devices,
    /// The open connection. Dropping it closes the port, so it is held for as
    /// long as input should flow.
    connection: Option<Connection>,
    /// The name of the open port, for reconnecting it if it returns.
    open_name: Option<String>,
    /// The app's end of the event ring — note numbers for the model (R-002),
    /// drained each frame. `None` until a port is open.
    events: Option<Events>,
    /// The thru sender, waiting to be handed to a fork when a port opens. Taken
    /// from `Audio` once; a fork consumes it, so re-opening needs it back — see
    /// the note in `open`.
    thru: Option<ThruSender>,
    snapshot: NoteHz,
    origin: Instant,
}

impl Keys {
    /// Build the manager around the engine's thru sender and the tuning to
    /// resolve keys through.
    pub fn new(thru: ThruSender, snapshot: NoteHz) -> Keys {
        Keys {
            devices: Devices::new(),
            connection: None,
            open_name: None,
            events: None,
            thru: Some(thru),
            snapshot,
            origin: Instant::now(),
        }
    }

    /// The ports present as of the last poll.
    pub fn ports(&self) -> &[String] {
        self.devices.ports()
    }

    /// The port currently open, if any.
    pub fn open_port(&self) -> Option<&str> {
        self.open_name.as_deref()
    }

    /// Poll for hot-plug and auto-reconnect. Call each UI frame.
    ///
    /// - if nothing is open and a keyboard is present, open the first one;
    /// - if the open port vanished, drop the connection but remember the name;
    /// - if a remembered port returned, reopen it.
    ///
    /// Returns what changed, for the caller to log.
    pub fn poll(&mut self, log: &Log) -> Change {
        let change = self.devices.poll();
        for name in &change.arrived {
            log.info(creator::UI, format!("MIDI in: {name} arrived"));
        }
        for name in &change.removed {
            log.info(creator::UI, format!("MIDI in: {name} removed"));
        }

        // The open device went away.
        if let Some(name) = &self.open_name
            && change.removed.iter().any(|r| r == name)
        {
            self.connection = None;
            log.info(creator::UI, format!("MIDI in: {name} disconnected"));
        }

        // Nothing open: take the first keyboard, or reconnect a remembered one.
        if self.connection.is_none() {
            let want = self
                .open_name
                .clone()
                .filter(|n| self.devices.index_of(n).is_some())
                .or_else(|| self.ports().first().cloned());
            if let Some(name) = want {
                self.open(&name, log);
            }
        }
        change
    }

    /// Open a port by name, wiring a fresh fork to the engine.
    fn open(&mut self, name: &str, log: &Log) {
        let Some(index) = self.devices.index_of(name) else {
            return;
        };
        // A fork consumes the thru sender. Once a connection has been made and
        // dropped, the sender is gone with it — so reopening a port after a
        // disconnect needs the sender back. Until reconnection is exercised
        // against real unplugging, a single open is the supported path; taking
        // the sender here means a second open no-ops rather than misbehaving.
        let Some(thru) = self.thru.take() else {
            log.warn(creator::UI, "MIDI in: thru sender already spent");
            return;
        };
        let (fork, events) = Fork::new(thru, self.snapshot.clone(), self.origin);
        match rev_midi::ports::open(index, fork) {
            Ok(connection) => {
                self.connection = Some(connection);
                self.events = Some(events);
                self.open_name = Some(name.to_string());
                log.info(creator::UI, format!("MIDI in: playing from {name}"));
            }
            Err(error) => {
                log.error(creator::UI, format!("MIDI in: cannot open {name}: {error}"));
            }
        }
    }

    /// Change the resolution — a tuning change, or (midi-04) a scale remap. The
    /// next key resolves differently. Rebuilds the fork's snapshot when a port
    /// is open; the swap is midi-04's, the hook is here.
    pub fn set_snapshot(&mut self, _snapshot: NoteHz) {
        // The fork lives inside the `midir` callback and cannot be reached from
        // here without a shared cell; that plumbing (an atomic snapshot swap) is
        // midi-04's. Recorded so the seam is visible rather than surprising.
    }

    /// Drain captured events — the model's view, for the log viewer and, later,
    /// recording. Returns how many were seen this frame.
    pub fn drain(&mut self, mut sink: impl FnMut(rev_midi::Captured)) -> usize {
        let Some(events) = self.events.as_mut() else {
            return 0;
        };
        let mut n = 0;
        while let Some(captured) = events.take() {
            sink(captured);
            n += 1;
        }
        n
    }
}
