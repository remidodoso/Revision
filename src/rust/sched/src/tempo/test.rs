use super::*;

use proptest::prelude::*;

const RATE: u32 = 48_000;
/// 120 bpm at 48 kHz: a quarter note is half a second.
const QUARTER: u64 = 24_000;

fn at_120() -> TempoMap {
    TempoMap::constant(DEFAULT_USEC_PER_QUARTER, RATE)
}

#[test]
fn quarters_land_where_a_human_can_check() {
    // 5040 ticks per quarter, 120 bpm, 48 kHz. The arithmetic a reader should be
    // able to do in their head, which is why these numbers were chosen.
    let map = at_120();
    for beat in 0..8u64 {
        assert_eq!(
            map.sample_at(Tick(PPQ * beat as i64)),
            SampleTime(QUARTER * beat),
            "beat {beat}"
        );
    }
}

#[test]
fn the_reverse_is_exact_on_tick_boundaries() {
    let map = at_120();
    for tick in [0, 1, 2, PPQ / 2, PPQ, PPQ * 3 + 17, PPQ * 1000] {
        let there = map.sample_at(Tick(tick));
        assert_eq!(
            map.tick_at(there),
            Tick(tick),
            "tick {tick} did not survive"
        );
    }
}

#[test]
fn an_empty_map_is_120() {
    let empty = TempoMap::new(std::iter::empty(), RATE);
    assert_eq!(empty.sample_at(Tick(PPQ)), SampleTime(QUARTER));
}

#[test]
fn tempo_before_the_first_point_is_that_points_tempo() {
    // The alternative — a fixed default before the first point — would make a map
    // beginning at tick 5040 mean something different from the same map beginning
    // at tick 0. Nobody wants that surprise.
    let late = TempoMap::new([(Tick(PPQ * 4), 250_000)], RATE);
    // 250_000 usec/quarter is 240 bpm: a quarter is 12_000 samples, from tick 0.
    assert_eq!(late.sample_at(Tick(PPQ)), SampleTime(12_000));
    assert_eq!(late.sample_at(Tick(PPQ * 4)), SampleTime(48_000));
}

#[test]
fn a_tempo_change_takes_effect_at_its_point_and_not_before() {
    // 120 for four beats, then 240.
    let map = TempoMap::new([(Tick(0), 500_000), (Tick(PPQ * 4), 250_000)], RATE);

    assert_eq!(map.sample_at(Tick(PPQ * 4)), SampleTime(QUARTER * 4));
    // After the change a quarter costs half as much.
    assert_eq!(
        map.sample_at(Tick(PPQ * 5)),
        SampleTime(QUARTER * 4 + 12_000)
    );
    assert_eq!(
        map.sample_at(Tick(PPQ * 8)),
        SampleTime(QUARTER * 4 + 48_000)
    );
}

#[test]
fn boundaries_are_anchored_so_error_cannot_compound() {
    // A map of many awkward tempos: if each boundary were re-derived rather than
    // anchored, the drift would show as a mismatch between walking the map and
    // asking it directly.
    let point: Vec<(Tick, i64)> = (0..64)
        .map(|n| (Tick(PPQ * n), 300_001 + n * 7919))
        .collect();
    let map = TempoMap::new(point.clone(), RATE);

    // Walk it by hand, segment by segment, and compare with the map's answer.
    let mut walked = 0u64;
    for window in point.windows(2) {
        let (from, upq) = window[0];
        let (to, _) = window[1];
        walked += span(to.get() - from.get(), upq, RATE);
        assert_eq!(
            map.sample_at(to),
            SampleTime(walked),
            "boundary at {} drifted",
            to.get()
        );
    }
}

#[test]
fn out_of_order_points_are_sorted_rather_than_trusted() {
    let jumbled = TempoMap::new([(Tick(PPQ * 4), 250_000), (Tick(0), 500_000)], RATE);
    let ordered = TempoMap::new([(Tick(0), 500_000), (Tick(PPQ * 4), 250_000)], RATE);
    for tick in [0, PPQ, PPQ * 4, PPQ * 9] {
        assert_eq!(jumbled.sample_at(Tick(tick)), ordered.sample_at(Tick(tick)));
    }
}

#[test]
fn rounding_has_no_systematic_bias() {
    // Half-up would push every tie the same direction. Over many ties that is a
    // measurable drag; round-half-to-even splits them.
    assert_eq!(div_round_even(5, 2), 2, "2.5 -> 2 (even)");
    assert_eq!(div_round_even(7, 2), 4, "3.5 -> 4 (even)");
    assert_eq!(div_round_even(4, 2), 2);
    assert_eq!(div_round_even(1, 3), 0);
    assert_eq!(div_round_even(2, 3), 1);
}

proptest! {
    /// The property this module exists for: order in, order out. A conversion
    /// that can invert two adjacent events produces a schedule that plays notes
    /// out of order — rare, unreproducible, and horrible to diagnose.
    #[test]
    fn conversion_is_monotonic(
        mut tick in prop::collection::vec(0i64..50_000_000, 2..40),
        upq in prop::collection::vec(1_000i64..2_000_000, 1..8),
        rate in prop::sample::select(vec![44_100u32, 48_000, 96_000, 192_000]),
    ) {
        let point: Vec<(Tick, i64)> = upq
            .iter()
            .enumerate()
            .map(|(n, &u)| (Tick(n as i64 * PPQ * 3), u))
            .collect();
        let map = TempoMap::new(point, rate);

        tick.sort_unstable();
        let mut previous = SampleTime(0);
        for t in tick {
            let here = map.sample_at(Tick(t));
            prop_assert!(
                here >= previous,
                "tick {t} went backwards: {here:?} after {previous:?}"
            );
            previous = here;
        }
    }

    /// Ticks survive the round trip **while a tick is at least a sample wide**.
    ///
    /// Beyond that — above roughly 525 bpm at 44.1 kHz — several ticks share a
    /// sample and no inverse can distinguish them. The property test found that;
    /// it is a fact about 5040 PPQ against a sample grid, not a defect.
    #[test]
    fn a_tick_survives_the_round_trip_while_it_is_wider_than_a_sample(
        tick in 0i64..20_000_000,
        upq in 120_000i64..1_500_000,
        rate in prop::sample::select(vec![44_100u32, 48_000, 96_000]),
    ) {
        prop_assume!(i128::from(upq) * i128::from(rate) >= i128::from(PPQ) * 1_000_000);
        let map = TempoMap::constant(upq, rate);
        prop_assert_eq!(map.tick_at(map.sample_at(Tick(tick))), Tick(tick));
    }

    /// And past that limit it is still *close* — never adrift, just unable to
    /// resolve below one sample.
    #[test]
    fn the_round_trip_is_never_off_by_more_than_a_sample_of_ticks(
        tick in 0i64..20_000_000,
        upq in 1_000i64..1_500_000,
        rate in prop::sample::select(vec![44_100u32, 48_000, 96_000]),
    ) {
        let map = TempoMap::constant(upq, rate);
        let back = map.tick_at(map.sample_at(Tick(tick))).get();
        // How many ticks one sample spans at this tempo.
        let per_sample = (i128::from(PPQ) * 1_000_000
            / (i128::from(upq) * i128::from(rate))) as i64;
        prop_assert!(
            (back - tick).abs() <= per_sample + 1,
            "{tick} came back as {back}, further than one sample ({per_sample} ticks)"
        );
    }
}
