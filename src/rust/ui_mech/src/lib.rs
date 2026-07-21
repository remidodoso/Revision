//! `rev-ui-mech` — the UI mechanism layer: window/surface (winit + softbuffer),
//! CPU rasterization (tiny-skia), text stack (cosmic-text), input routing per the
//! mechanism contract — implicit pointer capture, activation ≠ focus,
//! keyboard-vs-text channels, and the hands-off clause (R-907): never move the
//! pointer, never steal focus, never scroll unbidden. Mechanism only; widget style
//! and identity belong above. No native handle leaks into the kit-facing API; no
//! Win32 outside this crate.
//!
//! Approved as ui-01 (`doc/completed/revision_ui01_proposal.md`). Two properties
//! define the crate:
//!
//! - **Every external UI dependency lives here.** `rev-ui-kit` depends on this crate
//!   and nothing else, so it cannot name a native handle, let alone reach one — and
//!   a browser backend later replaces exactly this crate.
//! - **Windows are keyed, never singular.** Scale factor, size, dirtiness, and
//!   surface are per-window state reached through a [`WindowId`]; the application
//!   opens one window today and nothing here knows that.
//!
//! Built in the order of ui-02's plan. Present: windows, surfaces, the frame loop,
//! the paint list, dirty regions, text, input, interface scale (R-938).

mod a11y;
mod blur;
mod canvas;
mod dirty;
mod error;
mod fill;
mod geometry;
mod host;
mod input;
mod mech;
mod paint;
mod text;
mod window;

pub use a11y::{Node, Role, Tree};
pub use canvas::Canvas;
pub use dirty::Dirty;
pub use error::MechError;
pub use fill::{Fill, PaintStat, Shadow};
pub use geometry::{Point, Rect, Size};
pub use host::{Frame, Host, Notice};
pub use input::{
    Button, CursorShape, Event, Key, KeyCode, Modifier, Named, Pointer, PointerKind, Reason,
    TargetId, Text, UiTime,
};
pub use mech::{Mech, run};
pub use paint::{Color, Outline, Painter, Path};
pub use text::{FontRole, Shaped, TextStyle};
pub use window::{WindowId, WindowRole, WindowSpec};
