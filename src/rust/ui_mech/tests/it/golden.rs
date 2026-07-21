//! Screenshot references for the paint list and the text stack.
//!
//! The scene is deliberately dense in the things that are easy to get subtly
//! wrong — antialiased curves, clipped paths, strokes at fractional widths, both
//! font roles, real italic and bold — because a reference image is only worth what
//! it would catch.

use rev_testkit::image;
use rev_ui_mech::{Canvas, Color, Fill, Outline, Painter, Point, Rect, Shadow, TextStyle};

const BACKGROUND: Color = Color::hex(0x202428);
const PANEL: Color = Color::hex(0x30363c);
const EDGE: Color = Color::hex(0x4a5058);
const RULE: Color = Color::hex(0x5a9fd4);
const INK: Color = Color::hex(0xd8dde3);
const LIT: Color = Color::hex(0xf0d070);

/// Everything the paint vocabulary can do, laid out so a change to any of it moves
/// pixels. Logical coordinates throughout; the caller chooses the scale.
fn scene(p: &mut Painter) {
    p.clear(BACKGROUND);
    p.fill_round_rect(Rect::new(8.0, 8.0, 384.0, 224.0), 6.0, PANEL);
    p.fill_rect(Rect::new(8.0, 44.0, 384.0, 2.0), RULE);

    // Text: both roles, both weights, real italic.
    let title = p.shape("Revision", &TextStyle::ui(20.0).bold());
    p.draw_text(&title, Point::new(20.0, 14.0), INK);
    let counter = p.shape("012|03|0000", &TextStyle::numeric(24.0));
    p.draw_text(&counter, Point::new(20.0, 56.0), LIT);
    let italic = p.shape("Brass Section — 16-ET", &TextStyle::ui(14.0).italic());
    p.draw_text(&italic, Point::new(20.0, 92.0), INK);

    // A clipped disc: exercises the mask path, and its clipped corner must stay
    // square while its curve stays smooth.
    p.push_offset(Point::new(20.0, 120.0));
    p.push_clip(Rect::new(0.0, 0.0, 60.0, 60.0));
    p.fill_path(&disc(Point::new(60.0, 60.0), 52.0), RULE);
    p.pop_clip();
    p.pop_offset();

    // Strokes at fractional widths, where rounding errors show up first.
    for (n, width) in [0.5f32, 1.0, 1.5, 2.0, 3.0].iter().enumerate() {
        let y = 130.0 + n as f32 * 18.0;
        p.stroke_line(
            Point::new(100.0, y),
            Point::new(240.0, y + 12.0),
            EDGE,
            *width,
        );
    }

    // Nested clips, to prove they only ever shrink.
    p.push_clip(Rect::new(250.0, 120.0, 100.0, 100.0));
    p.push_clip(Rect::new(250.0, 120.0, 400.0, 400.0));
    p.fill_round_rect(Rect::new(250.0, 120.0, 400.0, 400.0), 10.0, LIT);
    p.pop_clip();
    p.pop_clip();
}

/// A circle as four cubic quarter-arcs.
fn disc(center: Point, r: f32) -> rev_ui_mech::Path {
    let (cx, cy, k) = (center.x, center.y, r * 0.552_284_8);
    let mut o = Outline::new();
    o.move_to(Point::new(cx, cy - r));
    o.cubic_to(
        Point::new(cx + k, cy - r),
        Point::new(cx + r, cy - k),
        Point::new(cx + r, cy),
    );
    o.cubic_to(
        Point::new(cx + r, cy + k),
        Point::new(cx + k, cy + r),
        Point::new(cx, cy + r),
    );
    o.cubic_to(
        Point::new(cx - k, cy + r),
        Point::new(cx - r, cy + k),
        Point::new(cx - r, cy),
    );
    o.cubic_to(
        Point::new(cx - r, cy - k),
        Point::new(cx - k, cy - r),
        Point::new(cx, cy - r),
    );
    o.close();
    o.finish().expect("disc is non-degenerate")
}

