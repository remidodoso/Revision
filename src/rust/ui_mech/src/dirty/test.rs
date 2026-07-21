use super::*;

#[test]
fn starts_empty_and_paints_nothing() {
    let d = Dirty::default();
    assert!(d.empty());
    assert!(d.bound().empty());
    assert!(!d.touches(Rect::new(0.0, 0.0, 100.0, 100.0)));
}

#[test]
fn repeated_marks_of_one_widget_stay_one_entry() {
    let mut d = Dirty::default();
    let r = Rect::new(10.0, 10.0, 20.0, 20.0);
    for _ in 0..50 {
        d.add(r);
    }
    assert_eq!(d.rect().len(), 1);
}

#[test]
fn a_covering_rect_absorbs_what_it_contains() {
    let mut d = Dirty::default();
    d.add(Rect::new(10.0, 10.0, 5.0, 5.0));
    d.add(Rect::new(30.0, 30.0, 5.0, 5.0));
    assert_eq!(d.rect().len(), 2);
    d.add(Rect::new(0.0, 0.0, 100.0, 100.0));
    assert_eq!(d.rect().len(), 1);
    assert_eq!(d.rect()[0], Rect::new(0.0, 0.0, 100.0, 100.0));
}

#[test]
fn past_the_cap_it_collapses_to_bounds() {
    let mut d = Dirty::default();
    for i in 0..(CAP * 3) {
        d.add(Rect::new(i as f32 * 20.0, 0.0, 5.0, 5.0));
        // The invariant is a bound, not a fixed size: collapsing folds everything
        // so far into one rectangle, and marks after it accumulate again.
        assert!(d.rect().len() <= CAP, "region grew past the cap");
    }
    // Collapse loses precision, never coverage — every mark still reports dirty.
    for i in 0..(CAP * 3) {
        assert!(
            d.touches(Rect::new(i as f32 * 20.0, 0.0, 5.0, 5.0)),
            "coverage lost for mark {i}"
        );
    }
}

#[test]
fn touches_is_exact_before_collapse() {
    let mut d = Dirty::default();
    d.add(Rect::new(0.0, 0.0, 10.0, 10.0));
    d.add(Rect::new(100.0, 100.0, 10.0, 10.0));
    assert!(d.touches(Rect::new(5.0, 5.0, 2.0, 2.0)));
    assert!(d.touches(Rect::new(105.0, 105.0, 2.0, 2.0)));
    // Between the two: the whole point of keeping rectangles rather than bounds.
    assert!(!d.touches(Rect::new(50.0, 50.0, 5.0, 5.0)));
}

#[test]
fn empty_marks_are_ignored() {
    let mut d = Dirty::default();
    d.add(Rect::new(10.0, 10.0, 0.0, 50.0));
    assert!(d.empty());
}

#[test]
fn clear_returns_to_empty() {
    let mut d = Dirty::default();
    d.add(Rect::new(0.0, 0.0, 10.0, 10.0));
    d.clear();
    assert!(d.empty());
}
