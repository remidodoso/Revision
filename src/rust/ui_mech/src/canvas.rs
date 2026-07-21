//! Offscreen painting: the same paint list, without a window.
//!
//! Exists for screenshot golden masters — which are only possible because the
//! renderer is CPU-side and the fonts are bundled, so output is bit-identical
//! across machines (ui-01 §2). The font stack here **never loads system fonts**:
//! a reference image that depends on what is installed is not a reference.
//!
//! Useful beyond tests, and deliberately public: offscreen render, thumbnails, and
//! anything else that wants pixels without a surface.

use tiny_skia::Pixmap;

use crate::fill::PaintStat;
use crate::geometry::Rect;
use crate::paint::Painter;
use crate::text::FontStack;

/// A pixel buffer that can be painted and encoded.
pub struct Canvas {
    pixmap: Pixmap,
    text: FontStack,
    stat: PaintStat,
    scale: f32,
}

impl Canvas {
    /// A canvas of `width` × `height` **device** pixels, drawn at `scale`. At scale
    /// 2 a 400×300 canvas presents 200×150 logical pixels to the painter, exactly
    /// as a window on a 2× display would.
    ///
    /// `None` for a zero or absurd size, which is the only way this fails.
    pub fn new(width: u32, height: u32, scale: f32) -> Option<Canvas> {
        Some(Canvas {
            pixmap: Pixmap::new(width, height)?,
            text: FontStack::new(false),
            stat: PaintStat::default(),
            scale,
        })
    }

    /// Logical size available to the painter.
    pub fn size(&self) -> crate::geometry::Size {
        crate::geometry::Size::new(
            self.pixmap.width() as f32 / self.scale,
            self.pixmap.height() as f32 / self.scale,
        )
    }

    /// Paint one pass. The whole canvas is treated as dirty.
    pub fn paint(&mut self, f: impl FnOnce(&mut Painter)) {
        self.stat.clear();
        let size = self.size();
        let bound = Rect::new(0.0, 0.0, size.w, size.h);
        let mut painter = Painter::new(
            &mut self.pixmap,
            &mut self.text,
            &mut self.stat,
            self.scale,
            bound,
        );
        f(&mut painter);
    }

    /// Encode as PNG. Unlike `testdata`'s raw `.f32` audio frames, a rendering
    /// reference should be *lookable-at* when it fails.
    pub fn png(&self) -> Result<Vec<u8>, crate::MechError> {
        self.pixmap
            .encode_png()
            .map_err(|e| crate::MechError::Surface(e.to_string()))
    }

    /// What the last `paint` cost in the shadow path.
    pub fn stat(&self) -> &PaintStat {
        &self.stat
    }

    /// Raw premultiplied RGBA8, for callers that would rather compare pixels.
    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }
}
