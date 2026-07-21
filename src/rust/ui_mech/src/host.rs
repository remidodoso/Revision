//! What the mechanism asks of its client.
//!
//! The client is the widget kit in practice, `rev-app` in this step. The trait is
//! deliberately thin: the mechanism drives, the host answers. Input routing joins in
//! step 6; this is the window-lifecycle slice.

use crate::Mech;
use crate::a11y::Tree;
use crate::dirty::Dirty;
use crate::geometry::Size;
use crate::input::{Event, TargetId};
use crate::paint::Painter;
use crate::window::WindowId;

/// Something happened to a window. Not input — input arrives as `Event` in step 6.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// New logical size. Also fires after a scale change, since logical size moves.
    Resized(Size),
    /// The window moved to a display with a different DPI, or the user changed it.
    /// Logical geometry is unaffected by design; only rasterization density changes.
    ScaleChanged(f32),
    /// The user asked to close. The host decides — nothing closes itself.
    CloseRequested,
    /// Platform focus arrived or left. Distinct from widget focus, which the
    /// mechanism owns separately (step 6).
    FocusChanged(bool),
}

/// One frame's worth of drawing surface.
pub struct Frame<'a> {
    /// Logical size of the paintable area.
    pub size: Size,
    /// Physical pixels per logical pixel, for this window, right now. Present for
    /// the rare case that needs device alignment; ordinary drawing never reads it,
    /// because the painter has already applied it.
    pub scale: f32,
    /// The drawing vocabulary, in logical pixels. Already clipped to the dirty
    /// region's bounds, so painting outside it costs nothing but is also wasted.
    pub paint: Painter<'a>,
    /// What needs repainting. Widgets that can skip work cheaply should consult it
    /// (`dirty.touches(rect)`) rather than relying on the clip alone.
    pub dirty: &'a Dirty,
}

/// The mechanism's client.
///
/// Every method receives the `WindowId` it concerns: there is no ambient "current
/// window" (ui-01 invariant 3).
pub trait Host {
    /// Called once, before any window exists. Open windows here.
    fn start(&mut self, mech: &mut Mech);

    /// A window-lifecycle notice.
    fn notice(&mut self, window: WindowId, notice: &Notice, mech: &mut Mech);

    /// Paint a frame. Called only when the window is dirty and has pixels.
    fn paint(&mut self, window: WindowId, frame: &mut Frame<'_>);

    /// What is at this logical position? Hit testing lives above the mechanism,
    /// over the widget tree; the mechanism only routes what this returns.
    ///
    /// Defaulted so a host with no widgets yet still compiles.
    fn hit(&self, window: WindowId, at: crate::geometry::Point) -> Option<TargetId> {
        let _ = (window, at);
        None
    }

    /// Called once per loop iteration, before the loop sleeps. Where time-driven
    /// appearance advances — a flashing control, a meter, a playhead.
    ///
    /// Defaulted to nothing, so an application with no animation pays nothing and
    /// the loop keeps sleeping until input arrives.
    fn tick(&mut self, mech: &mut Mech) {
        let _ = mech;
    }

    /// This window's accessibility tree (R-1510).
    ///
    /// Defaulted to empty so a host may ignore it — but the method exists from the
    /// first day, so a widget that cannot name itself, state its value, or report
    /// its bounds is visibly deficient rather than quietly so. The platform bridge
    /// is deferred; the shape is not.
    fn a11y(&self, window: WindowId) -> Tree {
        let _ = window;
        Tree::default()
    }

    /// An input event, already routed: `target` is the captured widget during a
    /// drag, the hovered one for a wheel, the focused one for a key.
    fn event(&mut self, window: WindowId, target: Option<TargetId>, ev: &Event, mech: &mut Mech) {
        let _ = (window, target, ev, mech);
    }
}
