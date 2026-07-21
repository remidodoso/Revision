//! The pane, as pictures and as behaviour.
//!
//! The goldens pin geometry — reserved gutters, a proportional thumb, the
//! cluster with and without its slider. They cannot say whether any of it
//! *feels* right, which is what `app/src/bin/pane.rs` is for (ui-07 §10.19).

use rev_testkit::image;
use rev_ui_kit::pane::{BAR, BarPolicy, MIN_THUMB, Pane, SLIDER, Scale};
use rev_ui_kit::{Intent, Kind, Kit, Skin, Widget, WidgetId};
use rev_ui_mech::{
    Button, Canvas, Event, Modifier, Point, Pointer, PointerKind, Rect, Size, TargetId, UiTime,
};

const PANE: WidgetId = WidgetId(1);
const FRAME: Rect = Rect {
    x: 10.0,
    y: 10.0,
    w: 300.0,
    h: 200.0,
};

fn kit_with(pane: Pane, frame: Rect) -> Kit {
    let root =
        Widget::new(0, Kind::Panel, "Panel", Rect::new(0.0, 0.0, 400.0, 260.0)).with_child(vec![
            Widget::new(PANE.0, Kind::Pane { pane }, "Pane", frame),
        ]);
    let mut kit = Kit::new(root, Skin::default());
    kit.layout(Rect::new(0.0, 0.0, 400.0, 260.0));
    kit
}

fn big() -> Pane {
    Pane {
        extent: Size::new(1_000_000.0, 1_000_000.0),
        scale: Scale { x: 1.0, y: 1.0 },
        bar: BarPolicy::Both,
        ..Pane::default()
    }
}

fn pointer(kind: PointerKind, at: Point) -> Event {
    Event::Pointer(Pointer {
        kind,
        at,
        button: Some(Button::Left),
        modifier: Modifier::default(),
        time: UiTime(0.0),
    })
}

fn pane_of(kit: &Kit) -> Pane {
    match kit.kind(PANE) {
        Some(Kind::Pane { pane }) => *pane,
        _ => panic!("the pane should be a pane"),
    }
}

fn press(kit: &mut Kit, at: Point) -> Option<(WidgetId, Intent)> {
    kit.event(
        Some(TargetId(u64::from(PANE.0))),
        &pointer(PointerKind::Down, at),
    )
}

fn drag(kit: &mut Kit, at: Point) -> Option<(WidgetId, Intent)> {
    kit.event(
        Some(TargetId(u64::from(PANE.0))),
        &pointer(PointerKind::Move, at),
    )
}

fn release(kit: &mut Kit, at: Point) -> Option<(WidgetId, Intent)> {
    kit.event(
        Some(TargetId(u64::from(PANE.0))),
        &pointer(PointerKind::Up, at),
    )
}

#[test]
fn dragging_out_of_the_bar_snaps_back_and_coming_home_resumes() {
    // **The behaviour, exactly as the book has it and as the kit already had it
    // for every other control** (inventory §3a, p. 165). And it is *not* a
    // cancel: the snap-back is an ordinary offset update, so there is nothing to
    // revert and releasing outside is simply the end of the gesture.
    let mut kit = kit_with(big(), FRAME);
    let pane = pane_of(&kit);
    let thumb = pane
        .thumb(FRAME, rev_ui_kit::Axis::Vertical)
        .expect("a thumb");
    let x = thumb.x + thumb.w / 2.0;

    press(&mut kit, Point::new(x, thumb.y + 2.0));
    drag(&mut kit, Point::new(x, thumb.y + 60.0));
    let moved = pane_of(&kit).offset.y;
    assert!(moved > 0.0, "the drag should scroll: {moved}");

    // Well off to the side: back to where the drag began.
    let out = drag(&mut kit, Point::new(x - 200.0, thumb.y + 60.0));
    assert!(
        matches!(out, Some((_, Intent::Scrolled(_)))),
        "the snap-back is an offset update, not a cancel: {out:?}"
    );
    assert_eq!(pane_of(&kit).offset.y, 0.0, "back to the origin");

    // And back into the bar: it picks up where the pointer is.
    drag(&mut kit, Point::new(x, thumb.y + 60.0));
    let resumed = pane_of(&kit).offset.y;
    assert!(
        (resumed - moved).abs() < 0.001,
        "resumed: {resumed} vs {moved}"
    );

    // Releasing far away emits nothing further — the value is already correct.
    drag(&mut kit, Point::new(x - 200.0, thumb.y + 60.0));
    let out = release(&mut kit, Point::new(x - 200.0, thumb.y + 60.0));
    assert!(out.is_none(), "nothing to report on release: {out:?}");
    assert_eq!(pane_of(&kit).offset.y, 0.0);
}

