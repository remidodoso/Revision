//! Keeping the playhead in view — and getting out of the way when the user
//! takes over (ui-06 §8).
//!
//! The rule is ch. 1's first principle applied to a conflict between the two
//! parties: *"Allow the user, not the computer, to initiate and control
//! actions."* Following is the computer initiating; scrolling is the user. When
//! they disagree the user wins, permanently, until they say otherwise.
//!
//! **The transport never animates the view** (R-947). A following view is
//! stationary and jumps; the only thing moving between frames is the playhead.

use rev_ui_kit::pane::{Axis, Pane};
use rev_ui_mech::Rect;

/// Where across the viewport the playhead is when the view jumps.
///
/// Its real job is that the playhead never reaches the edge: at 1.0 you would
/// watch it hit the wall and then stumble. A user preference in waiting — the
/// settings system's second known customer, after R-944's display origin.
pub const FOLLOW_TRIGGER: f32 = 0.80;

/// Where the playhead lands afterwards. Its job is history: half a viewport of
/// what was just played stays visible.
pub const FOLLOW_LAND: f32 = 0.50;

/// The two are a **constrained pair, not two sliders**. If `land >= trigger` the
/// playhead lands on or past the trigger and fires again immediately, and the
/// view thrashes forever. Enforced where they are set, not discovered during
/// playback.
pub const MIN_SEPARATION: f32 = 0.15;

/// Whether a trigger/land pair can be used at all.
pub fn is_usable(trigger: f32, land: f32) -> bool {
    (0.0..=1.0).contains(&trigger)
        && (0.0..=1.0).contains(&land)
        && trigger - land >= MIN_SEPARATION
}

/// Following, and why it might not be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Follow {
    armed: bool,
    trigger: f32,
    land: f32,
}

impl Default for Follow {
    fn default() -> Follow {
        Follow {
            armed: true,
            trigger: FOLLOW_TRIGGER,
            land: FOLLOW_LAND,
        }
    }
}

impl Follow {
    pub fn armed(&self) -> bool {
        self.armed
    }

    /// Set the pair. Refused as a pair when they are too close, because a
    /// refusal is the only thing that prevents a repeating jump.
    pub fn set(&mut self, trigger: f32, land: f32) -> bool {
        if !is_usable(trigger, land) {
            return false;
        }
        self.trigger = trigger;
        self.land = land;
        true
    }

    /// The explicit control.
    pub fn set_armed(&mut self, armed: bool) {
        self.armed = armed;
    }

    /// The user scrolled. **Only a horizontal scroll takes over**: follow
    /// governs time, and looking at a high note makes no claim about where you
    /// are in the piece. (The book is adjacent — "if you can scroll in one
    /// orientation to reveal the selection, don't scroll in both".)
    ///
    /// What makes "the user" answerable at all is that **the kit emits an
    /// `Intent` only from input**: the roll moves the pane by mutating it, so
    /// anything arriving as `Intent::Scrolled` is by construction an act. The
    /// same shape as `Reason::{User, Programmatic}` for focus, and no third
    /// mechanism invented for it.
    pub fn user_scrolled(&mut self, axis: Axis) {
        if axis == Axis::Horizontal {
            self.armed = false;
        }
    }

    /// An explicit locate — return-to-zero, a locator recall, any "take me
    /// there". **Not** plain Play, which must not yank the view out from under
    /// someone who is looking at something; and **not** Stop, which is often
    /// pressed precisely in order to go and look.
    pub fn located(&mut self) {
        self.armed = true;
    }

    /// While armed, zoom anchors the playhead rather than the pointer — so
    /// zooming does not fight following, and does not disarm it either. Follow
    /// changes what stays still.
    pub fn zoom_anchor(&self, rect: Rect, pane: &Pane, playhead: Option<f64>) -> Option<f32> {
        if !self.armed {
            return None;
        }
        let beat = playhead?;
        let inner = pane.interior(rect);
        Some(inner.x + ((beat - f64::from(pane.offset.x)) / f64::from(pane.scale.x)) as f32)
    }

    /// Move the view if the playhead has passed the trigger. Returns the new
    /// offset when it jumped, and `None` when it did not — which is most calls,
    /// and is the point.
    pub fn advance(&self, rect: Rect, pane: &mut Pane, playhead: f64) -> Option<f32> {
        if !self.armed {
            return None;
        }
        let viewport = f64::from(pane.viewport(rect, Axis::Horizontal));
        if viewport <= 0.0 {
            return None;
        }
        let across = (playhead - f64::from(pane.offset.x)) / viewport;
        // Behind the view as well as ahead of it: locating backwards should
        // bring the playhead back into sight, not leave it off to the left.
        if across >= f64::from(self.trigger) || across < 0.0 {
            let want = playhead - f64::from(self.land) * viewport;
            pane.offset.x = want.max(0.0) as f32;
            // Near the end of the piece the content cannot scroll far enough to
            // put the playhead at `land`, so this clamps and the playhead simply
            // runs to the right within the final view. Correct, not a defect.
            pane.clamp(rect);
            return Some(pane.offset.x);
        }
        None
    }
}

#[cfg(test)]
mod test;
