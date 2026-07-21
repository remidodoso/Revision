//! Blur for shadow masks.
//!
//! A **three-pass box blur** on 8-bit coverage. Three boxes converge on a Gaussian
//! closely enough that the difference is invisible at shadow radii, and it costs a
//! running sum per row rather than a convolution — which is what CSS `box-shadow`
//! and Skia effectively do. Written here rather than taken as a dependency: it is
//! under a hundred lines, and the posture is to roll anything this small.
//!
//! Integer arithmetic throughout, so the result is bit-identical everywhere — the
//! property the screenshot golden masters rest on.
//!
//! **Transposition was tried and rejected**, with numbers. A vertical box blur
//! strides by `width` and reads one byte per cache line, so transposing between
//! passes to make it sequential looks obviously right — and measured *worse*
//! (43 ns/px against 33) at the mask sizes a skin actually casts. A shadow mask is
//! a few thousand bytes and already sits in L1; there is no stride penalty to
//! avoid, and the transposes are pure added work. Kept here as a warning: the
//! cache argument only applies to masks far larger than any widget casts.
//! See `benches/blur.rs`.

/// Scratch buffers, reused across every shadow in a frame. Allocating per shadow
/// showed up as a measurable share of a panel repaint.
#[derive(Default)]
pub(crate) struct Scratch {
    buffer: Vec<u8>,
    /// `quotient[s] == s / window`, for every sum a window can hold.
    ///
    /// A box blur divides once per pixel per pass — six divisions per pixel, and
    /// integer division is the dearest instruction in the loop by an order of
    /// magnitude. The sums are small (255 × window, a few thousand at most), so an
    /// exact table replaces the divide with a load. Exact, not approximate: a
    /// reciprocal-multiply would shift golden masters for nothing.
    quotient: Vec<u8>,
    window: u32,
}

/// Blur `mask` in place. `sigma` is in pixels; zero or negative leaves it alone.
///
/// The box radius is derived from sigma by the standard equivalence (a box of
/// half-width `r` applied three times approximates a Gaussian of `sigma ≈ r*0.75`),
/// rounded to an integer so the result cannot drift.
pub(crate) fn blur(mask: &mut [u8], width: usize, height: usize, sigma: f32, s: &mut Scratch) {
    if sigma <= 0.0 || width == 0 || height == 0 {
        return;
    }
    let radius = ((sigma / 0.75).round() as usize).max(1);
    let window = (radius * 2 + 1) as u32;
    let len = width * height;
    // Split the borrow: the table and the buffer are siblings inside Scratch.
    let Scratch {
        buffer,
        quotient,
        window: cached,
    } = s;
    if *cached != window {
        quotient.clear();
        quotient.extend((0..=255 * window).map(|v| (v / window) as u8));
        *cached = window;
    }
    if buffer.len() < len {
        buffer.resize(len, 0);
    }
    let flat = &mut buffer[..len];

    for _ in 0..3 {
        box_rows(mask, flat, width, height, radius, quotient);
        box_columns(flat, mask, width, height, radius, quotient);
    }
}

/// How far a blur of this sigma spreads, for sizing the buffer it needs.
pub(crate) fn spread(sigma: f32) -> f32 {
    if sigma <= 0.0 {
        return 0.0;
    }
    // Three boxes of half-width r reach 3r; round up so nothing is clipped.
    ((sigma / 0.75).round() * 3.0).ceil() + 1.0
}

/// One horizontal box pass, edges clamped. The running sum makes the cost
/// independent of the radius, which is why a wide shadow is no dearer than a tight
/// one. Row slices are taken up front so the inner loop is bounds-check free.
fn box_rows(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize, q: &[u8]) {
    for y in 0..height {
        let s = &src[y * width..(y + 1) * width];
        let d = &mut dst[y * width..(y + 1) * width];
        let last = width - 1;

        // Prime with the leading window, clamping past both edges.
        let mut sum = u32::from(s[0]) * (radius as u32 + 1);
        for x in 1..=radius {
            sum += u32::from(s[x.min(last)]);
        }
        for (x, out) in d.iter_mut().enumerate() {
            *out = q[sum as usize];
            let leaving = s[x.saturating_sub(radius)];
            let entering = s[(x + radius + 1).min(last)];
            sum = sum - u32::from(leaving) + u32::from(entering);
        }
    }
}

/// One vertical box pass, striding by `width`. Same running sum; the index
/// arithmetic is hoisted so the inner loop is two loads and a store.
fn box_columns(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize, q: &[u8]) {
    let last = height - 1;
    for x in 0..width {
        let mut sum = u32::from(src[x]) * (radius as u32 + 1);
        for y in 1..=radius {
            sum += u32::from(src[y.min(last) * width + x]);
        }
        for y in 0..height {
            dst[y * width + x] = q[sum as usize];
            let leaving = src[y.saturating_sub(radius) * width + x];
            let entering = src[(y + radius + 1).min(last) * width + x];
            sum = sum - u32::from(leaving) + u32::from(entering);
        }
    }
}

#[cfg(test)]
mod test;
