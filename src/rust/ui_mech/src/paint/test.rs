use super::*;
use crate::fill::{Fill, Shadow};

/// A painter over a scratch buffer, with the font stack in test mode (no system
/// fonts, so nothing here depends on what is installed).
fn painter<'a>(
    pixmap: &'a mut Pixmap,
    text: &'a mut FontStack,
    scale: f32,
    bound: Rect,
) -> Painter<'a> {
    // Leaked so the helper stays a one-liner at every call site; test-only, and the
    // process is about to end anyway.
    let stat = Box::leak(Box::new(crate::fill::PaintStat::default()));
    Painter::new(pixmap, text, stat, scale, bound)
}

/// Read a device pixel back as `(r, g, b)`. tiny-skia stores premultiplied RGBA;
/// every colour in these tests is opaque, so the stored bytes are the colour.
fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * pixmap.width() + x) * 4) as usize;
    let d = pixmap.data();
    (d[i], d[i + 1], d[i + 2])
}

const RED: Color = Color::hex(0xff0000);
const BLUE: Color = Color::hex(0x0000ff);

#[test]
fn scale_applies_once() {
    // 20x10 buffer at 2x: a 5x5 logical rect covers 10x10 device pixels.
    let mut pixmap = Pixmap::new(20, 10).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        2.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    p.fill_rect(Rect::new(0.0, 0.0, 5.0, 5.0), RED);
    assert_eq!(pixel(&pixmap, 9, 9), (255, 0, 0));
    assert_eq!(pixel(&pixmap, 10, 9), (0, 0, 0));
}

#[test]
fn offset_and_clip_nest() {
    let mut pixmap = Pixmap::new(40, 40).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    p.push_offset(Point::new(10.0, 10.0));
    p.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0)); // device 10,10..20,20
    // Draw far beyond the clip; only the clipped part may land.
    p.fill_rect(Rect::new(0.0, 0.0, 100.0, 100.0), RED);
    p.pop_clip();
    p.pop_offset();
    assert_eq!(pixel(&pixmap, 10, 10), (255, 0, 0));
    assert_eq!(pixel(&pixmap, 19, 19), (255, 0, 0));
    assert_eq!(pixel(&pixmap, 20, 20), (0, 0, 0)); // clipped
    assert_eq!(pixel(&pixmap, 5, 5), (0, 0, 0)); // offset
}

#[test]
fn clip_only_shrinks() {
    let mut pixmap = Pixmap::new(40, 40).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    p.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
    // A wider inner clip cannot widen the effective region.
    p.push_clip(Rect::new(0.0, 0.0, 30.0, 30.0));
    p.fill_rect(Rect::new(0.0, 0.0, 30.0, 30.0), BLUE);
    assert_eq!(pixel(&pixmap, 9, 9), (0, 0, 255));
    assert_eq!(pixel(&pixmap, 15, 15), (0, 0, 0));
}

#[test]
fn clip_query_returns_logical_coordinates() {
    let mut pixmap = Pixmap::new(80, 80).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        2.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.push_offset(Point::new(5.0, 5.0));
    p.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
    // Round-trips through device space and back into the caller's frame.
    assert_eq!(p.clip(), Rect::new(0.0, 0.0, 10.0, 10.0));
}

#[test]
fn path_fill_is_clipped() {
    let mut pixmap = Pixmap::new(40, 40).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    let mut o = Outline::new();
    o.move_to(Point::new(0.0, 0.0));
    o.line_to(Point::new(40.0, 0.0));
    o.line_to(Point::new(40.0, 40.0));
    o.close();
    let path = o.finish().unwrap();
    p.push_clip(Rect::new(0.0, 0.0, 20.0, 20.0));
    p.fill_path(&path, RED);
    // Inside the triangle and inside the clip.
    assert_eq!(pixel(&pixmap, 18, 5), (255, 0, 0));
    // Inside the triangle, outside the clip: the mask path must hold.
    assert_eq!(pixel(&pixmap, 30, 10), (0, 0, 0));
}

#[test]
fn degenerate_outline_yields_nothing() {
    assert!(Outline::new().finish().is_none());
}