#[test]
fn escape_does_not_touch_a_drag() {
    // Escape cancels an *uncommitted change to the document*. Navigation is not
    // that, and a scroll interrupted by a stray keypress would be absurd
    // (ui-07 §5.6).
    use rev_ui_mech::{Key, KeyCode, Named};

    let mut kit = kit_with(big(), FRAME);
    let pane = pane_of(&kit);
    let thumb = pane
        .thumb(FRAME, rev_ui_kit::Axis::Vertical)
        .expect("a thumb");
    let x = thumb.x + thumb.w / 2.0;
    // A tenth and a fifth of the *track*, not arbitrary pixel counts: the track
    // is short once the cluster has taken its end, and a 40-pixel drag pins the
    // thumb at the bottom, where a second drag can prove nothing.
    let track = pane
        .track(FRAME, rev_ui_kit::Axis::Vertical)
        .expect("a track");
    press(&mut kit, Point::new(x, thumb.y + 2.0));
    drag(&mut kit, Point::new(x, thumb.y + track.h * 0.1));
    let during = pane_of(&kit).offset.y;

    let escape = Event::Key(Key {
        code: KeyCode::Named(Named::Escape),
        pressed: true,
        repeat: false,
        modifier: Modifier::default(),
        time: UiTime(0.0),
    });
    kit.event(Some(TargetId(u64::from(PANE.0))), &escape);
    assert_eq!(pane_of(&kit).offset.y, during, "the view did not move");

    drag(&mut kit, Point::new(x, thumb.y + track.h * 0.2));
    assert!(pane_of(&kit).offset.y > during, "and the drag continues");
}

#[test]
fn clicking_the_gray_area_pages() {
    let mut kit = kit_with(big(), FRAME);
    let pane = pane_of(&kit);
    let thumb = pane
        .thumb(FRAME, rev_ui_kit::Axis::Vertical)
        .expect("a thumb");
    let viewport = pane.viewport(FRAME, rev_ui_kit::Axis::Vertical);

    press(
        &mut kit,
        Point::new(thumb.x + thumb.w / 2.0, thumb.bottom() + 20.0),
    );
    let after = pane_of(&kit).offset.y;
    assert!(after > 0.0, "it paged forward: {after}");
    assert!(
        after < viewport,
        "a page is a windowful less an overlap, not more: {after} vs {viewport}"
    );
}

#[test]
fn the_magnifier_buttons_zoom() {
    let mut kit = kit_with(big(), FRAME);
    let pane = pane_of(&kit);
    let (minus, plus) = pane
        .zoom_button(FRAME, rev_ui_kit::Axis::Vertical)
        .expect("buttons");

    let before = pane_of(&kit).scale.y;
    press(
        &mut kit,
        Point::new(plus.x + plus.w / 2.0, plus.y + plus.h / 2.0),
    );
    let zoomed_in = pane_of(&kit).scale.y;
    assert!(
        zoomed_in < before,
        "[+] means more magnification, so fewer units per pixel: {zoomed_in} vs {before}"
    );

    release(&mut kit, Point::new(plus.x, plus.y));
    press(
        &mut kit,
        Point::new(minus.x + minus.w / 2.0, minus.y + minus.h / 2.0),
    );
    assert!(pane_of(&kit).scale.y > zoomed_in, "[-] goes back out");
}

#[test]
fn the_wheel_scrolls_and_the_modifier_zooms_where_the_pointer_is() {
    let mut kit = kit_with(big(), FRAME);
    let inside = Point::new(FRAME.x + 100.0, FRAME.y + 80.0);
    let wheel = |modifier: Modifier, dy: f32| {
        Event::Pointer(Pointer {
            kind: PointerKind::Wheel { dx: 0.0, dy },
            at: inside,
            button: None,
            modifier,
            time: UiTime(0.0),
        })
    };

    // Far enough in that the anchor is not fighting the top edge. Zooming out
    // at a point near the origin *cannot* hold the content still — doing so
    // would need a negative offset, and clamping to the content wins. That is
    // correct, and the first version of this test mistook it for a defect.
    kit.event(
        Some(TargetId(u64::from(PANE.0))),
        &wheel(Modifier::default(), -300.0),
    );
    assert!(pane_of(&kit).offset.y > 0.0, "the wheel scrolls");

    let before = pane_of(&kit).to_content(FRAME, inside);
    let scale = pane_of(&kit).scale.y;
    kit.event(
        Some(TargetId(u64::from(PANE.0))),
        &wheel(
            Modifier {
                ctrl: true,
                ..Modifier::default()
            },
            -1.0,
        ),
    );
    assert!(pane_of(&kit).scale.y != scale, "the modifier zooms");
    let after = pane_of(&kit).to_content(FRAME, inside);
    assert!(
        (after.y - before.y).abs() < 1.0,
        "and holds the content under the pointer: {} moved to {}",
        before.y,
        after.y
    );
}

