//! Shaping, measurement, and glyph rasterization.
//!
//! Kit widgets never touch a font, a face, or a glyph: they name a **role**, and the
//! role table resolves it. Roles exist from day one even though the table is small
//! (ui-01 §13.3) — the same build-for-N discipline as windows and projects.
//!
//! **Fonts are bundled and embedded**, because bit-identical screenshot golden
//! masters are impossible with system fonts: two machines enumerate different faces
//! and rasterize different glyphs. System fallback is available at runtime and
//! forced off under test, so users still get their glyphs and references stay exact.

use std::collections::HashMap;

use cosmic_text::{
    Attrs, Buffer, CacheKeyFlags, Family, FontSystem, Metrics, Shaping, Style, SwashCache,
    SwashContent, Weight,
};

use crate::geometry::{Point, Size};

/// What a piece of text is *for*. Widgets ask for a role, never a family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontRole {
    /// Labels, names, menus, prose — anything read as language.
    #[default]
    Ui,
    /// Counters, tempo, positions — anything read as a number. Monospaced, so
    /// digits cannot shift as values change.
    Numeric,
}

/// How to render a piece of text. Size is in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub role: FontRole,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl Default for TextStyle {
    fn default() -> TextStyle {
        TextStyle {
            role: FontRole::Ui,
            size: 13.0,
            bold: false,
            italic: false,
        }
    }
}

impl TextStyle {
    pub fn ui(size: f32) -> TextStyle {
        TextStyle {
            size,
            ..TextStyle::default()
        }
    }

    pub fn numeric(size: f32) -> TextStyle {
        TextStyle {
            role: FontRole::Numeric,
            size,
            ..TextStyle::default()
        }
    }

    pub fn bold(self) -> TextStyle {
        TextStyle { bold: true, ..self }
    }

    pub fn italic(self) -> TextStyle {
        TextStyle {
            italic: true,
            ..self
        }
    }
}

/// Bundled faces, embedded so the binary is self-sufficient and every machine
/// rasterizes the same shapes. Declared in `asset/asset.json`; credited by R-1515.
const BUNDLED: &[&[u8]] = &[
    include_bytes!("../../../../asset/font/SourceSans3-Regular.ttf"),
    include_bytes!("../../../../asset/font/SourceSans3-It.ttf"),
    include_bytes!("../../../../asset/font/SourceSans3-Bold.ttf"),
    include_bytes!("../../../../asset/font/SourceSans3-BoldIt.ttf"),
    include_bytes!("../../../../asset/font/JetBrainsMonoNL-Regular.ttf"),
    include_bytes!("../../../../asset/font/JetBrainsMonoNL-Bold.ttf"),
];

const UI_FAMILY: &str = "Source Sans 3";
/// The `NL` build: ligatures are absent from the font rather than suppressed by a
/// shaper flag, so `!=` in a track name can never render as `≠`.
const NUMERIC_FAMILY: &str = "JetBrains Mono NL";

/// One laid-out glyph, in logical coordinates. Rasterization happens later, at the
/// window's scale, so a shaped run survives a DPI change unchanged.
#[derive(Debug, Clone)]
struct Glyph {
    inner: cosmic_text::LayoutGlyph,
    /// Baseline offset of the line this glyph sits on.
    line_y: f32,
}

/// Text that has been shaped and measured but not yet drawn.
#[derive(Debug, Clone)]
pub struct Shaped {
    glyph: Vec<Glyph>,
    size: Size,
    /// Byte length of the source, so a caret can sit past the last glyph.
    len: usize,
}

impl Shaped {
    /// Logical bounding size: advance width by line height.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Where a caret sits for a byte offset — the left edge of the glyph that
    /// starts there, or the end of the run.
    pub fn caret(&self, byte: usize) -> Point {
        let x = self
            .glyph
            .iter()
            .find(|g| g.inner.start >= byte)
            .map_or(self.size.w, |g| g.inner.x);
        Point::new(x, 0.0)
    }

    /// The byte offset a click at `x` addresses. Past the halfway point of a glyph
    /// the caret belongs after it, which is what makes click-to-place feel right.
    pub fn byte_at(&self, x: f32) -> usize {
        for g in &self.glyph {
            if x < g.inner.x + g.inner.w {
                return if x < g.inner.x + g.inner.w / 2.0 {
                    g.inner.start
                } else {
                    g.inner.end
                };
            }
        }
        self.len
    }
}

/// A shaped-run cache key: everything that decides a run's layout except the
/// text itself (which is the inner map's key, so a hit needs no allocation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ShapeKey {
    role: FontRole,
    /// `f32` is not `Hash`/`Eq`; its bit pattern is, and equal sizes shape alike.
    size_bits: u32,
    bold: bool,
    italic: bool,
}

impl ShapeKey {
    fn of(style: &TextStyle) -> ShapeKey {
        ShapeKey {
            role: style.role,
            size_bits: style.size.to_bits(),
            bold: style.bold,
            italic: style.italic,
        }
    }
}