#[test]
fn dirty_bound_limits_the_frame() {
    let mut pixmap = Pixmap::new(40, 40).unwrap();
    // Last frame: everything blue.
    {
        let mut text = FontStack::new(false);
        let mut p = painter(&mut pixmap, &mut text, 1.0, Rect::new(0.0, 0.0, 40.0, 40.0));
        p.clear(BLUE);
    }
    // This frame: only a corner is dirty, and the host paints as if it owned the
    // window. Everything outside the region must survive untouched — that is what
    // makes partial painting correct rather than merely cheap.
    {
        let mut text = FontStack::new(false);
        let mut p = painter(&mut pixmap, &mut text, 1.0, Rect::new(0.0, 0.0, 10.0, 10.0));
        p.clear(RED);
        p.fill_rect(Rect::new(0.0, 0.0, 40.0, 40.0), RED);
    }
    assert_eq!(pixel(&pixmap, 5, 5), (255, 0, 0));
    assert_eq!(pixel(&pixmap, 20, 20), (0, 0, 255));
}

#[test]
fn text_lands_inside_its_clip() {
    let mut pixmap = Pixmap::new(200, 60).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 200.0, 60.0),
    );
    p.clear(Color::hex(0x000000));
    let shaped = p.shape("888888888888", &TextStyle::numeric(20.0));
    // Clip to the left half; the run is wider than that.
    p.push_clip(Rect::new(0.0, 0.0, 60.0, 60.0));
    p.draw_text(&shaped, Point::new(0.0, 0.0), Color::hex(0xffffff));
    p.pop_clip();

    let lit = |x0: u32, x1: u32| {
        (x0..x1)
            .flat_map(|x| (0..60).map(move |y| (x, y)))
            .filter(|&(x, y)| pixel(&pixmap, x, y) != (0, 0, 0))
            .count()
    };
    assert!(lit(0, 60) > 50, "no text drawn inside the clip");
    assert_eq!(lit(60, 200), 0, "text escaped its clip");
}

#[test]
fn text_blends_rather_than_replacing() {
    // Antialiased edges must composite against what is already there, or every
    // glyph acquires a black halo on a coloured panel.
    let mut pixmap = Pixmap::new(120, 40).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 120.0, 40.0),
    );
    p.clear(BLUE);
    let shaped = p.shape("8", &TextStyle::numeric(28.0));
    p.draw_text(&shaped, Point::new(2.0, 2.0), RED);

    let mut partial = 0;
    for x in 0..120 {
        for y in 0..40 {
            let (r, g, b) = pixel(&pixmap, x, y);
            // A blended edge pixel is neither pure background nor pure ink.
            if r > 0 && b > 0 && g == 0 {
                partial += 1;
            }
        }
    }
    assert!(partial > 5, "found {partial} blended edge pixels");
}

#[test]
fn fills_snap_to_the_pixel_grid() {
    // At a fractional scale a rectangle's edges land between pixels; snapping is
    // what keeps a panel edge from softening into two grey rows.
    let mut pixmap = Pixmap::new(40, 40).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.25,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    // 10.0 * 1.25 = 12.5 device -> snapped to 13.
    p.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), RED);
    for x in 0..13 {
        assert_eq!(pixel(&pixmap, x, 0), (255, 0, 0), "column {x} not solid");
    }
    assert_eq!(
        pixel(&pixmap, 13, 0),
        (0, 0, 0),
        "bled past the snapped edge"
    );
}

#[test]
fn a_hairline_never_rounds_away() {
    // A sub-pixel rect keeps one pixel rather than vanishing: rules and separators
    // are exactly the things that are thinner than a device pixel at 1x.
    let mut pixmap = Pixmap::new(20, 20).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    p.fill_rect(Rect::new(2.0, 2.0, 10.0, 0.3), RED);
    assert_eq!(pixel(&pixmap, 5, 2), (255, 0, 0), "hairline disappeared");
}

#[test]
fn an_odd_width_rule_stays_one_crisp_row() {
    // A 1px horizontal rule at a fractional y must not straddle two rows.
    let mut pixmap = Pixmap::new(20, 20).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    p.stroke_line(Point::new(0.0, 8.3), Point::new(20.0, 8.3), RED, 1.0);
    assert_eq!(pixel(&pixmap, 10, 8), (255, 0, 0), "rule missed its row");
    assert_eq!(pixel(&pixmap, 10, 7), (0, 0, 0), "rule bled upward");
    assert_eq!(pixel(&pixmap, 10, 9), (0, 0, 0), "rule bled downward");
}

#[test]
fn a_gradient_runs_between_its_stops() {
    let mut pixmap = Pixmap::new(40, 40).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    p.fill_rect(
        Rect::new(0.0, 0.0, 40.0, 40.0),
        Fill::vertical(0.0, 40.0, vec![(0.0, RED), (1.0, BLUE)]),
    );
    let top = pixel(&pixmap, 20, 0);
    let mid = pixel(&pixmap, 20, 20);
    let bottom = pixel(&pixmap, 20, 39);
    assert!(top.0 > 200 && top.2 < 60, "top is not red: {top:?}");
    assert!(
        bottom.2 > 200 && bottom.0 < 60,
        "bottom is not blue: {bottom:?}"
    );
    assert!(mid.0 > 60 && mid.2 > 60, "middle did not blend: {mid:?}");
}

