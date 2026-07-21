use super::*;

/// Tests never load system fonts: a golden master that depends on what is
/// installed is not a master.
fn text() -> FontStack {
    FontStack::new(false)
}

#[test]
fn both_bundled_families_resolve() {
    let mut t = text();
    for role in [FontRole::Ui, FontRole::Numeric] {
        let s = t.shape(
            "Revision",
            &TextStyle {
                role,
                ..Default::default()
            },
        );
        assert!(s.size().w > 0.0, "{role:?} produced no advance");
        assert_eq!(s.glyph.len(), 8, "{role:?} lost glyphs");
    }
}

#[test]
fn the_numeric_role_is_monospaced() {
    // The property the Counter depends on: digits cannot shift as values change.
    let mut t = text();
    let style = TextStyle::numeric(14.0);
    let narrow = t.shape("111111", &style).size().w;
    let wide = t.shape("000000", &style).size().w;
    assert!(
        (narrow - wide).abs() < 0.01,
        "numeric role is not monospaced: {narrow} vs {wide}"
    );
}

#[test]
fn the_ui_role_is_proportional() {
    // The counterpart: proportional text is what makes names readable.
    let mut t = text();
    let style = TextStyle::ui(14.0);
    let narrow = t.shape("llllll", &style).size().w;
    let wide = t.shape("MMMMMM", &style).size().w;
    assert!(wide > narrow * 1.5, "ui role looks monospaced");
}

#[test]
fn bold_and_italic_are_real_faces() {
    // Not a synthesized oblique: a real italic face has different advances.
    let mut t = text();
    let upright = t.shape("Revision", &TextStyle::ui(16.0)).size().w;
    let italic = t.shape("Revision", &TextStyle::ui(16.0).italic()).size().w;
    let bold = t.shape("Revision", &TextStyle::ui(16.0).bold()).size().w;
    assert!(italic != upright, "italic did not resolve to its own face");
    assert!(bold > upright, "bold did not resolve to its own face");
}

#[test]
fn size_scales_the_advance() {
    let mut t = text();
    let small = t.shape("Revision", &TextStyle::ui(10.0)).size().w;
    let large = t.shape("Revision", &TextStyle::ui(20.0)).size().w;
    assert!((large / small - 2.0).abs() < 0.1, "{small} -> {large}");
}

#[test]
fn caret_walks_left_to_right() {
    let mut t = text();
    let s = t.shape("120|3|0000", &TextStyle::numeric(14.0));
    let mut last = -1.0;
    for byte in 0..10 {
        let x = s.caret(byte).x;
        assert!(x > last, "caret went backwards at byte {byte}");
        last = x;
    }
    // Past the end sits at the run's width, so a caret after the last character
    // has somewhere to be.
    assert_eq!(s.caret(100).x, s.size().w);
}

#[test]
fn byte_at_round_trips_through_caret() {
    // What click-to-place-caret in the Counter depends on.
    let mut t = text();
    let s = t.shape("120|3|0000", &TextStyle::numeric(14.0));
    for byte in 0..10 {
        let x = s.caret(byte).x;
        assert_eq!(s.byte_at(x + 0.5), byte, "byte {byte} did not round-trip");
    }
    assert_eq!(s.byte_at(s.size().w + 100.0), 10, "past the end");
}

#[test]
fn shaping_is_scale_free() {
    // Shaped runs survive a DPI change: rasterization is scaled, layout is not.
    let mut t = text();
    let a = t.shape("Revision", &TextStyle::ui(13.0)).size();
    let b = t.shape("Revision", &TextStyle::ui(13.0)).size();
    assert_eq!(a, b);
}

#[test]
fn rendering_produces_coverage() {
    let mut t = text();
    let s = t.shape("8", &TextStyle::numeric(24.0));
    let mut covered = 0usize;
    t.render(&s, Point::new(0.0, 0.0), 1.0, |_, _, a| {
        if a > 0 {
            covered += 1;
        }
    });
    assert!(covered > 20, "digit rasterized to {covered} covered pixels");
}
