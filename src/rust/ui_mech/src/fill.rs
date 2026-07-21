//! What a shape is filled with, and what casts a shadow.
//!
//! Deliberately two variants and one shadow kind. The exhibits use linear gradients
//! and blurred rounded-rect shadows; nothing else has a caller, and a primitive
//! designed without one is shaped by nobody. Adding a variant later is additive —
//! this is not a toolkit for external consumption, and completeness for its own sake
//! buys nothing.

use crate::geometry::Point;
use crate::paint::Color;

/// A fill: flat colour, or a linear gradient between two logical points.
#[derive(Debug, Clone, PartialEq)]
pub enum Fill {
    Solid(Color),
    Linear {
        from: Point,
        to: Point,
        /// Position in 0..=1 with its colour, in order. Two stops is the common
        /// case; the exhibits' slider caps use three.
        stop: Vec<(f32, Color)>,
    },
}

impl From<Color> for Fill {
    fn from(c: Color) -> Fill {
        Fill::Solid(c)
    }
}

impl Fill {
    /// A vertical gradient down a rectangle's height — the slider-cap idiom, and
    /// the reason this shortcut exists at all.
    pub fn vertical(top: f32, bottom: f32, stop: Vec<(f32, Color)>) -> Fill {
        Fill::Linear {
            from: Point::new(0.0, top),
            to: Point::new(0.0, bottom),
            stop,
        }
    }
}

/// A blurred shadow cast by a rounded rectangle — outer, or inset.
///
/// `sigma` is a blur radius in logical pixels, in the sense CSS uses: the visible
/// softness, not a Gaussian standard deviation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub offset: Point,
    pub sigma: f32,
    pub color: Color,
    /// Inside the shape (a recessed slot) rather than behind it (a raised panel).
    pub inset: bool,
}

impl Shadow {
    /// Cast behind the shape.
    pub fn outer(offset: Point, sigma: f32, color: Color) -> Shadow {
        Shadow {
            offset,
            sigma,
            color,
            inset: false,
        }
    }

    /// Cast inside the shape — the recessed groove a slider slot sits in.
    pub fn inset(offset: Point, sigma: f32, color: Color) -> Shadow {
        Shadow {
            offset,
            sigma,
            color,
            inset: true,
        }
    }
}

/// What the shadow path cost, so the question "is a cache worth it?" is answered by
/// measurement rather than by assumption.
///
/// Deliberately no cache yet. Build it, count it, look at the numbers, and add one
/// only if they justify it — the same doctrine as the perf ledger: track first.
#[derive(Debug, Clone, Default)]
pub struct PaintStat {
    /// Shadows drawn this frame.
    pub shadow: u32,
    /// Mask pixels blurred — the quantity that would actually be saved.
    pub blur_pixel: u64,
    pub blur_nanos: u64,
    /// Distinct shadow geometries this frame, in device pixels: a cache can only
    /// help to the extent this is smaller than `shadow`.
    geometry: Vec<(u32, u32, u32, u32)>,
}

impl PaintStat {
    pub fn distinct(&self) -> usize {
        self.geometry.len()
    }

    pub(crate) fn record(&mut self, key: (u32, u32, u32, u32), pixel: u64, nanos: u64) {
        self.shadow += 1;
        self.blur_pixel += pixel;
        self.blur_nanos += nanos;
        if !self.geometry.contains(&key) {
            self.geometry.push(key);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.shadow = 0;
        self.blur_pixel = 0;
        self.blur_nanos = 0;
        self.geometry.clear();
    }

    /// One line, for a debug overlay or the console.
    pub fn summary(&self) -> String {
        format!(
            "shadow {} ({} distinct) · {} px blurred · {:.2} ms",
            self.shadow,
            self.distinct(),
            self.blur_pixel,
            self.blur_nanos as f64 / 1.0e6
        )
    }
}
