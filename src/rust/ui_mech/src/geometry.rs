//! Geometry in **logical pixels**.
//!
//! Device pixels exist only inside the backend (`window`), so DPI is handled in
//! exactly one place — ui-01 invariant 4. Nothing above this crate multiplies by a
//! scale factor, and nothing stores one.

/// A point in logical pixels, relative to whatever the context says.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// A size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

/// A rectangle in logical pixels: origin plus extent, never min/max pairs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }
}

impl Size {
    pub fn new(w: f32, h: f32) -> Size {
        Size { w, h }
    }

    /// True when the size encloses no pixels — a minimized window, chiefly.
    pub fn empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn empty(&self) -> bool {
        self.size().empty()
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// The smallest rectangle containing both. An empty operand contributes
    /// nothing, so unioning into `Rect::default()` accumulates correctly.
    pub fn union(&self, other: Rect) -> Rect {
        if self.empty() {
            return other;
        }
        if other.empty() {
            return *self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Rect::new(x, y, right - x, bottom - y)
    }

    /// The overlap, or an empty rect at the origin when they do not overlap.
    pub fn intersect(&self, other: Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= x || bottom <= y {
            return Rect::default();
        }
        Rect::new(x, y, right - x, bottom - y)
    }
}

#[cfg(test)]
mod test;
