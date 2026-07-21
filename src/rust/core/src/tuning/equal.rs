//! Owned exponentiation and logarithm (R-501: the note-number-to-frequency
//! mapping shall be deterministic and identical across platforms).
//!
//! `f64::powf`/`exp2`/`log2` bottom out in the platform's libm, which is *not*
//! guaranteed to produce identical bits on every target — so equal temperaments,
//! whose steps are irrational, own their math here. Everything below is plain
//! IEEE-754 add/subtract/multiply/divide plus exact power-of-two scaling; Rust
//! never contracts to FMA implicitly, so every target evaluates the same
//! operations in the same order and gets the same bits.
//!
//! These are reproducible, not correctly-rounded: expect a few ulps of error
//! against an ideal `exp2`, which is inaudible (~1e-13 cents) and — more to the
//! point — identical everywhere.

/// Multiply by 2^exponent exactly, by constructing the power of two directly.
/// Exact for any representable result: no rounding is introduced.
fn scale_by_pow2(value: f64, exponent: i32) -> f64 {
    // Split extreme exponents into steps so the constructed f64 stays normal.
    let mut out = value;
    let mut remaining = exponent;
    while remaining > 1023 {
        out *= f64::from_bits((2046u64) << 52); // 2^1023
        remaining -= 1023;
    }
    while remaining < -1022 {
        out *= f64::from_bits(1u64 << 52); // 2^-1022
        remaining += 1022;
    }
    out * f64::from_bits(((remaining + 1023) as u64) << 52)
}

/// 2^y.
///
/// Split y into an integer part (exact, applied as a power-of-two scale) and a
/// fraction in [0, 1). The fraction is halved four times into [0, 1/16], where a
/// twelve-term Taylor series for `exp` converges far past double precision, then
/// squared back up — four squarings cost about two ulps.
pub fn exp2(y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    if y == f64::INFINITY {
        return f64::INFINITY;
    }
    if y == f64::NEG_INFINITY {
        return 0.0;
    }

    let whole = y.floor();
    let fraction = y - whole; // [0, 1)

    // exp(z) where z = (fraction/16)·ln2 ≤ 0.0434: the k=12 term is ~1e-23.
    let z = (fraction / 16.0) * std::f64::consts::LN_2;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..=12 {
        term *= z / f64::from(k);
        sum += term;
    }

    // Undo the four halvings.
    let mut reduced = sum;
    for _ in 0..4 {
        reduced *= reduced;
    }

    scale_by_pow2(reduced, whole as i32)
}

/// log2(x) for finite x > 0; NaN for x < 0, −∞ for x = 0.
///
/// Decompose x = m·2^e with m in [1, 2), narrow m to [√½, √2) so the argument of
/// the series is small, then use ln(m) = 2·atanh((m−1)/(m+1)) — whose argument
/// is at most 0.1716, so the series converges to below 1e-19 within the terms
/// taken.
pub fn log2(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return f64::INFINITY;
    }

    // Normalize subnormals into the normal range before touching the exponent.
    let (x, subnormal_shift) = if x < f64::MIN_POSITIVE {
        (x * scale_by_pow2(1.0, 64), 64)
    } else {
        (x, 0)
    };

    let bits = x.to_bits();
    let mut exponent = (((bits >> 52) & 0x7ff) as i32) - 1023 - subnormal_shift;
    let mut mantissa = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | (1023u64 << 52));
    if mantissa > std::f64::consts::SQRT_2 {
        mantissa *= 0.5;
        exponent += 1;
    }

    let t = (mantissa - 1.0) / (mantissa + 1.0);
    let t_squared = t * t;
    let mut term = t;
    let mut sum = t;
    for k in (3..=25).step_by(2) {
        term *= t_squared;
        sum += term / f64::from(k);
    }

    f64::from(exponent) + 2.0 * sum / std::f64::consts::LN_2
}

/// log2 of an exact ratio, with the octave short-circuited.
///
/// 2/1 is overwhelmingly the common case and its logarithm is exactly 1, so
/// octave-periodic tunings (12-ET, 16-ET, …) never touch the series at all.
pub fn log2_ratio(num: i64, den: i64) -> f64 {
    if num == 2 && den == 1 {
        return 1.0;
    }
    log2(num as f64) - log2(den as f64)
}

/// ratio^exponent for an integer exponent, by repeated multiplication —
/// deterministic, and exact when the ratio is a power of two.
pub fn ratio_powi(num: i64, den: i64, exponent: i32) -> f64 {
    if num == 2 && den == 1 {
        return scale_by_pow2(1.0, exponent);
    }
    let base = if exponent >= 0 {
        num as f64 / den as f64
    } else {
        den as f64 / num as f64
    };
    let mut out = 1.0;
    for _ in 0..exponent.abs() {
        out *= base;
    }
    out
}

#[cfg(test)]
mod test;
