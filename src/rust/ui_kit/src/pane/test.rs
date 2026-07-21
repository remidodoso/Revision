use super::*;

const RECT: Rect = Rect {
    x: 10.0,
    y: 20.0,
    w: 400.0,
    h: 300.0,
};

fn pane(extent: Size) -> Pane {
    Pane {
        extent,
        ..Pane::default()
    }
}

#[test]
fn the_interior_does_not_depend_on_whether_there_is_anything_to_scroll() {
    // The point of reserving: an empty pane and a full one have the *same*
    // interior, so content never reflows because a bar appeared.
    let empty = pane(Size::new(10.0, 10.0)).interior(RECT);
    let full = pane(Size::new(100_000.0, 100_000.0)).interior(RECT);
    assert_eq!(empty, full);
    assert_eq!(empty.w, RECT.w - BAR);
    assert_eq!(empty.h, RECT.h - BAR);
}

#[test]
fn a_bar_with_nothing_to_scroll_is_inactive_and_still_there() {
    let pane = pane(Size::new(10.0, 10.0));
    assert!(pane.gutter(RECT, Axis::Vertical).is_some(), "still there");
    assert!(!pane.active(RECT, Axis::Vertical), "but inactive");
    assert!(pane.thumb(RECT, Axis::Vertical).is_none(), "and no thumb");
}

#[test]
fn only_the_declared_axes_reserve() {
    let mut pane = pane(Size::new(1000.0, 1000.0));
    pane.bar = BarPolicy::Vertical;
    assert_eq!(pane.interior(RECT).h, RECT.h, "no horizontal gutter");
    assert_eq!(pane.interior(RECT).w, RECT.w - BAR);
    assert!(pane.gutter(RECT, Axis::Horizontal).is_none());
}

#[test]
fn the_thumb_is_proportional() {
    let track = pane(Size::new(0.0, 0.0))
        .track(RECT, Axis::Vertical)
        .expect("a track")
        .h;
    let viewport = pane(Size::new(0.0, 0.0)).viewport(RECT, Axis::Vertical);

    // Half the content in view: half the track.
    let half = pane(Size::new(0.0, viewport * 2.0));
    let thumb = half.thumb(RECT, Axis::Vertical).expect("active").h;
    assert!(
        (thumb - track / 2.0).abs() < 0.5,
        "half the content should give half a thumb: {thumb} of {track}"
    );

    // A quarter in view: a quarter of the track. The claim is the *ratio*, so
    // it has to hold at more than one point.
    let quarter = pane(Size::new(0.0, viewport * 4.0));
    let thumb = quarter.thumb(RECT, Axis::Vertical).expect("active").h;
    assert!((thumb - track / 4.0).abs() < 0.5, "{thumb} of {track}");
}

#[test]
fn the_thumb_has_a_floor_and_the_end_is_still_reachable() {
    // **The bug this test exists for.** With a floor, travel is `track − thumb`,
    // not `track − MIN_THUMB`. Get it wrong and the last part of a long document
    // cannot be reached — invisible in short content, permanent in long.
    let mut pane = pane(Size::new(0.0, 1_000_000.0));
    let track = pane.track(RECT, Axis::Vertical).expect("a track");
    let thumb = pane.thumb(RECT, Axis::Vertical).expect("active");
    assert!(
        (thumb.h - MIN_THUMB).abs() < 0.001,
        "the floor should engage: {}",
        thumb.h
    );

    pane.offset.y = f32::MAX;
    pane.clamp(RECT);
    let end = pane.thumb(RECT, Axis::Vertical).expect("active");
    assert!(
        (end.bottom() - track.bottom()).abs() < 0.5,
        "at the end of the content the thumb should reach the end of the track: \
         {} vs {}",
        end.bottom(),
        track.bottom()
    );
}

#[test]
fn dragging_the_thumb_to_the_bottom_reaches_the_bottom() {
    // The same claim from the other side: the pointer position that puts the
    // thumb at the end must produce the maximum offset.
    let pane = pane(Size::new(0.0, 1_000_000.0));
    let track = pane.track(RECT, Axis::Vertical).expect("a track");
    let offset = pane.offset_for_thumb(
        RECT,
        Axis::Vertical,
        Point::new(0.0, track.bottom() + 500.0),
        0.0,
    );
    let limit = pane.extent.h - pane.viewport(RECT, Axis::Vertical);
    assert!((offset - limit).abs() < 0.5, "{offset} vs {limit}");
}

