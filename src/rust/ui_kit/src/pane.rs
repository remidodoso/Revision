//! The scrollable pane's geometry, as arithmetic.
//!
//! Everything the kit needs to lay out a pane, place its bars, size its thumb
//! and convert between spaces — with no painting, no input, and no widget tree,
//! so it can be tested by asserting numbers (ui-07 §4, §5).
//!
//! **Three coordinate spaces**, named because confusing them is the classic
//! source of off-by-a-scroll-offset bugs:
//!
//! - **content** — what the application thinks in: ticks and hertz for the roll,
//!   lines for the log.
//! - **pane** — logical pixels inside the viewport, origin at its top-left.
//! - **window** — what the mechanism layer uses.
//!
//! **Offsets are in content units.** A zoom must not move the view, and it
//! cannot if the offset lives in the space zoom does not change.

use rev_ui_mech::{Point, Rect, Size};

/// Which of the two axes. Named rather than indexed: `bar[0]` reads as nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Which bars a pane reserves space for — **whether or not they are needed**.
///
/// A bar that appears on demand narrows the content; narrower content can be
/// taller; taller content can demand the other bar, which narrows it again. At
/// one particular content size that loop does not settle. Reserving makes it
/// unreachable, and makes layout a pure function of window size and extent
/// (ui-07 §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarPolicy {
    #[default]
    Both,
    /// The log window: text wraps, so only one axis ever scrolls.
    Vertical,
    Horizontal,
    None,
}

impl BarPolicy {
    pub fn has(self, axis: Axis) -> bool {
        matches!(
            (self, axis),
            (BarPolicy::Both, _)
                | (BarPolicy::Vertical, Axis::Vertical)
                | (BarPolicy::Horizontal, Axis::Horizontal)
        )
    }
}

/// Content units per logical pixel, per axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub x: f32,
    pub y: f32,
}

impl Default for Scale {
    fn default() -> Scale {
        Scale { x: 1.0, y: 1.0 }
    }
}

impl Scale {
    pub fn on(self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.x,
            Axis::Vertical => self.y,
        }
    }
}

/// How wide a scroll bar is by default.
///
/// **Wider than current convention, deliberately** — the same decision the skin
/// inventory already took for type, where the scale is "larger than control-skin
/// convention" because the target display is a 50-inch panel at desk distance.
/// Bars shrank because phone conventions leaked to the desktop, not because
/// anyone measured a desk. A scroll bar is a long thin target and the thin
/// dimension is the one that costs; widening it is the cheapest acquisition
/// improvement there is, in the direction a document least misses.
///
/// In **logical pixels**, the interface-scale unit everywhere above the
/// mechanism layer (R-938). The live value comes from the skin
/// (`Metric::scroll_bar`); this is only the default.
pub const BAR: f32 = 22.0;

/// The thumb never shrinks past this, however long the content. Proportional to
/// the bar: at a wide bar a short thumb reads as a stub.
pub const MIN_THUMB: f32 = BAR * 1.5;

/// The zoom slider's length when there is room for it.
///
/// **Fixed, not "whatever is left".** The gutter belongs to the scroll bar; the
/// cluster is a guest at the far end of it. An earlier version let the slider
/// take all the remaining room and the scroll track came out zero-length —
/// caught by the thumb-floor test, which suddenly had no track to sit in.
pub const SLIDER: f32 = 96.0;

/// The track never shrinks below this. When the gutter cannot afford a track
/// *and* a slider, the slider is what goes (ui-07 §6.4) — scrolling is the
/// gutter's job and zoom is the guest.
pub const MIN_TRACK: f32 = MIN_THUMB * 2.0;

/// How far outside the bar the pointer may stray during a thumb drag before the
/// thumb snaps home.
///
/// The book's figure is "a little more than the width of the scroll box"
/// (inventory §3a, p. 165) — which is what this was, and it proved too tight in
/// the hand. Doubled after using it, which is the order that was promised:
/// widened *after* being operated, not guessed at in advance.
pub const DRAG_TOLERANCE: f32 = BAR * 3.0;

