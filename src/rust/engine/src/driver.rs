//! Drivers: the two things that call [`Engine::process`](crate::Engine::process).
//!
//! The engine core knows nothing about cpal, WASAPI or files. That is not extra
//! work, it is less: the offline renderer eng-07 needs for its bit-identity gate
//! (R-1402) comes free rather than as a parallel implementation, and every
//! engine test runs headless in CI on a machine with no sound card — which is
//! the difference between an engine that is tested and one that is auditioned.

pub mod device;
pub mod offline;

pub use device::{Device, OpenReport, Request};
pub use offline::Offline;