#[test]
fn scrolling_cannot_leave_the_content() {
    let mut pane = pane(Size::new(0.0, 2_000.0));
    pane.scroll_by(RECT, 0.0, -5_000.0);
    assert_eq!(pane.offset.y, 0.0, "not above the top");
    pane.scroll_by(RECT, 0.0, 50_000.0);
    let limit = pane.extent.h - pane.viewport(RECT, Axis::Vertical);
    assert!((pane.offset.y - limit).abs() < 0.001, "not past the bottom");
}

#[test]
fn a_page_keeps_a_unit_of_overlap() {
    // The book's rule (inventory 3a, p. 164): a windowful *minus at least one
    // unit*, so the reader keeps a reference point across the jump.
    let pane = pane(Size::new(0.0, 10_000.0));
    let viewport = pane.viewport(RECT, Axis::Vertical);
    let page = pane.page(RECT, Axis::Vertical, 20.0);
    assert!(page < viewport, "a page is less than a windowful");
    assert!((page - (viewport - 20.0)).abs() < 0.001);
}

#[test]
fn zoom_holds_the_content_under_the_anchor() {
    // The property that decides whether zoom feels like a lens or a jump, and
    // the reason the demo draws its coordinates: this is checkable by eye there
    // and by assertion here.
    for factor in [ZOOM_STEP, 1.0 / ZOOM_STEP, 4.0, 0.25] {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let mut pane = pane(Size::new(50_000.0, 50_000.0));
            pane.offset = Point::new(1_234.0, 5_678.0);
            let anchor = Point::new(RECT.x + 137.0, RECT.y + 91.0);
            let before = pane.to_content(RECT, anchor);
            pane.zoom(RECT, axis, factor, anchor);
            let after = pane.to_content(RECT, anchor);
            let (b, a) = match axis {
                Axis::Horizontal => (before.x, after.x),
                Axis::Vertical => (before.y, after.y),
            };
            assert!(
                (a - b).abs() < 0.5,
                "{axis:?} at {factor}: {b} moved to {a}"
            );
        }
    }
}

#[test]
fn zoom_stays_inside_its_limits() {
    let mut pane = pane(Size::new(50_000.0, 50_000.0));
    let anchor = Point::new(RECT.x, RECT.y);
    for _ in 0..500 {
        pane.zoom(RECT, Axis::Vertical, ZOOM_STEP, anchor);
    }
    assert!(pane.scale.y >= pane.scale_min - f32::EPSILON);
    for _ in 0..1000 {
        pane.zoom(RECT, Axis::Vertical, 1.0 / ZOOM_STEP, anchor);
    }
    assert!(pane.scale.y <= pane.scale_max + f32::EPSILON);
}

#[test]
fn equal_slider_travel_is_equal_ratio() {
    // Logarithmic position: the reason creeping the control feels even instead
    // of exploding at one end.
    let mut pane = pane(Size::new(50_000.0, 50_000.0));
    let anchor = Point::new(RECT.x, RECT.y);
    let scale_at = |position: f32| {
        let mut p = pane;
        p.set_zoom_position(RECT, Axis::Vertical, position, anchor);
        p.scale.y
    };
    let a = scale_at(0.2) / scale_at(0.3);
    let b = scale_at(0.6) / scale_at(0.7);
    assert!(
        (a - b).abs() < 0.01,
        "equal travel gave unequal ratios: {a} vs {b}"
    );

    // And the round trip: a position set is a position read.
    for position in [0.0, 0.25, 0.5, 0.75, 1.0] {
        pane.set_zoom_position(RECT, Axis::Vertical, position, anchor);
        let read = pane.zoom_position(Axis::Vertical);
        assert!(
            (read - position).abs() < 0.01,
            "{position} read back {read}"
        );
    }
}

#[test]
fn pushing_toward_plus_zooms_in() {
    // Direction is the thing a magnifier glyph promises, so it had better be
    // true: a higher slider position means more magnification, which means
    // *fewer* content units per pixel.
    let mut pane = pane(Size::new(50_000.0, 50_000.0));
    let anchor = Point::new(RECT.x, RECT.y);
    pane.set_zoom_position(RECT, Axis::Vertical, 0.2, anchor);
    let out = pane.scale.y;
    pane.set_zoom_position(RECT, Axis::Vertical, 0.8, anchor);
    assert!(
        pane.scale.y < out,
        "{} should be less than {out}",
        pane.scale.y
    );
}

