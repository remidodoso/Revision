//! rev-midi — MIDI I/O: `midir` wrapper with hot-plug enumeration (R-601),
//! driver-boundary timestamps and clock-domain correlation to the engine's
//! sample clock (R-603), and the thru fast path (R-605). Input forks at
//! birth: fast path → engine (live), event path → app (capture/journal) —
//! the live/playback classification exists from the first build.
//!
//! **midi-01 builds the shapes.** The [`Fork`] and its two rings, the clock
//! [`Correlation`], the [`NoteHz`] resolution, and the parsed [`Message`] — all
//! testable without a device — plus a real `midir` open ([`ports`]) proving the
//! types against the library. Runtime enumeration and hot-plug are midi-02;
//! live playthrough and the honest latency print are midi-03.

pub mod correlate;
pub mod devices;
pub mod event;
pub mod input;
pub mod ports;
pub mod snapshot;

pub use correlate::{Correlation, Pair};
pub use devices::{Change, Devices};
pub use event::{Captured, Message};
pub use input::{Events, Fork};
pub use ports::Connection;
pub use snapshot::NoteHz;
