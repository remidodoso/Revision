use super::*;

use rev_ui_mech::Size;

const RECT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 415.0,
    h: 200.0,
};

/// A pane whose interior is 400 wide once the gutter is taken, at one beat per
/// 50 pixels — so a viewport is exactly 8 beats and the arithmetic below is
/// readable.
fn pane() -> Pane {
    Pane {
        extent: Size::new(1_000.0, 4.0),
        scale: rev_ui_kit::pane::Scale { x: 0.02, y: 0.01 },
        ..Pane::default()
    }
}

fn viewport(pane: &Pane) -> f64 {
    f64::from(pane.viewport(RECT, Axis::Horizontal))
}

#[test]
fn it_follows_by_jumping_and_lands_where_it_should() {
    let follow = Follow::default();
    let mut pane = pane();
    let width = viewport(&pane);

    // Comfortably inside: nothing happens, which is the common case and the
    // whole point of paging rather than sliding.
    assert!(follow.advance(RECT, &mut pane, width * 0.5).is_none());
    assert_eq!(pane.offset.x, 0.0);

    // Past the trigger: one jump, and the playhead lands at `land`.
    let playhead = width * f64::from(FOLLOW_TRIGGER) + 0.01;
    assert!(follow.advance(RECT, &mut pane, playhead).is_some());
    let across = (playhead - f64::from(pane.offset.x)) / width;
    assert!(
        (across - f64::from(FOLLOW_LAND)).abs() < 0.01,
        "landed at {across}, wanted {FOLLOW_LAND}"
    );

    // And immediately afterwards it does not fire again — which is what the
    // minimum separation exists to guarantee.
    assert!(follow.advance(RECT, &mut pane, playhead).is_none());
}

#[test]
fn the_jump_is_thirty_percent_of_a_view() {
    // Stated in the proposal so nobody is surprised: `(trigger - land)` of a
    // viewport, not a windowful. Asserted so it stays true.
    let follow = Follow::default();
    let mut pane = pane();
    let width = viewport(&pane);
    let before = f64::from(pane.offset.x);
    follow.advance(RECT, &mut pane, width * f64::from(FOLLOW_TRIGGER) + 0.01);
    let moved = (f64::from(pane.offset.x) - before) / width;
    let expect = f64::from(FOLLOW_TRIGGER - FOLLOW_LAND);
    assert!(
        (moved - expect).abs() < 0.01,
        "moved {moved}, wanted {expect}"
    );
}

#[test]
fn a_horizontal_scroll_takes_over_and_a_vertical_one_does_not() {
    // Follow governs time. Looking at a high note makes no claim about where
    // you are in the piece.
    let mut follow = Follow::default();
    follow.user_scrolled(Axis::Vertical);
    assert!(
        follow.armed(),
        "vertical scrolling is not a claim about time"
    );
    follow.user_scrolled(Axis::Horizontal);
    assert!(!follow.armed(), "horizontal scrolling takes over");

    let mut pane = pane();
    let width = viewport(&pane);
    assert!(
        follow.advance(RECT, &mut pane, width * 0.99).is_none(),
        "and once taken over, the view stays where the user put it"
    );
}

#[test]
fn a_locate_re_arms_and_nothing_else_does() {
    let mut follow = Follow::default();
    follow.user_scrolled(Axis::Horizontal);
    assert!(!follow.armed());

    // Plain Play must not yank the view out from under someone looking at
    // something, and Stop is often pressed precisely in order to go and look —
    // so neither is expressible here at all. Only a locate, and the toggle.
    follow.located();
    assert!(follow.armed(), "an explicit locate re-arms");

    follow.user_scrolled(Axis::Horizontal);
    follow.set_armed(true);
    assert!(follow.armed(), "and so does the control");
}

#[test]
fn zoom_does_not_disarm_but_it_does_change_the_anchor() {
    // Zooming while following is normal; it should not fight the follow and it
    // should not switch it off. Instead follow changes what stays still.
    let follow = Follow::default();
    let pane = pane();
    let anchor = follow.zoom_anchor(RECT, &pane, Some(4.0));
    assert!(anchor.is_some(), "armed: the playhead is the anchor");
    assert!(follow.armed(), "and zooming did not disarm anything");

    let mut off = Follow::default();
    off.set_armed(false);
    assert!(
        off.zoom_anchor(RECT, &pane, Some(4.0)).is_none(),
        "not armed: the pointer anchors, which is the pane's own behaviour"
    );
}

#[test]
fn the_pair_is_constrained_because_the_alternative_is_a_loop() {
    // If `land >= trigger` the playhead lands on or past the trigger and fires
    // again immediately. Refusing the setting is the only thing that prevents
    // it, and the refusal has to happen where it is set.
    let mut follow = Follow::default();
    assert!(!follow.set(0.5, 0.5), "equal is a loop");
    assert!(!follow.set(0.5, 0.6), "inverted is worse");
    assert!(!follow.set(0.5, 0.4), "too close is still a loop");
    assert!(follow.set(0.9, 0.3), "and a usable pair is accepted");

    // The accepted pair does not repeat.
    let mut pane = pane();
    let width = viewport(&pane);
    let playhead = width * 0.9 + 0.01;
    assert!(follow.advance(RECT, &mut pane, playhead).is_some());
    assert!(follow.advance(RECT, &mut pane, playhead).is_none());
}

#[test]
fn the_end_of_the_piece_clamps_and_the_playhead_runs_to_the_edge() {
    // Correct, not a defect: there is no content left to scroll into, so the
    // playhead simply travels within the final view.
    let follow = Follow::default();
    let mut pane = pane();
    let last = f64::from(pane.extent.w);
    follow.advance(RECT, &mut pane, last);
    let limit = f64::from(pane.extent.w) - viewport(&pane);
    assert!(
        (f64::from(pane.offset.x) - limit).abs() < 0.01,
        "clamped at {} not {limit}",
        pane.offset.x
    );
}

#[test]
fn locating_backwards_brings_the_playhead_back_into_view() {
    let follow = Follow::default();
    let mut pane = pane();
    pane.offset.x = 200.0;
    assert!(
        follow.advance(RECT, &mut pane, 4.0).is_some(),
        "behind the view is as much out of view as ahead of it"
    );
    let across = (4.0 - f64::from(pane.offset.x)) / viewport(&pane);
    assert!((0.0..=1.0).contains(&across), "back in sight: {across}");
}

#[test]
fn a_stationary_view_is_the_common_case() {
    // R-947, as arithmetic: play a whole viewport's worth of music and count
    // the jumps. Sliding would be one per frame; this is one per 30 % of a view.
    let follow = Follow::default();
    let mut pane = pane();
    let width = viewport(&pane);
    let mut jumps = 0;
    let mut beat = 0.0;
    while beat < width * 3.0 {
        if follow.advance(RECT, &mut pane, beat).is_some() {
            jumps += 1;
        }
        // A frame at 60 Hz, 120 bpm: two beats a second.
        beat += 2.0 / 60.0;
    }
    // Three viewports of music at 30 % a jump is about ten.
    assert!(
        (8..=12).contains(&jumps),
        "{jumps} jumps for three viewports"
    );
}