/// One zoom step: a **quarter** of an octave, as a *ratio*, because the scale
/// axis is logarithmic.
///
/// Started at an eighth and doubled after using it — an eighth was too fine to
/// get anywhere with, and crossing the range took more clicks than anyone will
/// spend. One number, shared by the magnifier buttons, the wheel over a cluster
/// and the keyboard, so that all three agree about what a step is.
pub const ZOOM_STEP: f32 = 1.189_207_1; // 2^(1/4)

/// The lengths a pane is built from.
///
/// **Carried on the pane rather than read from constants** so that the skin can
/// set them: how wide a scroll bar is, is a matter of feel, and feel lives in
/// the skin beside `slider_width` and `touch_min`. `Kit::layout` copies them in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneMetric {
    pub bar: f32,
    pub min_thumb: f32,
    pub slider: f32,
    pub min_track: f32,
    pub drag_tolerance: f32,
}

impl Default for PaneMetric {
    fn default() -> PaneMetric {
        PaneMetric {
            bar: BAR,
            min_thumb: MIN_THUMB,
            slider: SLIDER,
            min_track: MIN_TRACK,
            drag_tolerance: DRAG_TOLERANCE,
        }
    }
}

impl PaneMetric {
    /// A cluster button is square, the bar's width across — so it grows with the
    /// bar, and the magnifier inside it grows with the button.
    pub fn button(&self) -> f32 {
        self.bar
    }
}

/// Which part of a pane's furniture the pointer is over.
///
/// **The interior is not one of these.** A control highlights under the pointer
/// because it is about to do something; content is not a control, and lighting
/// it up would promise an action that does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Thumb(Axis),
    Track(Axis),
    ZoomOut(Axis),
    ZoomIn(Axis),
    ZoomSlider(Axis),
}

impl Part {
    pub fn axis(self) -> Axis {
        match self {
            Part::Thumb(a)
            | Part::Track(a)
            | Part::ZoomOut(a)
            | Part::ZoomIn(a)
            | Part::ZoomSlider(a) => a,
        }
    }
}

/// A scrollable region: how big the content is, where we are in it, and how
/// magnified it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pane {
    /// The content's size, in content units.
    pub extent: Size,
    /// The viewport's top-left within the content, in content units.
    pub offset: Point,
    pub scale: Scale,
    pub bar: BarPolicy,
    /// Zoom limits, as content-units-per-pixel. Content-relative, so a pane can
    /// never zoom out to a void it has to be rescued from (ui-07 §6.2).
    pub scale_min: f32,
    pub scale_max: f32,
    pub metric: PaneMetric,
}

impl Default for Pane {
    fn default() -> Pane {
        Pane {
            extent: Size::new(0.0, 0.0),
            offset: Point::new(0.0, 0.0),
            scale: Scale::default(),
            bar: BarPolicy::default(),
            scale_min: 1.0 / 64.0,
            scale_max: 64.0,
            metric: PaneMetric::default(),
        }
    }
}

impl Pane {
    /// The interior: the pane's rect less the reserved gutters.
    ///
    /// **Reserved unconditionally.** This does not consult the extent, which is
    /// the whole point — the interior is the same size whether or not there is
    /// anything to scroll.
    pub fn interior(&self, rect: Rect) -> Rect {
        let right = if self.bar.has(Axis::Vertical) {
            BAR
        } else {
            0.0
        };
        let bottom = if self.bar.has(Axis::Horizontal) {
            BAR
        } else {
            0.0
        };
        Rect::new(
            rect.x,
            rect.y,
            (rect.w - right).max(0.0),
            (rect.h - bottom).max(0.0),
        )
    }

    /// The whole gutter for an axis: bar plus zoom cluster.
    pub fn gutter(&self, rect: Rect, axis: Axis) -> Option<Rect> {
        if !self.bar.has(axis) {
            return None;
        }
        let inner = self.interior(rect);
        Some(match axis {
            Axis::Vertical => Rect::new(inner.right(), rect.y, self.metric.bar, inner.h),
            Axis::Horizontal => Rect::new(rect.x, inner.bottom(), inner.w, self.metric.bar),
        })
    }

