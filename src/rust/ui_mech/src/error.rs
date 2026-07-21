//! Failures the mechanism can report.
//!
//! Platform errors are wrapped rather than surfaced raw: `rev-ui-kit` must not be
//! able to name a winit or softbuffer type (ui-01 §6), and that includes error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MechError {
    #[error("event loop: {0}")]
    EventLoop(String),
    #[error("window creation: {0}")]
    Window(String),
    #[error("surface: {0}")]
    Surface(String),
}

impl From<winit::error::EventLoopError> for MechError {
    fn from(e: winit::error::EventLoopError) -> MechError {
        MechError::EventLoop(e.to_string())
    }
}

impl From<winit::error::OsError> for MechError {
    fn from(e: winit::error::OsError) -> MechError {
        MechError::Window(e.to_string())
    }
}

impl From<softbuffer::SoftBufferError> for MechError {
    fn from(e: softbuffer::SoftBufferError) -> MechError {
        MechError::Surface(e.to_string())
    }
}