/// A pane with nothing to scroll: bars present, outlined, empty. The picture
/// that would catch anyone reintroducing auto-hiding.
#[test]
fn an_inactive_pane() {
    let kit = kit_with(
        Pane {
            extent: Size::new(10.0, 10.0),
            ..big()
        },
        FRAME,
    );
    let mut canvas = Canvas::new(400, 260, 1.0).unwrap();
    canvas.paint(|p| kit.paint(p));
    if let Err(e) = image::compare_png("pane_inactive", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

/// The ordinary case: both bars active, a thumb at its floor, and the full
/// cluster with its slider.
#[test]
fn a_scrolled_pane() {
    let mut pane = big();
    pane.offset = Point::new(120_000.0, 300_000.0);
    let kit = kit_with(pane, FRAME);
    let mut canvas = Canvas::new(400, 260, 1.0).unwrap();
    canvas.paint(|p| kit.paint(p));
    println!("pane: thumb floor {MIN_THUMB}, bar {BAR}");
    if let Err(e) = image::compare_png("pane_scrolled", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

/// The same pane at 1.25×, because every length in the pane is in logical
/// pixels and interface scale is the one thing that would betray a stray
/// device-pixel constant (R-938).
#[test]
fn a_scrolled_pane_at_125x() {
    let mut pane = big();
    pane.offset = Point::new(120_000.0, 300_000.0);
    let kit = kit_with(pane, FRAME);
    let mut canvas = Canvas::new(500, 325, 1.25).unwrap();
    canvas.paint(|p| kit.paint(p));
    if let Err(e) = image::compare_png("pane_125x", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

/// Too cramped for a slider: the cluster collapses to `[-][+]` and **the buttons
/// do not move**. Degradation is subtractive (ui-07 §6.4).
#[test]
fn a_cramped_cluster() {
    let narrow = Rect::new(10.0, 10.0, 2.0 * BAR + SLIDER + BAR + 20.0, 200.0);
    let kit = kit_with(big(), narrow);
    let mut canvas = Canvas::new(400, 260, 1.0).unwrap();
    canvas.paint(|p| kit.paint(p));
    if let Err(e) = image::compare_png("pane_cramped", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

#[test]
fn hovering_lights_the_furniture_and_never_the_content() {
    // A control highlights because it is about to do something. Content is not
    // offering to do anything, so it stays as it is.
    let mut kit = kit_with(big(), FRAME);
    let pane = pane_of(&kit);
    let (_, plus) = pane
        .zoom_button(FRAME, rev_ui_kit::Axis::Vertical)
        .expect("buttons");

    let hover = |kit: &mut Kit, at: Point| {
        kit.event(
            Some(TargetId(u64::from(PANE.0))),
            &Event::Pointer(Pointer {
                kind: PointerKind::Move,
                at,
                button: None,
                modifier: Modifier::default(),
                time: UiTime(0.0),
            }),
        );
    };

    hover(
        &mut kit,
        Point::new(plus.x + plus.w / 2.0, plus.y + plus.h / 2.0),
    );
    assert_eq!(
        pane.part_at(FRAME, Point::new(plus.x + 2.0, plus.y + 2.0)),
        Some(rev_ui_kit::pane::Part::ZoomIn(rev_ui_kit::Axis::Vertical))
    );

    // The interior is not a part at all.
    let inside = Point::new(FRAME.x + 40.0, FRAME.y + 40.0);
    hover(&mut kit, inside);
    assert_eq!(
        pane.part_at(FRAME, inside),
        None,
        "content is not a control"
    );
}

#[test]
fn the_wheel_takes_its_axis_from_what_it_is_over() {
    // The kit's own rule — "the wheel aims where you are looking" — reaching the
    // pane. Rolling over the *horizontal* cluster zooms the horizontal axis,
    // even though the wheel is the vertical input: the control names the axis
    // and the wheel only supplies the amount.
    let mut kit = kit_with(big(), Rect::new(10.0, 10.0, 340.0, 220.0));
    let rect = kit.rect(PANE).expect("placed");
    let pane = pane_of(&kit);

    let roll = |kit: &mut Kit, at: Point| {
        kit.event(
            Some(TargetId(u64::from(PANE.0))),
            &Event::Pointer(Pointer {
                kind: PointerKind::Wheel { dx: 0.0, dy: -1.0 },
                at,
                button: None,
                modifier: Modifier::default(),
                time: UiTime(0.0),
            }),
        );
    };

    let (_, plus) = pane
        .zoom_button(rect, rev_ui_kit::Axis::Horizontal)
        .expect("buttons");
    let before = pane_of(&kit).scale;
    roll(
        &mut kit,
        Point::new(plus.x + plus.w / 2.0, plus.y + plus.h / 2.0),
    );
    let after = pane_of(&kit).scale;
    assert!(after.x != before.x, "the horizontal cluster zoomed time");
    assert_eq!(after.y, before.y, "and left pitch alone");

    // And over the horizontal *bar*, the wheel scrolls horizontally.
    let track = pane
        .track(rect, rev_ui_kit::Axis::Horizontal)
        .expect("a track");
    let was = pane_of(&kit).offset;
    roll(
        &mut kit,
        Point::new(track.right() - 10.0, track.y + track.h / 2.0),
    );
    let now = pane_of(&kit).offset;
    assert!(now.x != was.x, "the horizontal bar scrolled across");
    assert_eq!(now.y, was.y, "and not down");
}