    /// How long the cluster is: two buttons, plus a slider when the gutter can
    /// afford one without starving the track.
    fn cluster_length(&self, gutter: Rect, axis: Axis) -> f32 {
        let available = match axis {
            Axis::Horizontal => gutter.w,
            Axis::Vertical => gutter.h,
        };
        let full = 2.0 * self.metric.button() + self.metric.slider;
        if available >= self.metric.min_track + full {
            full
        } else {
            (2.0 * self.metric.button()).min(available)
        }
    }

    /// The zoom cluster, at the **far end** of the gutter: the right of a
    /// horizontal bar, the bottom of a vertical one (ui-07 §6.4).
    pub fn cluster(&self, rect: Rect, axis: Axis) -> Option<Rect> {
        let gutter = self.gutter(rect, axis)?;
        let want = self.cluster_length(gutter, axis);
        Some(match axis {
            Axis::Horizontal => {
                let w = want.min(gutter.w);
                Rect::new(gutter.right() - w, gutter.y, w, gutter.h)
            }
            Axis::Vertical => {
                let h = want.min(gutter.h);
                Rect::new(gutter.x, gutter.bottom() - h, gutter.w, h)
            }
        })
    }

    /// The zoom slider's trough, when there is room for one between the
    /// buttons. `None` means the cluster has collapsed to `[-][+]`.
    ///
    /// **The buttons do not move when it disappears.** Degradation here is
    /// subtractive — the middle is absent, nothing is substituted — which is
    /// what keeps the zoom-in button in the same place at every window size.
    pub fn zoom_slider(&self, rect: Rect, axis: Axis) -> Option<Rect> {
        let gutter = self.gutter(rect, axis)?;
        let (button, slider) = (self.metric.button(), self.metric.slider);
        if self.cluster_length(gutter, axis) <= 2.0 * button {
            return None;
        }
        Some(match axis {
            Axis::Horizontal => {
                Rect::new(gutter.right() - button - slider, gutter.y, slider, gutter.h)
            }
            Axis::Vertical => Rect::new(
                gutter.x,
                gutter.bottom() - button - slider,
                gutter.w,
                slider,
            ),
        })
    }

    /// The two cluster buttons, `(minus, plus)`.
    ///
    /// Horizontal reads `[-] … [+]`. **Vertical is that rotated 90° clockwise**,
    /// so minus is *above* plus — one object in two orientations rather than two
    /// arrangements to learn. It departs from ch. 7 p. 214's up-means-more, and
    /// the magnifier glyphs are what make that safe (inventory §7).
    pub fn zoom_button(&self, rect: Rect, axis: Axis) -> Option<(Rect, Rect)> {
        let gutter = self.gutter(rect, axis)?;
        let slider = self.zoom_slider(rect, axis);
        let button = self.metric.button();
        Some(match axis {
            Axis::Horizontal => {
                let plus = Rect::new(gutter.right() - button, gutter.y, button, gutter.h);
                let minus_x = match slider {
                    Some(s) => s.x - button,
                    None => plus.x - button,
                };
                (Rect::new(minus_x, gutter.y, button, gutter.h), plus)
            }
            Axis::Vertical => {
                let plus = Rect::new(gutter.x, gutter.bottom() - button, gutter.w, button);
                let minus_y = match slider {
                    Some(s) => s.y - button,
                    None => plus.y - button,
                };
                (Rect::new(gutter.x, minus_y, gutter.w, button), plus)
            }
        })
    }

    /// The scroll bar's track — the gutter less whatever the cluster took.
    pub fn track(&self, rect: Rect, axis: Axis) -> Option<Rect> {
        let gutter = self.gutter(rect, axis)?;
        let (minus, _) = self.zoom_button(rect, axis)?;
        Some(match axis {
            Axis::Horizontal => {
                Rect::new(gutter.x, gutter.y, (minus.x - gutter.x).max(0.0), gutter.h)
            }
            Axis::Vertical => {
                Rect::new(gutter.x, gutter.y, gutter.w, (minus.y - gutter.y).max(0.0))
            }
        })
    }

