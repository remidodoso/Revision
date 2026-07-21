//! The profile identities, ported from `notch/padsynth.mjs` §2.
//!
//! Identities rather than captured data, deliberately: a golden vector says
//! "this is what it did last time", and an identity says "this is what a saw
//! *is*". The second one catches a mistake the first would have enshrined.

use super::*;

fn spec(source: Source) -> BakeSpec {
    BakeSpec {
        source,
        ..BakeSpec::default()
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

#[test]
fn a_saw_is_one_over_k() {
    let a = profile(&spec(Source::Saw), 100.0);
    for (index, value) in a.iter().enumerate() {
        let k = index as f64 + 1.0;
        assert!(close(*value, 1.0 / k), "harmonic {k}: {value}");
    }
}

#[test]
fn a_pulse_at_shape_zero_is_an_odd_only_square() {
    // Duty 0.5 gives |sin(πk/2)|/k exactly: every even harmonic vanishes and
    // the odd ones fall as 1/k. That is a square wave, and it is the identity
    // that proves the duty morph is anchored where it should be.
    let a = profile(&spec(Source::Pulse), 100.0);
    for (index, value) in a.iter().enumerate() {
        let k = index + 1;
        if k % 2 == 0 {
            assert!(*value < 1e-12, "harmonic {k} should vanish: {value}");
        } else {
            assert!(close(*value, 1.0 / k as f64), "harmonic {k}: {value}");
        }
    }
}

#[test]
fn a_saw_at_shape_one_is_an_odd_only_triangle() {
    let mut spec = spec(Source::Saw);
    spec.shape = 1.0;
    let a = profile(&spec, 100.0);
    for (index, value) in a.iter().enumerate() {
        let k = index + 1;
        if k % 2 == 0 {
            assert!(*value < 1e-12, "harmonic {k} should vanish: {value}");
        } else {
            let expect = 1.0 / (k * k) as f64;
            assert!(close(*value, expect), "harmonic {k}: {value} vs {expect}");
        }
    }
}

#[test]
fn tilt_is_a_bare_rolloff_at_its_exponent() {
    for exponent in [0.5, 1.0, 1.5, 2.5] {
        let mut spec = spec(Source::Tilt);
        spec.tilt = exponent;
        let a = profile(&spec, 100.0);
        for (index, value) in a.iter().enumerate() {
            let k = index as f64 + 1.0;
            assert!(close(*value, 1.0 / k.powf(exponent)), "k {k}: {value}");
        }
    }
}

#[test]
fn the_voice_source_is_the_glottal_rolloff() {
    let a = profile(&spec(Source::Voice), 100.0);
    for (index, value) in a.iter().enumerate() {
        let k = index as f64 + 1.0;
        assert!(close(*value, 1.0 / k.powf(GLOTTAL_TILT)));
    }
}

#[test]
fn no_vowel_is_a_bypass_and_not_an_approximation() {
    // Exactly 1.0, at every frequency: a "nearly flat" mask would tilt every
    // default patch in the instrument, which is the kind of error that gets
    // mistaken for taste.
    for f in [20.0, 100.0, 440.0, 3_000.0, 19_000.0] {
        assert_eq!(formant_mask(f, Vowel::None, 1.0, 9.0), 1.0);
    }
}

#[test]
fn a_resonator_peaks_at_its_centre() {
    // The identity belongs to the resonator, not to the mask. Asserting that
    // the *mask* peaks at every formant is false and the first version of this
    // test failed on it: F1 and F2 of "ah" are 800 and 1150 Hz, close enough
    // that F1's skirt — at gain 1.0 against F2's 0.6 — swallows F2 into one
    // broad peak. That is what a vowel is, and it would be wrong to fix.
    for fc in [300.0, 1_200.0, 3_000.0] {
        assert!(close(resonator(fc, fc, 9.0), 1.0), "unity at the centre");
        assert!(resonator(fc * 0.7, fc, 9.0) < 0.5, "skirts fall below");
        assert!(resonator(fc * 1.4, fc, 9.0) < 0.5);
    }
}

#[test]
fn a_vowel_lifts_its_formant_region_and_falls_away_above() {
    // What survives of the peak claim, and what the mask is actually for: near
    // a formant there is more energy than far above every formant.
    for vowel in [Vowel::Ooh, Vowel::Oh, Vowel::Ah, Vowel::Eh, Vowel::Ee] {
        let formant = vowel.formant().expect("a vowel has formants");
        let far = formant_mask(formant[2] * 4.0, vowel, 1.0, 9.0);
        for centre in formant {
            let at = formant_mask(centre, vowel, 1.0, 9.0);
            assert!(at > far * 2.0, "{vowel:?} at {centre}: {at} vs far {far}");
        }
    }
}

#[test]
fn size_moves_the_formants_and_nothing_else() {
    // A bigger vocal tract puts the same peaks lower. The mask at a scaled
    // frequency is the mask at the unscaled one — the whole envelope slides.
    let vowel = Vowel::Ee;
    for f in [400.0, 1_200.0, 3_000.0] {
        let plain = formant_mask(f, vowel, 1.0, 9.0);
        let scaled = formant_mask(f * 1.25, vowel, 1.25, 9.0);
        assert!((plain - scaled).abs() < 1e-9, "{plain} vs {scaled}");
    }
}

#[test]
fn the_mask_is_universal_across_sources() {
    // The point of the formant bank being a *mask* is that any source can be
    // vowel-shaped. So the ratio between a vowelled profile and a plain one is
    // the mask, whatever the source underneath it is.
    for source in [Source::Saw, Source::Pulse, Source::Voice, Source::Tilt] {
        let plain = profile(&spec(source), 120.0);
        let vowelled = profile(
            &BakeSpec {
                source,
                vowel: Vowel::Oh,
                ..BakeSpec::default()
            },
            120.0,
        );
        for (index, (p, v)) in plain.iter().zip(&vowelled).enumerate() {
            if *p < 1e-12 {
                continue;
            }
            let mask = formant_mask(120.0 * (index as f64 + 1.0), Vowel::Oh, 1.0, 9.0);
            assert!((v / p - mask).abs() < 1e-9, "{source:?} harmonic {index}");
        }
    }
}

#[test]
fn the_voice_source_with_a_vowel_is_a_choir() {
    // Stated as a test because it is the claim the inventory makes about the
    // port: the old Choir instrument is not a fifth source, it is Voice times a
    // vowel at the old fixed Q.
    let choir = profile(
        &BakeSpec {
            source: Source::Voice,
            vowel: Vowel::Ah,
            formant_q: 9.0,
            ..BakeSpec::default()
        },
        130.0,
    );
    for (index, value) in choir.iter().enumerate() {
        let k = index as f64 + 1.0;
        let expect = 1.0 / k.powf(GLOTTAL_TILT) * formant_mask(130.0 * k, Vowel::Ah, 1.0, 9.0);
        assert!(close(*value, expect));
    }
}
