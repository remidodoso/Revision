use super::*;

#[test]
fn interface_scale_composes_with_platform_dpi() {
    // The point of R-938: the two multiply. A window on a 2x display at 1.5x
    // interface scale rasterizes at 3x, and its logical size shrinks to match.
    assert_eq!(compose_scale(2.0, None, 1.5), 3.0);
    assert_eq!(compose_scale(1.0, None, 1.0), 1.0);
}

#[test]
fn a_window_override_replaces_the_default_rather_than_compounding() {
    // A palette dragged onto a laptop panel wants its own answer, not the
    // workspace default multiplied by one.
    assert_eq!(compose_scale(2.0, Some(1.0), 1.5), 2.0);
    assert_eq!(compose_scale(1.0, Some(2.0), 1.5), 2.0);
}
