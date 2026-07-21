//! The paint list: the drawing vocabulary the kit is given.
//!
//! Small and closed on purpose. It is the seam that keeps the renderer swappable
//! (ui-01 §2) — deliberately implementable by a browser canvas without translation
//! loss, and by a GPU presentation path later without any widget noticing. Nothing
//! here exposes tiny-skia, and nothing above this crate may name it.
//!
//! **Everything is logical pixels.** The painter applies the window's scale factor
//! once, at the boundary, so widget geometry never multiplies by DPI.

use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Pixmap, PremultipliedColorU8,
    Shader, SpreadMode, Stroke, Transform,
};

use crate::blur;
use crate::fill::{Fill, PaintStat, Shadow};
use crate::geometry::{Point, Rect};
use crate::text::{FontStack, Shaped, TextStyle};

/// Opaque sRGB colour. Alpha composites within a frame; the window itself is opaque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// From `0xRRGGBB`, the form skin tables are written in.
    pub const fn hex(v: u32) -> Color {
        Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    pub(crate) fn skia(&self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }
}

/// A filled or stroked outline, built in logical pixels.
#[derive(Debug, Clone)]
pub struct Path(tiny_skia::Path);

/// Builds a [`Path`]. Coordinates are logical; the painter scales them.
#[derive(Debug, Default)]
pub struct Outline(PathBuilder);

impl Outline {
    pub fn new() -> Outline {
        Outline(PathBuilder::new())
    }

    pub fn move_to(&mut self, p: Point) {
        self.0.move_to(p.x, p.y);
    }

    pub fn line_to(&mut self, p: Point) {
        self.0.line_to(p.x, p.y);
    }

    pub fn quad_to(&mut self, control: Point, to: Point) {
        self.0.quad_to(control.x, control.y, to.x, to.y);
    }

    pub fn cubic_to(&mut self, c1: Point, c2: Point, to: Point) {
        self.0.cubic_to(c1.x, c1.y, c2.x, c2.y, to.x, to.y);
    }

    pub fn close(&mut self) {
        self.0.close();
    }

    /// `None` when nothing was added, or the result is degenerate.
    pub fn finish(self) -> Option<Path> {
        self.0.finish().map(Path)
    }
}

/// Draws into one window's back buffer for one frame.
///
/// Offsets and clips nest as stacks; both are cheap and both are honoured by every
/// primitive.
pub struct Painter<'a> {
    pixmap: &'a mut Pixmap,
    /// Shaping and glyph rasterization. Borrowed for the frame, so a widget can
    /// shape text at paint time without the mechanism handing out a second path
    /// to the font stack.
    text: &'a mut FontStack,
    /// Physical pixels per logical pixel: the only place DPI is applied.
    scale: f32,
    /// Accumulated translation in logical pixels.
    offset: Point,
    offset_stack: Vec<Point>,
    /// Current clip in **device** pixels; starts as the whole buffer.
    clip: Rect,
    clip_stack: Vec<Rect>,
    /// A rectangular clip mask, built only when a non-trivial clip meets a primitive
    /// that cannot be clipped analytically. Cached because consecutive draws share a
    /// clip far more often than they change it.
    mask: Option<(Rect, Mask)>,
    /// What the shadow path cost this frame. Measured, not assumed.
    stat: &'a mut PaintStat,
    /// Blur scratch, reused across every shadow in the frame.
    scratch: blur::Scratch,
}