#[test]
fn paint_and_text_at_1x() {
    let mut canvas = Canvas::new(400, 240, 1.0).unwrap();
    canvas.paint(scene);
    if let Err(e) = image::compare_png("scene_1x", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

#[test]
fn paint_and_text_at_2x() {
    // The same scene at 2x must be the same drawing, not a different one: this is
    // where a stray stored scale factor or a double-applied DPI would show up.
    let mut canvas = Canvas::new(800, 480, 2.0).unwrap();
    canvas.paint(scene);
    if let Err(e) = image::compare_png("scene_2x", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

#[test]
fn rendering_is_reproducible_within_a_run() {
    // Two canvases, same scene, byte-identical — the property that makes the
    // references meaningful in the first place.
    let mut a = Canvas::new(400, 240, 1.0).unwrap();
    let mut b = Canvas::new(400, 240, 1.0).unwrap();
    a.paint(scene);
    b.paint(scene);
    assert_eq!(a.data(), b.data(), "identical scenes rendered differently");
}

#[test]
fn paint_and_text_at_125x() {
    // The case the other two miss entirely: at an integer scale every edge already
    // lands on the grid, so pixel snapping is invisible and untested. A fractional
    // interface scale (R-938) is where a panel edge would soften into two grey rows
    // and a one-pixel rule would disappear.
    let mut canvas = Canvas::new(500, 300, 1.25).unwrap();
    canvas.paint(scene);
    if let Err(e) = image::compare_png("scene_125x", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

/// Gradients and shadows — the machined look the exhibits are built from, reduced
/// to the primitives that produce it. A separate scene rather than an extension of
/// the first, so the existing references keep guarding exactly what they guarded.
fn chrome(p: &mut Painter) {
    p.clear(BACKGROUND);

    // A raised panel: outer shadow, then the panel, then its 1px top highlight —
    // which is a hairline, not a blur, and always was.
    p.shadow_round_rect(
        Rect::new(16.0, 16.0, 368.0, 128.0),
        8.0,
        &Shadow::outer(Point::new(0.0, 6.0), 12.0, Color::rgba(0, 0, 0, 140)),
    );
    p.fill_round_rect(Rect::new(16.0, 16.0, 368.0, 128.0), 8.0, PANEL);
    p.fill_rect(Rect::new(24.0, 16.0, 352.0, 1.0), Color::hex(0x454c54));

    // Slider slots: a recessed groove is an inset shadow inside a dark fill.
    for n in 0..5 {
        let x = 40.0 + n as f32 * 36.0;
        let slot = Rect::new(x, 36.0, 6.0, 88.0);
        p.fill_round_rect(slot, 3.0, Color::hex(0x171a1e));
        p.shadow_round_rect(
            slot,
            3.0,
            &Shadow::inset(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 200)),
        );

        // The cap: a three-stop vertical gradient with its own drop shadow — the
        // exhibits' locked slider-cap spec, in primitives.
        let cap = Rect::new(x - 12.0, 44.0 + n as f32 * 14.0, 30.0, 15.0);
        p.shadow_round_rect(
            cap,
            2.0,
            &Shadow::outer(Point::new(0.0, 2.0), 3.0, Color::rgba(0, 0, 0, 160)),
        );
        p.fill_round_rect(
            cap,
            2.0,
            Fill::vertical(
                0.0,
                15.0,
                vec![
                    (0.0, Color::hex(0x6a707a)),
                    (0.46, Color::hex(0x2f343a)),
                    (1.0, Color::hex(0x454c55)),
                ],
            ),
        );
        // The cap's centre line.
        p.fill_rect(Rect::new(cap.x + 2.0, cap.y + 7.0, cap.w - 4.0, 1.0), INK);
    }

    // A readout in the amber the exhibits use for values, with tabular figures.
    let value = p.shape("  0.00 dB", &TextStyle::numeric(15.0));
    p.draw_text(&value, Point::new(232.0, 60.0), LIT);
    let label = p.shape("Output", &TextStyle::ui(13.0));
    p.draw_text(&label, Point::new(232.0, 42.0), Color::hex(0x8a929b));

    // A lit lamp: a small filled disc over its own glow.
    p.shadow_round_rect(
        Rect::new(340.0, 44.0, 14.0, 14.0),
        7.0,
        &Shadow::outer(Point::new(0.0, 0.0), 7.0, Color::rgba(240, 90, 80, 220)),
    );
    p.fill_round_rect(
        Rect::new(340.0, 44.0, 14.0, 14.0),
        7.0,
        Color::hex(0xf05a50),
    );
}

#[test]
fn chrome_at_1x() {
    let mut canvas = Canvas::new(400, 160, 1.0).unwrap();
    canvas.paint(chrome);
    println!("chrome 1x: {}", canvas.stat().summary());
    if let Err(e) = image::compare_png("chrome_1x", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

#[test]
fn chrome_at_125x() {
    let mut canvas = Canvas::new(500, 200, 1.25).unwrap();
    canvas.paint(chrome);
    println!("chrome 1.25x: {}", canvas.stat().summary());
    if let Err(e) = image::compare_png("chrome_125x", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}