#[test]
fn a_gradient_is_anchored_to_the_shape_not_the_clip() {
    // Clipping a shape must not shift its shading, or a partially dirty frame
    // repaints a widget with different colours than the frame before it.
    let mut whole = Pixmap::new(40, 40).unwrap();
    let mut part = Pixmap::new(40, 40).unwrap();
    let mut text = FontStack::new(false);
    let grad = || Fill::vertical(0.0, 40.0, vec![(0.0, RED), (1.0, BLUE)]);
    {
        let mut p = painter(
            &mut whole,
            &mut text,
            1.0,
            Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
        );
        p.clear(Color::hex(0x000000));
        p.fill_rect(Rect::new(0.0, 0.0, 40.0, 40.0), grad());
    }
    {
        let mut p = painter(&mut part, &mut text, 1.0, Rect::new(0.0, 0.0, 1.0e6, 1.0e6));
        p.clear(Color::hex(0x000000));
        p.push_clip(Rect::new(0.0, 20.0, 40.0, 20.0));
        p.fill_rect(Rect::new(0.0, 0.0, 40.0, 40.0), grad());
        p.pop_clip();
    }
    for y in 20..40 {
        assert_eq!(
            pixel(&whole, 20, y),
            pixel(&part, 20, y),
            "row {y} shaded differently when clipped"
        );
    }
}

#[test]
fn an_outer_shadow_falls_outside_the_shape() {
    let mut pixmap = Pixmap::new(80, 80).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    p.shadow_round_rect(
        Rect::new(30.0, 30.0, 20.0, 20.0),
        4.0,
        &Shadow::outer(Point::new(0.0, 4.0), 4.0, Color::rgb(255, 255, 255)),
    );
    // Below the shape, where the offset pushes it, there is ink...
    let below = pixel(&pixmap, 40, 56);
    assert!(below.0 > 20, "no shadow below the shape: {below:?}");
    // ...and far away there is none.
    assert_eq!(pixel(&pixmap, 5, 5), (0, 0, 0), "shadow reached the corner");
}

#[test]
fn an_inset_shadow_stays_inside_the_shape() {
    let mut pixmap = Pixmap::new(80, 80).unwrap();
    let mut text = FontStack::new(false);
    let mut p = painter(
        &mut pixmap,
        &mut text,
        1.0,
        Rect::new(0.0, 0.0, 1.0e6, 1.0e6),
    );
    p.clear(Color::hex(0x000000));
    p.shadow_round_rect(
        Rect::new(20.0, 20.0, 40.0, 40.0),
        2.0,
        &Shadow::inset(Point::new(0.0, 3.0), 3.0, Color::rgb(255, 255, 255)),
    );
    // Ink hugs the inside of the top edge, where the offset silhouette is absent.
    let inside_top = pixel(&pixmap, 40, 22);
    assert!(
        inside_top.0 > 20,
        "no inset ink at the top edge: {inside_top:?}"
    );
    // Nothing escapes the shape.
    assert_eq!(
        pixel(&pixmap, 40, 15),
        (0, 0, 0),
        "inset shadow escaped upward"
    );
    assert_eq!(
        pixel(&pixmap, 10, 40),
        (0, 0, 0),
        "inset shadow escaped sideways"
    );
    // The middle stays clear — an inset shadow is a rim, not a wash.
    assert_eq!(
        pixel(&pixmap, 40, 45),
        (0, 0, 0),
        "inset shadow filled the shape"
    );
}

#[test]
fn shadow_cost_is_counted() {
    // The measurement that decides whether a cache is worth building: how many
    // shadows, and how many *distinct* geometries among them.
    let mut canvas = crate::Canvas::new(120, 120, 1.0).unwrap();
    canvas.paint(|p| {
        let s = Shadow::outer(Point::new(0.0, 2.0), 3.0, Color::rgba(0, 0, 0, 180));
        for n in 0..4 {
            p.shadow_round_rect(Rect::new(10.0 + n as f32 * 20.0, 10.0, 16.0, 16.0), 3.0, &s);
        }
        p.shadow_round_rect(Rect::new(10.0, 60.0, 60.0, 20.0), 3.0, &s);
    });
    assert_eq!(canvas.stat().shadow, 5);
    assert_eq!(
        canvas.stat().distinct(),
        2,
        "identical geometries counted twice"
    );
    assert!(canvas.stat().blur_pixel > 0);
}
