//! Dirty regions: what needs repainting, per window.
//!
//! In the v0 design from the start (ui-01 §2) rather than retrofitted, because the
//! difference is between a renderer that scales with *content* and one that scales
//! with the user's monitor — and a widget kit that never expected dirty tracking is
//! unpleasant to teach it later.
//!
//! A region is a short list of logical-pixel rectangles. Past a cap it collapses to
//! its bounds: many small rectangles cost more to reason about than they save, and
//! the pathological cases (a full redraw arriving as two hundred fragments) are
//! exactly where the bookkeeping would hurt most.

use crate::geometry::Rect;

/// Beyond this many rectangles, the region collapses to its bounding box.
const CAP: usize = 8;

/// The parts of a window that need repainting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dirty {
    rect: Vec<Rect>,
}

impl Dirty {
    /// Nothing to repaint. A window in this state must not be painted at all.
    pub fn empty(&self) -> bool {
        self.rect.is_empty()
    }

    /// The rectangles, in the order they were added. Widgets that can skip work
    /// cheaply should ask [`Dirty::touches`] instead of walking these.
    pub fn rect(&self) -> &[Rect] {
        &self.rect
    }

    /// The smallest rectangle covering everything dirty; empty when nothing is.
    pub fn bound(&self) -> Rect {
        self.rect
            .iter()
            .fold(Rect::default(), |acc, r| acc.union(*r))
    }

    /// Does `r` overlap anything that needs repainting? The question a widget asks
    /// before deciding whether to draw itself or its children.
    pub fn touches(&self, r: Rect) -> bool {
        self.rect.iter().any(|d| !d.intersect(r).empty())
    }

    /// Add a rectangle. Empty rectangles are ignored, rectangles already covered
    /// are dropped, and a rectangle covering an existing one replaces it — cheap
    /// checks that keep the common case (repeated marks of the same widget) at one
    /// entry rather than `CAP`.
    pub(crate) fn add(&mut self, r: Rect) {
        if r.empty() {
            return;
        }
        if self.rect.iter().any(|d| covers(*d, r)) {
            return;
        }
        self.rect.retain(|d| !covers(r, *d));
        self.rect.push(r);
        if self.rect.len() > CAP {
            let bound = self.bound();
            self.rect.clear();
            self.rect.push(bound);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.rect.clear();
    }
}

/// Does `outer` fully contain `inner`?
fn covers(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.right() >= inner.right()
        && outer.bottom() >= inner.bottom()
}

#[cfg(test)]
mod test;
