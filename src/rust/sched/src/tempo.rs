//! Ticks to samples, and back.
//!
//! **In integers, always.** `rev-core` has `tick_to_second`, which returns `f64`;
//! the compile path does not use it. Three properties depend on staying in
//! integer arithmetic, and each of them is the kind of failure that reproduces
//! once a month on one machine:
//!
//! - **Monotonic.** Two ticks in order must map to samples in order. A rounding
//!   scheme that can invert two adjacent events plays notes out of order.
//! - **Deterministic across platforms** (R-1503). Float division is not
//!   guaranteed identical across targets and optimization settings.
//! - **Non-accumulating.** A tempo map is piecewise constant, so a position is
//!   whole segments plus a partial. Each segment boundary is computed **once and
//!   anchored**; nothing downstream re-derives it, so error cannot compound.

use rev_core::tick::{PPQ, Tick};
use rev_engine::SampleTime;

/// Tempo used before the first tempo point, and when a phrase has no map at all.
/// 120 bpm expressed the way the model stores tempo: integer microseconds per
/// quarter, MIDI-exact (core-01).
pub const DEFAULT_USEC_PER_QUARTER: i64 = 500_000;

const MICROS: i128 = 1_000_000;

/// One constant-tempo span, carrying the sample position of its own start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    from_tick: i64,
    /// Anchored: computed once when the map is built.
    from_sample: u64,
    usec_per_quarter: i64,
}

/// A phrase's tempo map, resolved against one sample rate.
///
/// Both directions live here. The reverse is not a convenience — the counter and
/// the roll need to turn the engine's sample position back into musical time, and
/// if they each did their own arithmetic they would disagree with the compiler at
/// the edges.
#[derive(Debug, Clone)]
pub struct TempoMap {
    /// Ascending by `from_tick`; the first always starts at tick 0.
    segment: Vec<Segment>,
    sample_rate: u32,
}

impl TempoMap {
    /// A constant tempo — the ordinary case, and what an empty map means.
    pub fn constant(usec_per_quarter: i64, sample_rate: u32) -> TempoMap {
        TempoMap {
            segment: vec![Segment {
                from_tick: 0,
                from_sample: 0,
                usec_per_quarter: usec_per_quarter.max(1),
            }],
            sample_rate,
        }
    }

    /// Build from the model's tempo points.
    ///
    /// Points arrive as `(at_tick, usec_per_quarter)`. They are sorted here
    /// rather than trusted, because a map that is out of order would produce a
    /// non-monotonic conversion — the one failure this module exists to prevent.
    ///
    /// **Tempo before the first point is that point's own tempo.** The
    /// alternative — a fixed default before the first point — makes a map that
    /// starts at tick 480 mean something different from the same map starting at
    /// tick 0, which is a surprise nobody wants.
    pub fn new(point: impl IntoIterator<Item = (Tick, i64)>, sample_rate: u32) -> TempoMap {
        let mut point: Vec<(i64, i64)> = point
            .into_iter()
            .map(|(at, upq)| (at.get().max(0), upq.max(1)))
            .collect();
        point.sort_by_key(|&(at, _)| at);
        point.dedup_by_key(|&mut (at, _)| at);

        if point.is_empty() {
            return TempoMap::constant(DEFAULT_USEC_PER_QUARTER, sample_rate);
        }

        // The first segment always starts at tick 0, taking the first point's
        // tempo backwards to the origin.
        let mut segment = Vec::with_capacity(point.len());
        segment.push(Segment {
            from_tick: 0,
            from_sample: 0,
            usec_per_quarter: point[0].1,
        });

        for &(at_tick, usec_per_quarter) in &point {
            if at_tick == 0 {
                continue; // already the opening segment
            }
            // Anchor: this boundary's sample position is computed from the
            // segment that precedes it, once, and stored.
            let previous = *segment.last().expect("at least one segment");
            let from_sample = previous.from_sample
                + span(
                    at_tick - previous.from_tick,
                    previous.usec_per_quarter,
                    sample_rate,
                );
            segment.push(Segment {
                from_tick: at_tick,
                from_sample,
                usec_per_quarter,
            });
        }

        TempoMap {
            segment,
            sample_rate,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Where a tick falls, in samples from the start.
    pub fn sample_at(&self, tick: Tick) -> SampleTime {
        let tick = tick.get().max(0);
        let segment = self.segment_for_tick(tick);
        SampleTime(
            segment.from_sample
                + span(
                    tick - segment.from_tick,
                    segment.usec_per_quarter,
                    self.sample_rate,
                ),
        )
    }

    /// The inverse, for the counter and the roll.
    ///
    /// `tick_at(sample_at(t)) == t` **while a tick is at least a sample wide** —
    /// that is, while `usec_per_quarter × sample_rate >= PPQ × 1_000_000`. Above
    /// roughly 525 bpm at 44.1 kHz (571 at 48 kHz) the model's 5040 ticks per
    /// quarter are finer than the sample grid, several ticks share a sample, and
    /// no inverse can tell them apart. That is a property of the resolutions
    /// involved rather than of this arithmetic, and it was found by the property
    /// test rather than reasoned about — which is why the test now states the
    /// weaker fact for tempos beyond it.
    ///
    /// Nothing downstream depends on exactness there: the compiler converts
    /// forwards, and the counter is displaying to a human.
    pub fn tick_at(&self, at: SampleTime) -> Tick {
        let segment = self.segment_for_sample(at.0);
        let sample = at.0 - segment.from_sample;
        // tick = sample × PPQ × 1e6 ÷ (usec_per_quarter × rate)
        let numerator = i128::from(sample) * i128::from(PPQ) * MICROS;
        let denominator = i128::from(segment.usec_per_quarter) * i128::from(self.sample_rate);
        Tick(segment.from_tick + div_round_even(numerator, denominator) as i64)
    }

    fn segment_for_tick(&self, tick: i64) -> Segment {
        // Linear scan: maps have a handful of points, and a binary search would
        // be more code than the loop it replaces.
        let mut found = self.segment[0];
        for segment in &self.segment {
            if segment.from_tick <= tick {
                found = *segment;
            } else {
                break;
            }
        }
        found
    }

    fn segment_for_sample(&self, sample: u64) -> Segment {
        let mut found = self.segment[0];
        for segment in &self.segment {
            if segment.from_sample <= sample {
                found = *segment;
            } else {
                break;
            }
        }
        found
    }
}

/// Samples spanned by `tick` ticks at a constant tempo.
///
/// `i128` because the intermediate overflows `i64`: at 48 kHz with a half-second
/// quarter, the numerator passes `i64::MAX` after about eleven days of material.
/// Real projects do not reach that; the cost of being sure is one wider integer.
fn span(tick: i64, usec_per_quarter: i64, sample_rate: u32) -> u64 {
    let numerator =
        i128::from(tick.max(0)) * i128::from(usec_per_quarter) * i128::from(sample_rate);
    let denominator = i128::from(PPQ) * MICROS;
    div_round_even(numerator, denominator) as u64
}

/// Divide, rounding halves to even.
///
/// Half-up would bias every conversion a fraction late, and the bias is
/// systematic — the same direction every time, at every tempo boundary.
/// Round-half-to-even has no bias, which is why money and measurement use it.
fn div_round_even(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    debug_assert!(numerator >= 0, "positions are never negative");
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let twice = remainder * 2;
    if twice > denominator || (twice == denominator && quotient % 2 != 0) {
        quotient + 1
    } else {
        quotient
    }
}

#[cfg(test)]
mod test;
