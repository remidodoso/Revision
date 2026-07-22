use super::*;

/// 48 kHz: 48 samples per millisecond, so 48e-6 samples per nanosecond.
const RATE_SPN: f64 = 48_000.0 / 1e9;

#[test]
fn a_clean_clock_recovers_its_rate_and_maps_exactly() {
    let mut c = Correlation::new(32);
    // A block every ~2.67 ms (128 frames at 48 kHz), sample and time in lockstep.
    for block in 0..32u64 {
        let sample = block * 128;
        let nanos = (sample as f64 / RATE_SPN) as u64;
        c.observe(Pair { sample, nanos });
    }
    let spn = c.samples_per_nano().expect("a fit");
    assert!((spn - RATE_SPN).abs() / RATE_SPN < 1e-6, "rate: {spn}");

    // An instant between two pairs maps to the sample it should.
    let want = 1000u64 * 128 / 48; // arbitrary interior sample... check the map instead:
    let _ = want;
    let sample = 20u64 * 128;
    let nanos = (sample as f64 / RATE_SPN) as u64;
    let got = c.sample_at(nanos).expect("mapped");
    assert!((got - sample as f64).abs() < 1.0, "{got} vs {sample}");
}

#[test]
fn jitter_averages_out() {
    // The point of a fit rather than differencing the last two: per-block timing
    // jitter (the callback is not perfectly periodic) should not wander the
    // mapping. Inject +/- 0.3 ms of jitter and the recovered rate stays true.
    let mut c = Correlation::new(48);
    let mut wobble = 0i64;
    for block in 0..48u64 {
        let sample = block * 128;
        wobble = (wobble + 97) % 7 - 3; // deterministic -3..3
        let jitter = wobble * 100_000; // up to 0.3 ms
        let nanos = ((sample as f64 / RATE_SPN) as i64 + jitter).max(0) as u64;
        c.observe(Pair { sample, nanos });
    }
    let spn = c.samples_per_nano().expect("a fit");
    assert!(
        (spn - RATE_SPN).abs() / RATE_SPN < 0.02,
        "jittered rate within 2%: {spn} vs {RATE_SPN}"
    );
}

#[test]
fn a_drifting_clock_is_tracked() {
    // A sound card whose real rate is 48_010 Hz, not the nominal 48_000. The fit
    // should report the *observed* rate, which is what makes recorded timing
    // right rather than nominal.
    let real = 48_010.0 / 1e9;
    let mut c = Correlation::new(32);
    for block in 0..32u64 {
        let sample = block * 128;
        let nanos = (sample as f64 / real) as u64;
        c.observe(Pair { sample, nanos });
    }
    let spn = c.samples_per_nano().expect("a fit");
    assert!(
        (spn - real).abs() / real < 1e-4,
        "tracked drift: {spn} vs {real}"
    );
}

#[test]
fn it_says_nothing_until_it_can() {
    let mut c = Correlation::new(8);
    assert!(c.samples_per_nano().is_none(), "no pairs, no fit");
    c.observe(Pair {
        sample: 0,
        nanos: 0,
    });
    assert!(c.samples_per_nano().is_none(), "one pair is not a line");
    c.observe(Pair {
        sample: 128,
        nanos: 2_666_667,
    });
    assert!(c.samples_per_nano().is_some(), "two makes a line");
}

#[test]
fn a_stale_republished_pair_is_dropped() {
    // The seqlock republishes the latest value every read; a duplicate must not
    // count as a second point, or the fit sees a vertical step.
    let mut c = Correlation::new(8);
    c.observe(Pair {
        sample: 128,
        nanos: 2_666_667,
    });
    c.observe(Pair {
        sample: 128,
        nanos: 2_666_667,
    }); // same again
    c.observe(Pair {
        sample: 0,
        nanos: 1_000,
    }); // and one from the past
    assert_eq!(c.len(), 1, "only the forward-moving pair was kept");
}

#[test]
fn the_history_is_bounded() {
    let mut c = Correlation::new(4);
    for block in 0..100u64 {
        c.observe(Pair {
            sample: block * 128,
            nanos: block * 2_666_667,
        });
    }
    assert_eq!(c.len(), 4, "old pairs age out");
    // And it still fits the recent slope.
    assert!(c.samples_per_nano().is_some());
}
