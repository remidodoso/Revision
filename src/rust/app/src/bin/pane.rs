//! rev-pane — the scrollable pane, operable, before either real consumer exists.
//!
//! **Why a demo and not only tests** (ui-07 §10.19). Golden screenshots pin
//! geometry, and geometry is the part reasoning gets right. Whether the
//! snap-back tolerance is too tight, whether pointer-anchored zoom tracks the
//! way a hand expects, whether a proportional thumb reads at a glance — none of
//! that is assertable, and eng-07's lesson was that 331 green tests said nothing
//! about a property no test named. This is where those get answered.
//!
//! **And it is a third consumer, deliberately.** ui-05 is a log window and ui-06
//! a piano roll; a pane validated only against whichever arrived first would be
//! shaped for it by accident. A synthetic third keeps it honest.
//!
//! What it draws: a ruled grid **with its coordinates written in it**, so
//! "the content under the pointer does not move when zooming" is checkable by
//! eye, and an absurd extent, so `MIN_THUMB` engages and the travel arithmetic
//! is exercised rather than assumed.

use rev_ui_kit::pane::{BarPolicy, Pane, Scale};
use rev_ui_kit::{Anchor, Intent, Kind, Kit, PaneArtist, Skin, Widget, WidgetId};
use rev_ui_mech::{
    Color, Event, Frame, Host, Mech, Notice, Painter, Point, Rect, Size, TargetId, TextStyle, Tree,
    WindowId, WindowSpec,
};

const BOTH: WidgetId = WidgetId(1);
const TALL: WidgetId = WidgetId(2);

/// A million units square. Large enough that the thumb hits its floor and the
/// "end of the content is unreachable" bug would be visible rather than
/// theoretical.
const EXTENT: f32 = 1_000_000.0;

/// The scene, as a description — the same discipline as the Control Bar: a
/// layout designer could edit this, and nothing here paints.
fn scene(size: Size) -> Widget {
    let gap = 12.0;
    let wide = ((size.w - gap * 3.0) * 0.62).max(120.0);
    let pane = |extent: Size, bar: BarPolicy| Pane {
        extent,
        bar,
        scale: Scale { x: 1.0, y: 1.0 },
        scale_min: 1.0 / 16.0,
        scale_max: 4_096.0,
        ..Pane::default()
    };
    Widget::new(0, Kind::Panel, "", Rect::new(0.0, 0.0, size.w, size.h)).with_child(vec![
        Widget::new(
            BOTH.0,
            Kind::Pane {
                pane: pane(Size::new(EXTENT, EXTENT), BarPolicy::Both),
            },
            "Both axes",
            Rect::new(gap, gap, wide, size.h - gap * 2.0 - 24.0),
        )
        .with_anchor(Anchor::FILL),
        Widget::new(
            TALL.0,
            Kind::Pane {
                pane: pane(Size::new(0.0, EXTENT), BarPolicy::Vertical),
            },
            "Vertical only",
            Rect::new(
                gap * 2.0 + wide,
                gap,
                (size.w - wide - gap * 3.0).max(80.0),
                size.h - gap * 2.0 - 24.0,
            ),
        )
        .with_anchor(Anchor {
            left: false,
            top: true,
            right: true,
            bottom: true,
        }),
    ])
}

/// The grid, drawn in content coordinates and labelled.
struct Grid;

impl PaneArtist for Grid {
    fn paint(&mut self, _: WidgetId, pane: &Pane, interior: Rect, p: &mut Painter) {
        let ink = Color::rgba(150, 160, 180, 255);
        let faint = Color::rgba(70, 78, 92, 255);
        let major = Color::rgba(210, 170, 80, 255);

        // A grid step that stays legible at every zoom: the smallest power of
        // ten that is at least 60 pixels apart. Without this the lines either
        // merge into a smear or vanish entirely, and neither tells you anything.
        let step = |scale: f32| {
            let mut step = 1.0f32;
            while step / scale < 60.0 {
                step *= 10.0;
            }
            step
        };
        let sx = step(pane.scale.x);
        let sy = step(pane.scale.y);

        let mut y = (pane.offset.y / sy).floor() * sy;
        while y < pane.offset.y + interior.h * pane.scale.y {
            let at = interior.y + (y - pane.offset.y) / pane.scale.y;
            let tenth = (y / (sy * 10.0)).round() * sy * 10.0;
            let heavy = (y - tenth).abs() < sy / 2.0;
            p.stroke_line(
                Point::new(interior.x, at),
                Point::new(interior.right(), at),
                if heavy { major } else { faint },
                1.0,
            );
            if heavy {
                let label = p.shape(&format!("{y:.0}"), &TextStyle::numeric(12.0));
                p.draw_text(&label, Point::new(interior.x + 4.0, at + 2.0), ink);
            }
            y += sy;
        }

        let mut x = (pane.offset.x / sx).floor() * sx;
        while x < pane.offset.x + interior.w * pane.scale.x {
            let at = interior.x + (x - pane.offset.x) / pane.scale.x;
            let tenth = (x / (sx * 10.0)).round() * sx * 10.0;
            let heavy = (x - tenth).abs() < sx / 2.0;
            p.stroke_line(
                Point::new(at, interior.y),
                Point::new(at, interior.bottom()),
                if heavy { major } else { faint },
                1.0,
            );
            if heavy && pane.extent.w > 0.0 {
                let label = p.shape(&format!("{x:.0}"), &TextStyle::numeric(12.0));
                p.draw_text(&label, Point::new(at + 3.0, interior.y + 14.0), ink);
            }
            x += sx;
        }
    }
}

