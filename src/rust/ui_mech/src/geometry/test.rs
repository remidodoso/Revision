use super::*;

#[test]
fn contain_is_half_open() {
    let r = Rect::new(10.0, 10.0, 100.0, 50.0);
    assert!(r.contains(Point::new(10.0, 10.0)));
    // right and bottom edges are outside, so adjacent rects never both claim a pixel
    assert!(!r.contains(Point::new(110.0, 30.0)));
    assert!(!r.contains(Point::new(30.0, 60.0)));
    assert!(r.contains(Point::new(109.9, 59.9)));
}

#[test]
fn union_ignores_empty() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert_eq!(a.union(Rect::default()), a);
    assert_eq!(Rect::default().union(a), a);
    // accumulating from a default rect is the dirty-region idiom
    let acc = Rect::default()
        .union(a)
        .union(Rect::new(20.0, 5.0, 5.0, 20.0));
    assert_eq!(acc, Rect::new(0.0, 0.0, 25.0, 25.0));
}

#[test]
fn intersect_of_disjoint_is_empty() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    let b = Rect::new(20.0, 20.0, 10.0, 10.0);
    assert!(a.intersect(b).empty());
    // touching edges do not overlap, consistent with half-open containment
    assert!(a.intersect(Rect::new(10.0, 0.0, 10.0, 10.0)).empty());
    assert_eq!(
        a.intersect(Rect::new(5.0, 5.0, 100.0, 100.0)),
        Rect::new(5.0, 5.0, 5.0, 5.0)
    );
}

#[test]
fn empty_size_detects_minimized() {
    assert!(Size::new(0.0, 100.0).empty());
    assert!(!Size::new(1.0, 1.0).empty());
}