    /// How much content fits in the viewport, in content units.
    pub fn viewport(&self, rect: Rect, axis: Axis) -> f32 {
        let inner = self.interior(rect);
        match axis {
            Axis::Horizontal => inner.w * self.scale.x,
            Axis::Vertical => inner.h * self.scale.y,
        }
    }

    fn extent_on(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.extent.w,
            Axis::Vertical => self.extent.h,
        }
    }

    fn offset_on(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.offset.x,
            Axis::Vertical => self.offset.y,
        }
    }

    fn set_offset_on(&mut self, axis: Axis, value: f32) {
        match axis {
            Axis::Horizontal => self.offset.x = value,
            Axis::Vertical => self.offset.y = value,
        }
    }

    /// True when there is more content than viewport. A bar with nothing to
    /// scroll is drawn **inactive**, never hidden (ui-07 §5.1).
    pub fn active(&self, rect: Rect, axis: Axis) -> bool {
        self.bar.has(axis) && self.extent_on(axis) > self.viewport(rect, axis) + f32::EPSILON
    }

    /// The thumb, or `None` when the bar is inactive.
    ///
    /// Proportional: it carries position **and** how much of the content is in
    /// view, which is the departure from the 1992 fixed box (inventory §7).
    pub fn thumb(&self, rect: Rect, axis: Axis) -> Option<Rect> {
        if !self.active(rect, axis) {
            return None;
        }
        let track = self.track(rect, axis)?;
        let length = match axis {
            Axis::Horizontal => track.w,
            Axis::Vertical => track.h,
        };
        let extent = self.extent_on(axis);
        let viewport = self.viewport(rect, axis);
        let thumb = (length * (viewport / extent)).clamp(self.metric.min_thumb.min(length), length);
        // **Travel is `track − thumb`, not `track − MIN_THUMB`.** Once the floor
        // engages the two differ, and using the wrong one leaves the end of a
        // long document unreachable — invisible in short content, permanent in
        // long.
        let travel = (length - thumb).max(0.0);
        let scrollable = (extent - viewport).max(f32::EPSILON);
        let at = (self.offset_on(axis) / scrollable).clamp(0.0, 1.0) * travel;
        Some(match axis {
            Axis::Horizontal => Rect::new(track.x + at, track.y, thumb, track.h),
            Axis::Vertical => Rect::new(track.x, track.y + at, track.w, thumb),
        })
    }

    /// Clamp the offset so the viewport stays inside the content.
    pub fn clamp(&mut self, rect: Rect) {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let limit = (self.extent_on(axis) - self.viewport(rect, axis)).max(0.0);
            let value = self.offset_on(axis).clamp(0.0, limit);
            self.set_offset_on(axis, value);
        }
    }

    /// Scroll by a distance in content units.
    pub fn scroll_by(&mut self, rect: Rect, dx: f32, dy: f32) {
        self.offset.x += dx;
        self.offset.y += dy;
        self.clamp(rect);
    }

    /// Where a thumb dragged to `at` (window coordinates) puts the offset.
    pub fn offset_for_thumb(&self, rect: Rect, axis: Axis, at: Point, grab: f32) -> f32 {
        let Some(track) = self.track(rect, axis) else {
            return self.offset_on(axis);
        };
        let Some(thumb) = self.thumb(rect, axis) else {
            return self.offset_on(axis);
        };
        let (origin, length, thumb_length, pointer) = match axis {
            Axis::Horizontal => (track.x, track.w, thumb.w, at.x),
            Axis::Vertical => (track.y, track.h, thumb.h, at.y),
        };
        let travel = (length - thumb_length).max(f32::EPSILON);
        let position = ((pointer - grab - origin) / travel).clamp(0.0, 1.0);
        position * (self.extent_on(axis) - self.viewport(rect, axis)).max(0.0)
    }

    /// A page: a windowful **less one unit of overlap**, which is what keeps
    /// the reader's context across the jump (inventory §3a, p. 164).
    pub fn page(&self, rect: Rect, axis: Axis, unit: f32) -> f32 {
        (self.viewport(rect, axis) - unit).max(unit.min(self.viewport(rect, axis)))
    }

    /// Zoom one axis by `factor`, holding the content at `anchor` still.
    ///
    /// The anchor is the whole trick: hold the point under the pointer and it is
    /// a lens; hold nothing and it is a jump (ui-07 §6.2).
    pub fn zoom(&mut self, rect: Rect, axis: Axis, factor: f32, anchor: Point) {
        let inner = self.interior(rect);
        let (pixel, origin) = match axis {
            Axis::Horizontal => (anchor.x - inner.x, self.offset.x),
            Axis::Vertical => (anchor.y - inner.y, self.offset.y),
        };
        let before = self.scale.on(axis);
        let after = (before * factor).clamp(self.scale_min, self.scale_max);
        if after == before {
            return;
        }
        // The content under the anchor is `origin + pixel·before`; solving for
        // the offset that keeps it there under the new scale gives this.
        let held = origin + pixel * before;
        let moved = held - pixel * after;
        match axis {
            Axis::Horizontal => {
                self.scale.x = after;
                self.offset.x = moved;
            }
            Axis::Vertical => {
                self.scale.y = after;
                self.offset.y = moved;
            }
        }
        self.clamp(rect);
    }

    /// The zoom slider's position, 0..=1, **logarithmic** — so equal travel is
    /// equal ratio, which is what makes creeping it feel even.
    pub fn zoom_position(&self, axis: Axis) -> f32 {
        let span = (self.scale_max / self.scale_min).ln();
        if span <= 0.0 {
            return 0.0;
        }
        // Inverted, so pushing toward `[+]` zooms *in*: a smaller number of
        // content units per pixel is more magnification.
        1.0 - ((self.scale.on(axis) / self.scale_min).ln() / span).clamp(0.0, 1.0)
    }

    /// Set the scale from a slider position, 0..=1, holding `anchor` still.
    pub fn set_zoom_position(&mut self, rect: Rect, axis: Axis, position: f32, anchor: Point) {
        let span = self.scale_max / self.scale_min;
        let want = self.scale_min * span.powf(1.0 - position.clamp(0.0, 1.0));
        let factor = want / self.scale.on(axis);
        self.zoom(rect, axis, factor, anchor);
    }

    /// Which piece of furniture is at a point, if any. `None` means the
    /// interior — content, which is not a control and does not highlight.
    pub fn part_at(&self, rect: Rect, at: Point) -> Option<Part> {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let gutter = self.gutter(rect, axis)?;
            if !gutter.contains(at) {
                continue;
            }
            if let Some((minus, plus)) = self.zoom_button(rect, axis) {
                if minus.contains(at) {
                    return Some(Part::ZoomOut(axis));
                }
                if plus.contains(at) {
                    return Some(Part::ZoomIn(axis));
                }
            }
            if let Some(slider) = self.zoom_slider(rect, axis)
                && slider.contains(at)
            {
                return Some(Part::ZoomSlider(axis));
            }
            if let Some(thumb) = self.thumb(rect, axis)
                && thumb.contains(at)
            {
                return Some(Part::Thumb(axis));
            }
            return Some(Part::Track(axis));
        }
        None
    }

    /// Content coordinates for a point in the window.
    pub fn to_content(&self, rect: Rect, at: Point) -> Point {
        let inner = self.interior(rect);
        Point::new(
            self.offset.x + (at.x - inner.x) * self.scale.x,
            self.offset.y + (at.y - inner.y) * self.scale.y,
        )
    }

    /// Window coordinates for a point in the content.
    pub fn to_window(&self, rect: Rect, at: Point) -> Point {
        let inner = self.interior(rect);
        Point::new(
            inner.x + (at.x - self.offset.x) / self.scale.x,
            inner.y + (at.y - self.offset.y) / self.scale.y,
        )
    }
}

#[cfg(test)]
mod test;
