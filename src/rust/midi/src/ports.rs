//! Opening a `midir` input port — just enough to prove the types against the
//! library (midi-01 §1).
//!
//! **Not the enumeration story.** Runtime discovery, hot-plug arrival and
//! removal (R-601), and persistent device identity (R-602) are midi-02. What is
//! here is: list the ports that exist right now, and connect one, wiring
//! [`Fork::on_message`] as the callback. That is the smallest thing that shows
//! the whole shape works end to end against real MIDI — and it needs a device
//! only to *run*, not to compile or to test the pieces.

use midir::{Ignore, MidiInput, MidiInputConnection};

use crate::input::Fork;

/// A held connection. Dropping it closes the port, so the caller must keep it
/// alive for as long as input should flow — the callback stops the instant this
/// is dropped.
pub struct Connection {
    _conn: MidiInputConnection<Fork>,
}

/// The input ports that exist right now, by name. A snapshot, not a
/// subscription — hot-plug is midi-02.
pub fn list() -> Vec<String> {
    let Ok(input) = MidiInput::new("revision-list") else {
        return Vec::new();
    };
    input
        .ports()
        .iter()
        .filter_map(|port| input.port_name(port).ok())
        .collect()
}

/// Open the port at `index` and start delivering messages into `fork`.
///
/// The `midir` timestamp is ignored on purpose: it is a different clock on every
/// platform, so the fork re-stamps in the engine's domain (§4). The `Fork` moves
/// into the callback's data, which is why the connection owns it.
pub fn open(index: usize, fork: Fork) -> Result<Connection, OpenError> {
    let mut input = MidiInput::new("revision").map_err(|_| OpenError::Unavailable)?;
    // Clock, active-sensing and sysex are noise for note input; drop them at the
    // source so the callback never sees them.
    input.ignore(Ignore::All);

    let ports = input.ports();
    let port = ports.get(index).ok_or(OpenError::NoSuchPort)?.clone();
    let name = input
        .port_name(&port)
        .unwrap_or_else(|_| "midi".to_string());

    let conn = input
        .connect(
            &port,
            &name,
            move |_timestamp, bytes, fork: &mut Fork| fork.on_message(bytes),
            fork,
        )
        .map_err(|_| OpenError::Refused)?;
    Ok(Connection { _conn: conn })
}

/// Why a port could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// No MIDI stack on this machine.
    Unavailable,
    /// The index is past the end of the port list.
    NoSuchPort,
    /// The OS refused the connection.
    Refused,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OpenError::Unavailable => "no MIDI stack available",
            OpenError::NoSuchPort => "no such MIDI port",
            OpenError::Refused => "the MIDI port refused the connection",
        })
    }
}

impl std::error::Error for OpenError {}