impl<'a> Painter<'a> {
    /// `bound` is the dirty region's bounds in **logical** pixels: the outermost
    /// clip, which no `push_clip` can widen.
    pub(crate) fn new(
        pixmap: &'a mut Pixmap,
        text: &'a mut FontStack,
        stat: &'a mut PaintStat,
        scale: f32,
        bound: Rect,
    ) -> Painter<'a> {
        let full = Rect::new(0.0, 0.0, pixmap.width() as f32, pixmap.height() as f32);
        let device = Rect::new(
            bound.x * scale,
            bound.y * scale,
            bound.w * scale,
            bound.h * scale,
        );
        Painter {
            pixmap,
            text,
            stat,
            scale,
            offset: Point::default(),
            offset_stack: Vec::new(),
            clip: full.intersect(device),
            clip_stack: Vec::new(),
            mask: None,
            scratch: blur::Scratch::default(),
        }
    }

    /// Fill the clipped area — the frame's first act.
    ///
    /// Honours the clip rather than the whole buffer, so clearing a partially dirty
    /// frame does not erase the pixels the last frame left valid outside it.
    pub fn clear(&mut self, color: Color) {
        let d = self.clip;
        if d.empty() {
            return;
        }
        if let Some(rect) = tiny_skia::Rect::from_xywh(d.x, d.y, d.w, d.h) {
            let mut paint = self.paint_of(Fill::Solid(color), d);
            // Overwrite rather than blend: "clear" means replace, and the buffer
            // still holds the previous frame.
            paint.blend_mode = tiny_skia::BlendMode::Source;
            self.pixmap
                .fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    /// Shift subsequent drawing by `d` logical pixels.
    pub fn push_offset(&mut self, d: Point) {
        self.offset_stack.push(self.offset);
        self.offset = Point::new(self.offset.x + d.x, self.offset.y + d.y);
    }

    pub fn pop_offset(&mut self) {
        if let Some(prev) = self.offset_stack.pop() {
            self.offset = prev;
        }
    }

    /// Restrict subsequent drawing to `r` (logical), intersected with the clip
    /// already in force. Clips only ever shrink.
    pub fn push_clip(&mut self, r: Rect) {
        self.clip_stack.push(self.clip);
        self.clip = self.clip.intersect(self.device(r));
        self.mask = None;
    }

    pub fn pop_clip(&mut self) {
        if let Some(prev) = self.clip_stack.pop() {
            self.clip = prev;
            self.mask = None;
        }
    }

    pub fn fill_rect(&mut self, r: Rect, fill: impl Into<Fill>) {
        // Axis-aligned rectangles clip exactly by intersection — no mask, no
        // rasterizer, and the overwhelmingly common case in a widget kit.
        let device = snap(self.device(r));
        let d = device.intersect(self.clip);
        if d.empty() {
            return;
        }
        // The gradient is anchored to the *requested* rectangle, not the clipped
        // one, so clipping a shape never shifts its shading.
        let paint = self.paint_of(fill.into(), device);
        if let Some(rect) = tiny_skia::Rect::from_xywh(d.x, d.y, d.w, d.h) {
            self.pixmap
                .fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    pub fn fill_round_rect(&mut self, r: Rect, radius: f32, fill: impl Into<Fill>) {
        let d = snap(self.device(r));
        let radius = (radius * self.scale).min(d.w / 2.0).min(d.h / 2.0);
        if d.empty() {
            return;
        }
        let fill = fill.into();
        if radius <= 0.5 {
            return self.fill_rect(r, fill);
        }
        let Some(path) = round_rect_path(d, radius) else {
            return;
        };
        self.fill_device_path(&path, fill, d);
    }

    pub fn stroke_line(&mut self, a: Point, b: Point, color: Color, width: f32) {
        let (mut a, mut b) = (self.device_point(a), self.device_point(b));
        // Axis-aligned strokes are snapped onto the device grid so a one-pixel rule
        // stays one crisp pixel instead of two grey ones. An odd-width stroke
        // centres on a pixel's middle (x.5); an even one on a boundary. Diagonals
        // are left alone — snapping them would bend the line.
        let device_width = (width * self.scale).max(1.0);
        let odd = (device_width.round() as i32) % 2 == 1;
        let align = |v: f32| if odd { v.floor() + 0.5 } else { v.round() };
        if (a.y - b.y).abs() < 0.01 {
            a.y = align(a.y);
            b.y = a.y;
        }
        if (a.x - b.x).abs() < 0.01 {
            a.x = align(a.x);
            b.x = a.x;
        }
        let mut outline = PathBuilder::new();
        outline.move_to(a.x, a.y);
        outline.line_to(b.x, b.y);
        let Some(path) = outline.finish() else {
            return;
        };
        let stroke = Stroke {
            width: device_width,
            ..Stroke::default()
        };
        // Field-splitting borrow: the mask cache and the pixmap are siblings, so
        // one can be read while the other is written.
        let Painter {
            pixmap, clip, mask, ..
        } = self;
        let mask = ensure_mask(pixmap.width(), pixmap.height(), *clip, mask);
        pixmap.stroke_path(&path, &solid(color), &stroke, Transform::identity(), mask);
    }

    pub fn fill_path(&mut self, path: &Path, fill: impl Into<Fill>) {
        // The path is in logical coordinates; transform it once into device space
        // rather than transforming at draw time, so clipping and stroking agree.
        let Some(device) = path.0.clone().transform(self.device_transform()) else {
            return;
        };
        let b = device.bounds();
        let anchor = Rect::new(b.x(), b.y(), b.width(), b.height());
        self.fill_device_path(&device, fill.into(), anchor);
    }

    /// Shape and measure text. Logical units; the result survives a DPI change,
    /// because rasterization is scaled but layout is not.
    pub fn shape(&mut self, text: &str, style: &TextStyle) -> Shaped {
        self.text.shape(text, style)
    }

    /// Draw a shaped run with its **top-left** at `at` — not its baseline, because
    /// widgets lay out boxes, and the baseline is the text stack's business.
    pub fn draw_text(&mut self, shaped: &Shaped, at: Point, color: Color) {
        let origin = self.device_point(at);
        let clip = self.clip;
        let width = self.pixmap.width() as i32;
        let height = self.pixmap.height() as i32;
        let Painter {
            pixmap,
            text,
            scale,
            ..
        } = self;
        let pixel = pixmap.pixels_mut();
        // Coverage in, source-over out. The clip is tested per pixel: glyph runs
        // are small, and a mask would cost a buffer-sized allocation to spare a
        // handful of comparisons.
        text.render(shaped, origin, *scale, |x, y, coverage| {
            if x < 0 || y < 0 || x >= width || y >= height {
                return;
            }
            let (fx, fy) = (x as f32, y as f32);
            if fx < clip.x || fy < clip.y || fx >= clip.right() || fy >= clip.bottom() {
                return;
            }
            let alpha = u32::from(coverage) * u32::from(color.a) / 255;
            if alpha == 0 {
                return;
            }
            let slot = &mut pixel[(y * width + x) as usize];
            *slot = blend(*slot, color, alpha as u8);
        });
    }

    /// Cast a blurred shadow from a rounded rectangle — behind it, or inside it.
    ///
    /// Rounded rectangles only: it is everything the skin casts shadows from, it
    /// keeps the paint vocabulary small, and it bounds the work to a mask that is
    /// cheap to rasterize. Arbitrary path shadows would be a much larger surface
    /// with no caller.
    ///
    /// No cache. The cost is counted instead ([`PaintStat`]), so whether a cache is
    /// worth building is a question the numbers answer rather than one we guess.
    pub fn shadow_round_rect(&mut self, r: Rect, radius: f32, shadow: &Shadow) {
        let shape = snap(self.device(r));
        if shape.empty() {
            return;
        }
        let s = self.scale;
        let sigma = (shadow.sigma * s).max(0.0);
        let offset = Point::new(shadow.offset.x * s, shadow.offset.y * s);
        let radius = (radius * s).min(shape.w / 2.0).min(shape.h / 2.0).max(0.0);
        let pad = blur::spread(sigma);

        // What can receive ink: the silhouette plus its blur for an outer shadow,
        // the shape itself for an inset one, which cannot escape it.
        let region = if shadow.inset {
            shape
        } else {
            Rect::new(
                shape.x + offset.x - pad,
                shape.y + offset.y - pad,
                shape.w + pad * 2.0,
                shape.h + pad * 2.0,
            )
        }
        .intersect(self.clip);
        if region.empty() {
            return;
        }

        let (rx, ry) = (region.x.floor(), region.y.floor());
        let rw = region.w.ceil() as usize;
        let rh = region.h.ceil() as usize;
        let (Some(mut cast), true) = (Mask::new(rw as u32, rh as u32), rw > 0 && rh > 0) else {
            return;
        };

        // The blurred silhouette, offset. For an inset shadow this is the *hole*:
        // the ink is what falls outside it, which is why it is inverted below.
        let cast_rect = Rect::new(
            shape.x + offset.x - rx,
            shape.y + offset.y - ry,
            shape.w,
            shape.h,
        );
        if let Some(path) = round_rect_path(cast_rect, radius) {
            cast.fill_path(&path, FillRule::Winding, true, Transform::identity());
        }
        let started = std::time::Instant::now();
        blur::blur(cast.data_mut(), rw, rh, sigma, &mut self.scratch);
        self.stat.record(
            (
                shape.w.round() as u32,
                shape.h.round() as u32,
                radius.round() as u32,
                sigma.round() as u32,
            ),
            (rw * rh) as u64,
            started.elapsed().as_nanos() as u64,
        );

        // An inset shadow is confined to the shape, so it needs the shape's own
        // unblurred coverage too.
        let confine = if shadow.inset {
            Mask::new(rw as u32, rh as u32).map(|mut m| {
                let local = Rect::new(shape.x - rx, shape.y - ry, shape.w, shape.h);
                if let Some(path) = round_rect_path(local, radius) {
                    m.fill_path(&path, FillRule::Winding, true, Transform::identity());
                }
                m
            })
        } else {
            None
        };

        let (inset, color, clip) = (shadow.inset, shadow.color, self.clip);
        let width = self.pixmap.width() as i32;
        let height = self.pixmap.height() as i32;
        let pixel = self.pixmap.pixels_mut();
        for y in 0..rh {
            for x in 0..rw {
                let coverage = cast.data()[y * rw + x];
                let a = if inset {
                    // Ink where the offset silhouette is *absent*, kept inside the
                    // shape — the recessed-groove look.
                    let inside = confine.as_ref().map_or(0, |m| m.data()[y * rw + x]);
                    (u32::from(255 - coverage) * u32::from(inside)) / 255
                } else {
                    u32::from(coverage)
                } * u32::from(color.a)
                    / 255;
                if a == 0 {
                    continue;
                }
                let (px, py) = (rx as i32 + x as i32, ry as i32 + y as i32);
                if px < 0 || py < 0 || px >= width || py >= height {
                    continue;
                }
                let (fx, fy) = (px as f32, py as f32);
                if fx < clip.x || fy < clip.y || fx >= clip.right() || fy >= clip.bottom() {
                    continue;
                }
                let slot = &mut pixel[(py * width + px) as usize];
                *slot = blend(*slot, color, a as u8);
            }
        }
    }

    /// The clip currently in force, in logical pixels — what a dirty-aware widget
    /// asks before deciding to draw at all.
    pub fn clip(&self) -> Rect {
        let s = self.scale;
        Rect::new(
            self.clip.x / s - self.offset.x,
            self.clip.y / s - self.offset.y,
            self.clip.w / s,
            self.clip.h / s,
        )
    }

    fn fill_device_path(&mut self, path: &tiny_skia::Path, fill: Fill, anchor: Rect) {
        let paint = self.paint_of(fill, anchor);
        let Painter {
            pixmap, clip, mask, ..
        } = self;
        let mask = ensure_mask(pixmap.width(), pixmap.height(), *clip, mask);
        pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), mask);
    }

    /// A tiny-skia paint for a fill. Gradient endpoints are given in the *shape's*
    /// logical frame (0..1 of its own box is the common case), resolved here against
    /// the device rectangle the shape occupies.
    fn paint_of(&self, fill: Fill, anchor: Rect) -> Paint<'static> {
        let shader = match fill {
            Fill::Solid(c) => Shader::SolidColor(c.skia()),
            Fill::Linear { from, to, stop } => {
                let s = self.scale;
                let p =
                    |q: Point| tiny_skia::Point::from_xy(anchor.x + q.x * s, anchor.y + q.y * s);
                let stops: Vec<GradientStop> = stop
                    .iter()
                    .map(|(at, c)| GradientStop::new(*at, c.skia()))
                    .collect();
                LinearGradient::new(
                    p(from),
                    p(to),
                    stops,
                    SpreadMode::Pad,
                    Transform::identity(),
                )
                .unwrap_or(Shader::SolidColor(
                    stop.first().map_or(Color::hex(0), |(_, c)| *c).skia(),
                ))
            }
        };
        Paint {
            shader,
            anti_alias: true,
            ..Paint::default()
        }
    }

    /// Logical rect -> device rect: offset, then scale. The one conversion site.
    fn device(&self, r: Rect) -> Rect {
        let s = self.scale;
        Rect::new(
            (r.x + self.offset.x) * s,
            (r.y + self.offset.y) * s,
            r.w * s,
            r.h * s,
        )
    }

    fn device_point(&self, p: Point) -> Point {
        Point::new(
            (p.x + self.offset.x) * self.scale,
            (p.y + self.offset.y) * self.scale,
        )
    }

    fn device_transform(&self) -> Transform {
        Transform::from_translate(self.offset.x, self.offset.y).post_scale(self.scale, self.scale)
    }
}

