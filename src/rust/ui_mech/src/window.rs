//! Windows and their CPU surfaces.
//!
//! Windows are **keyed, never singular** (ui-01 invariant 3): every window-facing
//! call takes a [`WindowId`], and per-window state — scale factor, size, surface —
//! lives in the map rather than in a field. The application opens one window today;
//! nothing here knows that.

use std::num::NonZeroU32;
use std::sync::Arc;

use crate::geometry::{Rect, Size};

/// Opaque handle to an open window. Stable for the window's lifetime; never reused
/// within a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub(crate) u32);

/// Platform DPI composed with interface scale (R-938). Free function so the rule
/// can be tested without a platform window: the two multiply, and a per-window
/// override replaces the workspace default rather than compounding with it.
pub(crate) fn compose_scale(platform: f32, window: Option<f32>, default_user: f32) -> f32 {
    platform * window.unwrap_or(default_user)
}

/// What a window is *for*. The role decides how the platform treats it — a palette
/// floats above its owner and never takes activation, which is the Control Bar's
/// inherited "always active, never comes to front" behavior expressed as a property
/// of the window rather than as a widget hack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowRole {
    /// An ordinary window: takes focus, appears in the taskbar.
    #[default]
    Document,
    /// A floating palette: never activates, stays above its owner.
    Palette,
}

/// How to open a window. Sizes are logical pixels; the platform applies DPI.
#[derive(Debug, Clone)]
pub struct WindowSpec {
    pub title: String,
    pub size: Size,
    pub role: WindowRole,
    pub resizable: bool,
    /// Interface scale for this window (R-938), overriding the workspace default.
    /// Set here rather than afterwards: a window is created asynchronously, so a
    /// scale applied to its id before it exists would be silently dropped.
    pub scale: Option<f32>,
}

impl Default for WindowSpec {
    fn default() -> WindowSpec {
        WindowSpec {
            title: String::from("Revision"),
            size: Size::new(960.0, 600.0),
            role: WindowRole::Document,
            resizable: true,
            scale: None,
        }
    }
}

/// One open window: the platform window, its CPU surface, and the DPI state that
/// must never leak upward.
pub(crate) struct Window {
    pub(crate) winit: Arc<winit::window::Window>,
    pub(crate) surface: softbuffer::Surface<Arc<winit::window::Window>, Arc<winit::window::Window>>,
    /// Physical pixels per logical pixel. **Mutable for the window's whole life** —
    /// dragging between monitors changes it (ui-01 invariant 4), so it is read from
    /// here at every use and stored nowhere else.
    pub(crate) scale: f32,
    /// Physical size, straight from the platform.
    pub(crate) physical: (u32, u32),
    /// User interface scale for this window (R-938), composed with the platform's
    /// DPI rather than replacing it. `None` means "use the workspace default".
    pub(crate) user_scale: Option<f32>,
    /// What needs repainting; cleared when a frame is presented. A window whose
    /// region is empty is never painted.
    pub(crate) dirty: crate::dirty::Dirty,
    /// Where painting happens: premultiplied RGBA8 at device resolution. Kept
    /// alongside the surface rather than allocated per frame, because a resize is
    /// rare and a frame is not.
    pub(crate) pixmap: Option<tiny_skia::Pixmap>,
}

impl Window {
    /// Platform DPI composed with the interface scale — the number the painter
    /// applies, and the only place the two are combined.
    pub(crate) fn effective_scale(&self, default_user: f32) -> f32 {
        compose_scale(self.scale, self.user_scale, default_user)
    }

    /// The whole window as a logical rectangle — what "everything is dirty" means.
    pub(crate) fn bounds(&self, default_user: f32) -> Rect {
        let s = self.logical(default_user);
        Rect::new(0.0, 0.0, s.w, s.h)
    }

    /// Logical size, derived from physical rather than remembered — one source of
    /// truth, so a scale change cannot leave the two disagreeing.
    pub(crate) fn logical(&self, default_user: f32) -> Size {
        let s = self.effective_scale(default_user);
        Size::new(self.physical.0 as f32 / s, self.physical.1 as f32 / s)
    }

    /// Resize the CPU surface to the current physical size. Returns false when the
    /// window has no pixels (minimized), which is not an error and not paintable.
    pub(crate) fn resize_surface(&mut self) -> Result<bool, crate::error::MechError> {
        let (Some(w), Some(h)) = (
            NonZeroU32::new(self.physical.0),
            NonZeroU32::new(self.physical.1),
        ) else {
            return Ok(false);
        };
        self.surface.resize(w, h)?;
        // Reallocate the paint buffer only when the size actually changed; winit
        // reports Resized generously.
        let matches = self
            .pixmap
            .as_ref()
            .is_some_and(|p| p.width() == w.get() && p.height() == h.get());
        if !matches {
            self.pixmap = tiny_skia::Pixmap::new(w.get(), h.get());
        }
        Ok(true)
    }
}

#[cfg(test)]
mod test;
