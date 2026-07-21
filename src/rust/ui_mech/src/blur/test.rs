use super::*;

fn block(width: usize, height: usize, rect: (usize, usize, usize, usize)) -> Vec<u8> {
    let mut m = vec![0u8; width * height];
    for y in rect.1..rect.1 + rect.3 {
        for x in rect.0..rect.0 + rect.2 {
            m[y * width + x] = 255;
        }
    }
    m
}

#[test]
fn zero_sigma_is_identity() {
    let mut m = block(16, 16, (4, 4, 8, 8));
    let before = m.clone();
    blur(&mut m, 16, 16, 0.0, &mut Scratch::default());
    assert_eq!(m, before);
}

#[test]
fn the_interior_of_a_wide_block_stays_solid() {
    // A shadow's core must not go grey; only its edge is supposed to soften.
    let mut m = block(64, 64, (8, 8, 48, 48));
    blur(&mut m, 64, 64, 3.0, &mut Scratch::default());
    assert_eq!(m[32 * 64 + 32], 255, "centre lost coverage");
}

#[test]
fn edges_soften_outward() {
    let mut m = block(64, 64, (24, 24, 16, 16));
    blur(&mut m, 64, 64, 4.0, &mut Scratch::default());
    // Just outside the original edge there is now partial coverage...
    let outside = m[32 * 64 + 21];
    assert!(outside > 0, "blur did not spread outside the shape");
    // ...and it falls off with distance.
    let further = m[32 * 64 + 16];
    assert!(further < outside, "coverage did not decrease with distance");
}

#[test]
fn total_coverage_is_roughly_conserved() {
    // A box blur moves ink around; it must not invent or destroy much of it.
    let m0 = block(96, 96, (32, 32, 32, 32));
    let mut m = m0.clone();
    blur(&mut m, 96, 96, 5.0, &mut Scratch::default());
    let before: u64 = m0.iter().map(|&v| u64::from(v)).sum();
    let after: u64 = m.iter().map(|&v| u64::from(v)).sum();
    let drift = (after as f64 - before as f64).abs() / before as f64;
    assert!(drift < 0.05, "coverage drifted {:.1}%", drift * 100.0);
}

#[test]
fn a_uniform_field_is_unchanged() {
    // The clamped edges must not darken a fully covered mask, or every shadow
    // would acquire a rim.
    let mut m = vec![255u8; 32 * 32];
    blur(&mut m, 32, 32, 4.0, &mut Scratch::default());
    assert!(
        m.iter().all(|&v| v == 255),
        "uniform field was not preserved"
    );
}

#[test]
fn blur_is_deterministic() {
    // The property the screenshot masters rest on: integer arithmetic, same answer
    // every time and on every machine.
    let mut a = block(48, 48, (12, 12, 24, 24));
    let mut b = a.clone();
    blur(&mut a, 48, 48, 3.7, &mut Scratch::default());
    blur(&mut b, 48, 48, 3.7, &mut Scratch::default());
    assert_eq!(a, b);
}

#[test]
fn spread_covers_the_reach() {
    // Whatever the buffer is padded by must actually contain the blur, or shadows
    // get clipped square at their own edges.
    let sigma = 4.0;
    let pad = spread(sigma) as usize;
    let (w, h) = (64, 64);
    let mut m = block(w, h, (32, 32, 1, 1));
    blur(&mut m, w, h, sigma, &mut Scratch::default());
    for y in 0..h {
        for x in 0..w {
            let far = x.abs_diff(32) > pad || y.abs_diff(32) > pad;
            if far {
                assert_eq!(m[y * w + x], 0, "coverage at ({x},{y}) beyond the spread");
            }
        }
    }
}