/// Snap a device rectangle onto the pixel grid.
///
/// Non-integer interface scales (R-938) are the reason: at 1.25x a panel edge lands
/// on a half pixel and softens into two grey rows, which is exactly how a control
/// skin stops looking like an instrument. A rectangle that had any extent keeps at
/// least one pixel of it, so a hairline never rounds away to nothing.
fn snap(r: Rect) -> Rect {
    let x0 = r.x.round();
    let y0 = r.y.round();
    let w = if r.w > 0.0 {
        (r.right().round() - x0).max(1.0)
    } else {
        0.0
    };
    let h = if r.h > 0.0 {
        (r.bottom().round() - y0).max(1.0)
    } else {
        0.0
    };
    Rect::new(x0, y0, w, h)
}

/// A mask for the current clip, or `None` when the clip is the whole buffer.
///
/// Built on demand and cached, because a mask costs a byte per device pixel and
/// consecutive draws share a clip far more often than they change it. Free function
/// rather than a method so the caller can hold the pixmap borrow at the same time.
fn ensure_mask(
    width: u32,
    height: u32,
    clip: Rect,
    cache: &mut Option<(Rect, Mask)>,
) -> Option<&Mask> {
    if clip == Rect::new(0.0, 0.0, width as f32, height as f32) {
        return None;
    }
    let stale = match cache {
        Some((r, _)) => *r != clip,
        None => true,
    };
    if stale {
        let mut mask = Mask::new(width, height)?;
        let rect = tiny_skia::Rect::from_xywh(clip.x, clip.y, clip.w, clip.h)?;
        mask.fill_path(
            &PathBuilder::from_rect(rect),
            FillRule::Winding,
            true,
            Transform::identity(),
        );
        *cache = Some((clip, mask));
    }
    cache.as_ref().map(|(_, m)| m)
}