#[test]
fn the_spaces_round_trip() {
    let mut pane = pane(Size::new(50_000.0, 50_000.0));
    pane.offset = Point::new(900.0, 400.0);
    pane.scale = Scale { x: 3.0, y: 0.25 };
    for at in [
        Point::new(RECT.x, RECT.y),
        Point::new(RECT.x + 200.0, RECT.y + 100.0),
        Point::new(RECT.x + 399.0, RECT.y + 299.0),
    ] {
        let there = pane.to_content(RECT, at);
        let back = pane.to_window(RECT, there);
        assert!((back.x - at.x).abs() < 0.01 && (back.y - at.y).abs() < 0.01);
    }
}

#[test]
fn the_cluster_sits_at_the_far_end_and_minus_leads() {
    let pane = pane(Size::new(50_000.0, 50_000.0));

    let gutter = pane.gutter(RECT, Axis::Horizontal).expect("a gutter");
    let (minus, plus) = pane.zoom_button(RECT, Axis::Horizontal).expect("buttons");
    assert!((plus.right() - gutter.right()).abs() < 0.001, "far end");
    assert!(minus.x < plus.x, "[-] then [+], left to right");

    // Vertical is that rotated 90 degrees clockwise: minus *above* plus. It
    // departs from ch. 7 p. 214's up-means-more deliberately (inventory 7).
    let gutter = pane.gutter(RECT, Axis::Vertical).expect("a gutter");
    let (minus, plus) = pane.zoom_button(RECT, Axis::Vertical).expect("buttons");
    assert!((plus.bottom() - gutter.bottom()).abs() < 0.001, "far end");
    assert!(minus.y < plus.y, "[-] above [+]");
}

#[test]
fn the_cluster_degrades_without_moving_its_buttons() {
    // Subtractive, not substitutive: the slider goes, the buttons stay exactly
    // where they were relative to the end of the bar. That is what keeps the
    // target in the same place at every window size.
    let pane = pane(Size::new(50_000.0, 50_000.0));
    let wide = Rect::new(0.0, 0.0, 600.0, 300.0);
    // Just too short to afford a track and a slider both.
    let narrow = Rect::new(0.0, 0.0, BAR + MIN_TRACK + 2.0 * BAR + SLIDER - 1.0, 300.0);

    assert!(pane.zoom_slider(wide, Axis::Horizontal).is_some());
    assert!(pane.zoom_slider(narrow, Axis::Horizontal).is_none());

    // The buttons stay at the far end of the *gutter* — which stops where the
    // vertical bar's gutter begins, not at the pane's edge.
    for rect in [wide, narrow] {
        let gutter = pane.gutter(rect, Axis::Horizontal).expect("a gutter");
        let (_, plus) = pane.zoom_button(rect, Axis::Horizontal).expect("buttons");
        assert!((gutter.right() - plus.right()).abs() < 0.001, "{rect:?}");
        assert_eq!(plus.w, BAR, "and the same size");
    }
}

#[test]
fn the_track_keeps_a_usable_length_at_every_width() {
    // The failure this guards: an earlier version gave the slider whatever was
    // left of the gutter, which at ordinary sizes left the scroll track exactly
    // zero pixels long. The gutter is the scroll bar's; the cluster is a guest.
    for width in [80.0f32, 120.0, 300.0, 1200.0] {
        let rect = Rect::new(0.0, 0.0, width, 200.0);
        let pane = pane(Size::new(50_000.0, 50_000.0));
        let track = pane.track(rect, Axis::Horizontal).expect("a track");
        if pane.zoom_slider(rect, Axis::Horizontal).is_some() {
            assert!(track.w >= MIN_TRACK, "starved track at width {width}");
        }
    }
    for width in [80.0f32, 120.0, 300.0, 1200.0] {
        let rect = Rect::new(0.0, 0.0, width, 200.0);
        let pane = pane(Size::new(50_000.0, 50_000.0));
        let track = pane.track(rect, Axis::Horizontal).expect("a track");
        let (minus, _) = pane.zoom_button(rect, Axis::Horizontal).expect("buttons");
        assert!(track.right() <= minus.x + 0.001, "at width {width}");
    }
}
