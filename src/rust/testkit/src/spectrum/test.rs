use super::*;

const RATE: u32 = 48_000;

fn sine(hz: f64, frames: usize, level: f32) -> Vec<f32> {
    (0..frames)
        .map(|n| {
            (level as f64 * (std::f64::consts::TAU * hz * n as f64 / f64::from(RATE)).sin()) as f32
        })
        .collect()
}

#[test]
fn it_finds_a_tone_to_within_a_hertz() {
    // The whole point: 8192 frames at 48 kHz means 5.86 Hz bins, so a test that
    // trusted the bin index could not tell 440 from 443. The interpolation is
    // what makes "the fundamental is 440 ± 1" a sentence a test can say.
    for hz in [55.0, 220.0, 440.0, 441.5, 1000.0, 3520.0] {
        let measured = analyse(&sine(hz, 8192, 0.5), RATE).loudest_hz();
        assert!((measured - hz).abs() < 1.0, "{hz} measured as {measured}");
    }
}

#[test]
fn the_fundamental_is_not_merely_the_loudest_partial() {
    // A tone whose second harmonic is louder than its first — a formant, or any
    // bright patch. Taking the loudest bin would report the octave.
    let mut sample = sine(200.0, 8192, 0.2);
    for (slot, second) in sample.iter_mut().zip(sine(400.0, 8192, 0.9)) {
        *slot += second;
    }
    let spectrum = analyse(&sample, RATE);
    assert!((spectrum.loudest_hz() - 400.0).abs() < 1.0, "the loudest");
    let f0 = spectrum.fundamental_hz(30.0);
    assert!((f0 - 200.0).abs() < 1.0, "the fundamental: {f0}");
}

#[test]
fn a_pure_harmonic_series_measures_as_clean() {
    // The measurement's own noise floor. Anything the sweep asserts has to sit
    // well above this, or it would be measuring the window rather than the
    // instrument.
    let mut sample = vec![0.0f32; 8192];
    for k in 1..=8 {
        for (slot, partial) in
            sample
                .iter_mut()
                .zip(sine(150.0 * f64::from(k), 8192, 0.5 / k as f32))
        {
            *slot += partial;
        }
    }
    let worst = analyse(&sample, RATE).worst_inharmonic(150.0, 60.0, 40.0);
    assert!(db(worst) < -80.0, "the floor is {} dB", db(worst));
}

#[test]
fn an_intruder_between_the_partials_is_found() {
    let mut sample = sine(150.0, 8192, 0.5);
    // 40 dB down, deliberately between two harmonics.
    for (slot, intruder) in sample.iter_mut().zip(sine(525.0, 8192, 0.005)) {
        *slot += intruder;
    }
    let worst = db(analyse(&sample, RATE).worst_inharmonic(150.0, 60.0, 40.0));
    assert!(
        (-40.0 - worst).abs() < 3.0,
        "a 40 dB-down intruder measured at {worst} dB"
    );
}