/// Source-over one coverage sample onto a premultiplied destination pixel.
fn blend(dst: PremultipliedColorU8, color: Color, alpha: u8) -> PremultipliedColorU8 {
    let a = u32::from(alpha);
    let inv = 255 - a;
    // Premultiply the source by its own coverage, then add the attenuated dest.
    let mix = |s: u8, d: u8| ((u32::from(s) * a + u32::from(d) * inv) / 255) as u8;
    let r = mix(color.r, dst.red());
    let g = mix(color.g, dst.green());
    let b = mix(color.b, dst.blue());
    let out_a = (a + u32::from(dst.alpha()) * inv / 255) as u8;
    // Clamped rather than asserted: rounding can push a channel a step past alpha,
    // and a panic in the text path would be a poor trade for one unit of error.
    PremultipliedColorU8::from_rgba(r.min(out_a), g.min(out_a), b.min(out_a), out_a).unwrap_or(dst)
}

fn solid(color: Color) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(color.skia()),
        anti_alias: true,
        ..Paint::default()
    }
}

/// A rounded rectangle as four arcs, approximated with cubics. Device space.
fn round_rect_path(d: Rect, r: f32) -> Option<tiny_skia::Path> {
    // 0.5523 is the standard circular-arc cubic constant: the control-point
    // distance that approximates a quarter circle to within ~0.02%.
    let k = r * 0.552_284_8;
    let (x0, y0, x1, y1) = (d.x, d.y, d.right(), d.bottom());
    let mut p = PathBuilder::new();
    p.move_to(x0 + r, y0);
    p.line_to(x1 - r, y0);
    p.cubic_to(x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r);
    p.line_to(x1, y1 - r);
    p.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    p.line_to(x0 + r, y1);
    p.cubic_to(x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r);
    p.line_to(x0, y0 + r);
    p.cubic_to(x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0);
    p.close();
    p.finish()
}

#[cfg(test)]
mod test;
