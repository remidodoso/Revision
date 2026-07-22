//! Mapping a monotonic instant to a sample position (midi-01 §4, R-603).
//!
//! A MIDI event is stamped, at the driver boundary, with a monotonic instant in
//! the engine's own clock domain. To place it on the timeline it has to become
//! a **sample position** — and the engine already tells us how, publishing a
//! `(sample, nanos)` pair every block over the position seqlock. Fit a line
//! through a short history of those pairs and any instant interpolates to a
//! sample.
//!
//! **Live play barely needs this** — a note is played as soon as it arrives, not
//! placed precisely. It earns its keep at recording, where the timestamp decides
//! where a note lands (R-810). So the module is built and tested here and
//! exercised there.
//!
//! The slope of the fit is the observed sample rate, drift and all — a number
//! worth displaying (R-814), and the thing that makes this a correlation rather
//! than an assumption.

/// One observation: this sample position was seen at this monotonic instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    pub sample: u64,
    pub nanos: u64,
}

/// A rolling least-squares fit of sample-position against monotonic time.
///
/// Bounded history: old pairs age out, so the fit tracks a clock that drifts
/// rather than averaging its whole past. A ring, no allocation after
/// construction.
#[derive(Debug, Clone)]
pub struct Correlation {
    pair: Vec<Pair>,
    head: usize,
    len: usize,
    capacity: usize,
}

impl Correlation {
    /// `window` pairs of history. At ~375 blocks a second (the k-rate), a few
    /// dozen spans a fraction of a second — enough to average out jitter, short
    /// enough to follow drift.
    pub fn new(window: usize) -> Correlation {
        let capacity = window.max(2);
        Correlation {
            pair: vec![
                Pair {
                    sample: 0,
                    nanos: 0
                };
                capacity
            ],
            head: 0,
            len: 0,
            capacity,
        }
    }

    /// Record a pair the engine published. Monotonic in both fields, so a stale
    /// or duplicate reading (the seqlock republishing the same value) is
    /// dropped rather than skewing the fit.
    pub fn observe(&mut self, pair: Pair) {
        if let Some(last) = self.last()
            && pair.nanos <= last.nanos
        {
            return;
        }
        self.pair[self.head] = pair;
        self.head = (self.head + 1) % self.capacity;
        self.len = (self.len + 1).min(self.capacity);
    }

    fn last(&self) -> Option<Pair> {
        if self.len == 0 {
            return None;
        }
        Some(self.pair[(self.head + self.capacity - 1) % self.capacity])
    }

    fn iter(&self) -> impl Iterator<Item = Pair> + '_ {
        let start = (self.head + self.capacity - self.len) % self.capacity;
        (0..self.len).map(move |i| self.pair[(start + i) % self.capacity])
    }

    /// Samples per nanosecond, from the fit — the observed sample rate scaled by
    /// 1e-9. `None` until there are two distinct pairs.
    pub fn samples_per_nano(&self) -> Option<f64> {
        if self.len < 2 {
            return None;
        }
        // Least squares against a shifted origin (the first pair), so the sums
        // stay small and an f64 keeps its precision even at large sample counts.
        let base = self.iter().next()?;
        let (mut sx, mut sy, mut sxx, mut sxy, mut n) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for p in self.iter() {
            let x = (p.nanos - base.nanos) as f64;
            let y = (p.sample as f64) - (base.sample as f64);
            sx += x;
            sy += y;
            sxx += x * x;
            sxy += x * y;
            n += 1.0;
        }
        let denom = n * sxx - sx * sx;
        if denom.abs() < f64::EPSILON {
            return None;
        }
        Some((n * sxy - sx * sy) / denom)
    }

    /// The sample position an instant maps to. `None` until the fit exists.
    ///
    /// Extrapolates freely: a MIDI event's instant is often a hair *after* the
    /// last published pair, so the answer is a short reach beyond the history,
    /// which a line handles honestly.
    pub fn sample_at(&self, nanos: u64) -> Option<f64> {
        let slope = self.samples_per_nano()?;
        // Anchor on the mean of the window, which is where a least-squares line
        // is most trustworthy.
        let (mut mean_n, mut mean_s, mut count) = (0.0f64, 0.0f64, 0.0f64);
        let base = self.iter().next()?;
        for p in self.iter() {
            mean_n += (p.nanos - base.nanos) as f64;
            mean_s += (p.sample as f64) - (base.sample as f64);
            count += 1.0;
        }
        mean_n /= count;
        mean_s /= count;
        let x = (nanos as f64) - (base.nanos as f64);
        Some((base.sample as f64) + mean_s + slope * (x - mean_n))
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod test;