struct Demo {
    kit: Kit,
    window: Option<WindowId>,
    /// What the pointer is over, in content units — printed so that
    /// pointer-anchored zoom can be *watched* rather than trusted.
    under: Option<(f32, f32)>,
    said: String,
}

impl Demo {
    fn new() -> Demo {
        Demo {
            kit: Kit::new(scene(Size::new(1100.0, 700.0)), Skin::default()),
            window: None,
            under: None,
            said: String::from("wheel scrolls · tilt scrolls across · ctrl+wheel zooms"),
        }
    }

    fn layout(&mut self, size: Size) {
        self.kit.layout(Rect::new(0.0, 0.0, size.w, size.h));
    }
}

impl Host for Demo {
    fn start(&mut self, mech: &mut Mech) {
        self.window = Some(mech.open_window(WindowSpec {
            title: String::from("Revision — pane"),
            size: Size::new(1100.0, 700.0),
            ..WindowSpec::default()
        }));
    }

    fn notice(&mut self, window: WindowId, notice: &Notice, mech: &mut Mech) {
        match notice {
            Notice::CloseRequested => {
                mech.close_window(window);
                mech.exit();
            }
            Notice::Resized(size) => {
                self.layout(*size);
                mech.mark_dirty_all(window);
            }
            _ => {}
        }
    }

    fn hit(&self, _: WindowId, at: Point) -> Option<TargetId> {
        self.kit.hit(at)
    }

    fn a11y(&self, _: WindowId) -> Tree {
        self.kit.a11y()
    }

    fn event(&mut self, window: WindowId, target: Option<TargetId>, ev: &Event, mech: &mut Mech) {
        // Where the pointer is, in content units. The number on screen is what
        // makes "the content under the pointer stayed put" an observation.
        if let Event::Pointer(p) = ev
            && let Some(rect) = self.kit.rect(BOTH)
            && let Some(Kind::Pane { pane }) = self.kit.kind(BOTH)
        {
            let at = pane.to_content(rect, p.at);
            self.under = rect.contains(p.at).then_some((at.x, at.y));
        }
        if let Some((id, intent)) = self.kit.event(target, ev) {
            self.said = match intent {
                Intent::Scrolled(offset) => {
                    format!("scrolled {} to {:.0}, {:.0}", id.0, offset.x, offset.y)
                }
                Intent::Zoomed(scale) => {
                    format!("zoomed {} to {:.3}, {:.3} units/px", id.0, scale.x, scale.y)
                }
                other => format!("{other:?} on {}", id.0),
            };
        }
        for rect in self.kit.take_dirty() {
            mech.mark_dirty(window, rect);
        }
        mech.mark_dirty_all(window);
    }

    fn paint(&mut self, _: WindowId, frame: &mut Frame<'_>) {
        let (background, ink) = {
            let s = self.kit.skin();
            (s.panel_lo, s.ink)
        };
        frame.paint.clear(background);
        self.kit.paint_with(&mut frame.paint, &mut Grid);

        let mut line = self.said.clone();
        if let Some((x, y)) = self.under {
            line = format!("{line}    ·    pointer at {x:.1}, {y:.1}");
        }
        let shaped = frame.paint.shape(&line, &TextStyle::numeric(13.0));
        let at = Point::new(14.0, frame.size.h - 20.0);
        frame.paint.draw_text(&shaped, at, ink);
    }
}

fn main() -> Result<(), rev_ui_mech::MechError> {
    rev_ui_mech::run(Demo::new())
}
