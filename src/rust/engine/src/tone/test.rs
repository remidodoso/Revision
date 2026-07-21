use super::*;

const RATE: u32 = 48_000;

#[test]
fn silence_is_detectable() {
    let tone = Tone::new(RATE);
    assert!(tone.is_silent());
}

#[test]
fn gain_ramps_rather_than_stepping() {
    // A hard gate on a sine is a click, and a click is indistinguishable from a
    // scheduling bug when you are trying to hear whether the scheduler works.
    let mut tone = Tone::new(RATE);
    tone.on(440.0, 1.0, RATE);

    let mut out = vec![0.0f32; 16];
    tone.render(&mut out);

    assert!(out[0].abs() < 0.01, "first sample near silent: {}", out[0]);
    for pair in out.windows(2) {
        assert!(
            (pair[1] - pair[0]).abs() < 0.05,
            "no step larger than a ramp: {pair:?}"
        );
    }
}

#[test]
fn it_adds_rather_than_assigns() {
    // A voice never assumes it is the only thing in the buffer.
    let mut tone = Tone::new(RATE);
    tone.on(1000.0, 1.0, RATE);
    let mut out = vec![0.5f32; 8];
    tone.render(&mut out);
    assert!(out.iter().all(|s| *s >= 0.49), "existing content survived");
}

#[test]
fn phase_stays_bounded_over_a_long_render() {
    // An unbounded f64 phase loses precision in `sin`, and the loss is audible
    // as slow detuning — so the wrap is a correctness property, not tidiness.
    let mut tone = Tone::new(RATE);
    tone.on(440.0, 1.0, RATE);
    let mut out = vec![0.0f32; RATE as usize * 10];
    tone.render(&mut out);
    assert!(tone.phase < std::f64::consts::TAU && tone.phase >= 0.0);
}

#[test]
fn frequency_is_derived_from_the_rate_it_is_told() {
    // There is no rate constant anywhere in this crate: the same frequency at
    // twice the rate must advance half as fast per sample.
    let mut slow = Tone::new(48_000);
    let mut fast = Tone::new(96_000);
    slow.on(440.0, 1.0, 48_000);
    fast.on(440.0, 1.0, 96_000);
    assert!((slow.step - fast.step * 2.0).abs() < 1e-12);
}
