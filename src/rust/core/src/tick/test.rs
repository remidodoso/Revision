use super::*;

#[test]
fn ppq_divides_common_subdivisions_exactly() {
    // The reason for 5040 = 2^4·3^2·5·7: duple, triple, quintuple and septuple
    // divisions of a quarter note are all exact integers, so tuplets never
    // round (R-003).
    for divisor in [
        2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 15, 16, 20, 21, 24, 28, 30, 35, 36,
    ] {
        assert_eq!(PPQ % divisor, 0, "PPQ is not divisible by {divisor}");
    }
    // The honest limit of the factorization: 32 subdivisions per quarter (a
    // 128th note) is not exact, since 5040 carries only 2^4.
    assert_ne!(PPQ % 32, 0);
}

#[test]
fn note_values_are_exact() {
    assert_eq!(Tick::per_note_value(4).get(), PPQ);
    assert_eq!(Tick::per_note_value(8).get(), PPQ / 2);
    assert_eq!(Tick::per_note_value(1).get(), PPQ * 4);
}

#[test]
fn second_conversion_round_trips() {
    let upq = bpm_to_usec_per_quarter(120.0);
    assert_eq!(upq, 500_000);
    // One quarter at 120 bpm is half a second.
    assert!((tick_to_second(Tick(PPQ), upq) - 0.5).abs() < 1e-12);
    assert_eq!(second_to_tick(0.5, upq), Tick(PPQ));
}

#[test]
fn tick_zero_is_zero_seconds() {
    assert_eq!(tick_to_second(Tick::ZERO, 500_000), 0.0);
}
