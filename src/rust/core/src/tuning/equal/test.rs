use super::*;

/// Tolerance for "as good as libm" — a few ulps. What matters for R-501 is that
/// the value is the *same* everywhere; these tests check it is also correct.
const EPS: f64 = 1e-14;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPS * b.abs().max(1.0)
}

#[test]
fn exp2_exact_at_integers() {
    // Integer exponents go through the exact power-of-two path: bit-exact.
    assert_eq!(exp2(0.0), 1.0);
    assert_eq!(exp2(1.0), 2.0);
    assert_eq!(exp2(-1.0), 0.5);
    assert_eq!(exp2(10.0), 1024.0);
    assert_eq!(exp2(-10.0), 1.0 / 1024.0);
}

#[test]
fn exp2_matches_reference_across_the_range() {
    let mut y = -30.0;
    while y <= 30.0 {
        assert!(close(exp2(y), y.exp2()), "exp2({y}) = {} ", exp2(y));
        y += 0.125;
    }
}

#[test]
fn exp2_half_squares_to_two() {
    let root = exp2(0.5);
    assert!(close(root * root, 2.0));
}

#[test]
fn log2_exact_at_powers_of_two() {
    assert_eq!(log2(1.0), 0.0);
    assert_eq!(log2(2.0), 1.0);
    assert_eq!(log2(0.5), -1.0);
    assert_eq!(log2(1024.0), 10.0);
}

#[test]
fn log2_matches_reference() {
    for x in [0.1, 0.5, 1.5, 3.0, 5.0, 7.0, 261.6255653005986, 44100.0] {
        assert!(close(log2(x), x.log2()), "log2({x})");
    }
}

#[test]
fn exp2_and_log2_round_trip() {
    for x in [0.25, 1.0, 3.0, 440.0, 20000.0] {
        assert!(close(exp2(log2(x)), x), "round trip at {x}");
    }
}

#[test]
fn log2_ratio_short_circuits_the_octave() {
    // Exactly 1.0, not merely close: octave tunings must not inherit series error.
    assert_eq!(log2_ratio(2, 1), 1.0);
    // Bohlen-Pierce's tritave still works, through the series.
    assert!(close(log2_ratio(3, 1), 3f64.log2()));
}

#[test]
fn ratio_powi_is_exact_for_octaves() {
    assert_eq!(ratio_powi(2, 1, 0), 1.0);
    assert_eq!(ratio_powi(2, 1, 3), 8.0);
    assert_eq!(ratio_powi(2, 1, -2), 0.25);
}

#[test]
fn ratio_powi_handles_just_ratios() {
    assert!(close(ratio_powi(3, 2, 2), 2.25));
    assert!(close(ratio_powi(3, 2, -1), 2.0 / 3.0));
}

#[test]
fn twelve_equal_lands_on_concert_pitch() {
    // Middle C anchored, A4 nine semitones up must be 440 Hz.
    let middle_c = 261.625_565_300_598_6;
    let a4 = middle_c * exp2(9.0 / 12.0);
    assert!((a4 - 440.0).abs() < 1e-9, "A4 = {a4}");
}

#[test]
fn sixteen_equal_step_is_seventy_five_cents() {
    let step_cents = 1200.0 * log2(exp2(1.0 / 16.0));
    assert!(
        (step_cents - 75.0).abs() < 1e-9,
        "step = {step_cents} cents"
    );
}

#[test]
fn repeated_evaluation_is_bit_identical() {
    // The determinism gate in miniature (R-1402): same input, same bits, always.
    for k in -50..=50 {
        let y = f64::from(k) / 16.0;
        assert_eq!(exp2(y).to_bits(), exp2(y).to_bits());
    }
}
