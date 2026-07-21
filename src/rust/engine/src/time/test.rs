use super::*;

#[test]
fn now_is_a_sentinel_not_a_reachable_position() {
    assert!(NOW.is_now());
    assert!(!SampleTime(0).is_now());
    // Reachable only after six million years at 96 kHz, which is the point.
    assert!(!SampleTime(u64::MAX - 1).is_now());
}

#[test]
fn arithmetic_saturates_rather_than_wraps() {
    assert_eq!(SampleTime(10).saturating_sub(SampleTime(30)), SampleTime(0));
    assert_eq!(SampleTime(u64::MAX) + 10, SampleTime(u64::MAX));
    assert_eq!(SampleTime(5) - SampleTime(9), 0);
}

#[test]
fn seconds_round_trip_at_the_usual_rates() {
    for rate in [44_100, 48_000, 96_000] {
        let t = SampleTime::from_seconds(2.5, rate);
        assert_eq!(t.0, u64::from(rate) * 5 / 2);
        assert!((t.seconds(rate) - 2.5).abs() < 1e-12);
    }
}