/// The text stack: font database, shaper, and glyph raster cache.
pub(crate) struct FontStack {
    system: FontSystem,
    cache: SwashCache,
    /// Cached raster coverage keyed by glyph and device size, so redrawing a
    /// counter does not re-rasterize its digits every frame.
    family: HashMap<FontRole, String>,
    /// Shaped-run cache: `(style) -> (text -> Shaped)`. Laying out a run with
    /// cosmic-text (a fresh `Buffer`, `set_text`, `shape_until_scroll`) costs far
    /// more than cloning the resulting glyphs, and a UI reshapes the *same* runs
    /// every frame — the static table text, the ellipsis candidates, the counter
    /// digits. The two-level shape lets a hit borrow the text as `&str` (no
    /// allocation); only a miss allocates the owned key. Fonts are fixed at
    /// startup, so a shaped run never goes stale.
    shaped: HashMap<ShapeKey, HashMap<String, Shaped>>,
}

/// How many distinct runs to hold before clearing the shape cache wholesale. The
/// live set for a window is a few dozen; this bound only guards a pathological
/// stream of ever-changing text (and a clear just means the next frame reshapes).
const SHAPE_CACHE_CAP: usize = 8192;

impl FontStack {
    /// `system_fallback` loads the platform's fonts behind the bundled ones. Tests
    /// pass false; the application passes true.
    pub(crate) fn new(system_fallback: bool) -> FontStack {
        let mut db = cosmic_text::fontdb::Database::new();
        for face in BUNDLED {
            db.load_font_data(face.to_vec());
        }
        if system_fallback {
            db.load_system_fonts();
        }
        // A fixed locale, not the machine's: locale steers fallback and script
        // defaults, and a golden master must not depend on regional settings.
        let system = FontSystem::new_with_locale_and_db(String::from("en-US"), db);
        let mut family = HashMap::new();
        family.insert(FontRole::Ui, String::from(UI_FAMILY));
        family.insert(FontRole::Numeric, String::from(NUMERIC_FAMILY));
        FontStack {
            system,
            cache: SwashCache::new(),
            family,
            shaped: HashMap::new(),
        }
    }

    /// Shape and measure. Logical units throughout; no scale factor is involved.
    /// A run already laid out under the same style is returned from the cache
    /// rather than reshaped — the whole point is that repainting an unchanged
    /// window costs no fresh shaping.
    pub(crate) fn shape(&mut self, text: &str, style: &TextStyle) -> Shaped {
        let key = ShapeKey::of(style);
        if let Some(hit) = self.shaped.get(&key).and_then(|runs| runs.get(text)) {
            return hit.clone();
        }
        let shaped = self.shape_uncached(text, style);
        if self.shaped.values().map(HashMap::len).sum::<usize>() >= SHAPE_CACHE_CAP {
            self.shaped.clear();
        }
        self.shaped
            .entry(key)
            .or_default()
            .insert(text.to_string(), shaped.clone());
        shaped
    }

    /// The actual cosmic-text layout, behind the cache.
    fn shape_uncached(&mut self, text: &str, style: &TextStyle) -> Shaped {
        let metrics = Metrics::new(style.size, style.size * 1.3);
        let mut buffer = Buffer::new(&mut self.system, metrics);
        let family = self
            .family
            .get(&style.role)
            .map_or(UI_FAMILY, String::as_str);
        let attrs = Attrs {
            family: Family::Name(family),
            weight: if style.bold {
                Weight::BOLD
            } else {
                Weight::NORMAL
            },
            style: if style.italic {
                Style::Italic
            } else {
                Style::Normal
            },
            cache_key_flags: CacheKeyFlags::empty(),
            ..Attrs::new()
        };
        // No width limit: a widget that wants wrapping asks for it explicitly, and
        // labels that silently wrap are a bug rather than a feature.
        buffer.set_size(None, None);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.system, false);

        let mut glyph = Vec::new();
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);
            glyph.extend(run.glyphs.iter().map(|g| Glyph {
                inner: g.clone(),
                line_y: run.line_y,
            }));
        }
        Shaped {
            glyph,
            size: Size::new(width, height.max(metrics.line_height)),
            len: text.len(),
        }
    }

    /// Rasterize a shaped run into a pixel sink at `origin` (device pixels).
    ///
    /// **Grayscale antialiasing only.** Subpixel (LCD) output depends on the
    /// physical panel's stripe order, which would make bit-identical screenshots
    /// meaningless — so a coverage mask is the whole rendering path (ui-01 §13.3).
    pub(crate) fn render(
        &mut self,
        shaped: &Shaped,
        origin: Point,
        scale: f32,
        mut plot: impl FnMut(i32, i32, u8),
    ) {
        for g in &shaped.glyph {
            let physical = g
                .inner
                .physical((origin.x, origin.y + g.line_y * scale), scale);
            let Some(image) = self.cache.get_image(&mut self.system, physical.cache_key) else {
                continue;
            };
            let left = physical.x + image.placement.left;
            let top = physical.y - image.placement.top;
            let (w, h) = (image.placement.width as i32, image.placement.height as i32);
            match image.content {
                // Coverage per pixel: the ordinary path for text.
                SwashContent::Mask => {
                    for row in 0..h {
                        for col in 0..w {
                            let a = image.data[(row * w + col) as usize];
                            if a > 0 {
                                plot(left + col, top + row, a);
                            }
                        }
                    }
                }
                // Colour glyphs (emoji) arrive as RGBA; use the alpha and let the
                // caller's colour stand in until colour glyphs are worth supporting.
                SwashContent::Color => {
                    for row in 0..h {
                        for col in 0..w {
                            let a = image.data[((row * w + col) * 4 + 3) as usize];
                            if a > 0 {
                                plot(left + col, top + row, a);
                            }
                        }
                    }
                }
                SwashContent::SubpixelMask => {}
            }
        }
    }
}

#[cfg(test)]
mod test;
