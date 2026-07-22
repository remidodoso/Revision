//! rev-engine — the real-time audio engine.
//!
//! **Below this crate's boundary there is only physics** (R-312): samples,
//! frequencies, channels, gains, and opaque handles. Note numbers, ticks,
//! tunings, tempo and phrases are resolved above it, by the schedule compiler —
//! the compiler is the last place music exists. That is what makes
//! tuning-awareness structural rather than a discipline anyone could forget: a
//! voice that only ever receives Hz *cannot* assume 12-ET.
//!
//! It is also why this crate depends on `cpal` and `rtrb` and nothing else —
//! not `rev-core` (there is nothing in a model of music for it to import), not
//! `rev-store` (it consumes compiled chunks and never reads a database), and not
//! `rev-log` (its records are POD, formatted app-side, which keeps bundled
//! SQLite out of the audio engine's dependency tree).
//!
//! The real-time callback is allocation-free by law: pre-allocated state,
//! lock-free rings in and out, garbage shipped home over a ring, and an
//! allocation guard in debug builds. Live paths add no buffering beyond the
//! device's own (R-304, R-1501); the budget is 10 ms round trip (R-311).
//!
//! Approved at eng-01; see `doc/completed/revision_eng01_proposal.md`.

pub mod automation;
pub mod command;
pub mod driver;
pub mod engine;
pub mod error;
pub mod format;
pub mod graph;
pub mod guard;
pub mod instrument;
pub mod live;
pub mod obs;
pub mod param;
pub mod port;
pub mod position;
pub mod table;
pub mod time;
mod tone;
pub mod voice;

pub use automation::{Automation, Curve};
pub use command::{Chunk, ChunkHandle, Command, Garbage, Note, What};
pub use engine::Engine;
pub use error::EngineError;
pub use format::{Block, Format, Planar, PlanarMut};
pub use graph::{
    BiquadMode, BuildError, Graph, GraphSpec, NodeKind, NodeRef, NodeSpec, ParamId, QUANTUM,
};
pub use instrument::{Instrument, Patch};
pub use live::{Live, LiveKey};
pub use obs::{Code, Creator, Level, Obs};
pub use port::{EngineSession, Refused, RtPort, ThruSender, session, session_with_thru};
pub use position::Position;
pub use table::{Table, TableId, TableSet};
pub use time::{NOW, SampleTime};
pub use voice::{Span, VoicePool, VoiceState};
