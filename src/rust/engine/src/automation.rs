//! Parameter automation: the four methods, per the W3C normative formulas.
//!
//! **Four, not the whole `AudioParam` surface.** The scope was settled by census
//! rather than by reading the specification end to end: the voice being ported
//! uses `setValueAtTime`, `linearRampToValueAtTime`,
//! `exponentialRampToValueAtTime` and `setTargetAtTime`, and nothing else
//! (`doc/revision_padlington_inventory.md` §4).
//!
//! Everything here is allocation-free and fixed-capacity, because a note-off
//! schedules a release **on the audio thread**. The event list is a small array;
//! inserting keeps it sorted with an insertion pass, which for eight elements is
//! cheaper than anything cleverer and has no failure mode.
//!
//! Time is in **frames since the voice started**. Seconds never appear: the
//! conversion happens where the instrument is built, and the release is
//! scheduled from the voice's own counter.

/// How many scheduled events one parameter can hold.
///
/// The Padlington envelope needs four (attack ramp, decay target, release
/// target, and one spare for the pitch attack). Eight leaves room without making
/// a voice large — a 16-voice instrument with six parameters holds 768 events,
/// which is nothing.
pub const EVENT_CAPACITY: usize = 8;

/// What happens between the previous value and this event's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    /// Jump to the value at this time and hold it.
    Set,
    /// Straight line from the previous value to this one.
    Linear,
    /// Constant ratio per unit time — a straight line in decibels, which is why
    /// it is the right curve for a filter cutoff in octaves.
    Exponential,
    /// Approach the value asymptotically, never arriving. `tau` is the time
    /// constant in frames: after `tau` the remaining distance is `1/e`.
    Target { tau: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    pub at: u64,
    pub value: f32,
    pub curve: Curve,
}

/// A parameter's schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct Automation {
    event: [Event; EVENT_CAPACITY],
    len: usize,
    /// The value before any event applies.
    initial: f32,
    /// Events that would not fit. Counted rather than dropped in silence — a
    /// missing envelope stage is audible and otherwise unattributable.
    lost: u32,
}

impl Automation {
    pub fn new(initial: f32) -> Automation {
        Automation {
            event: [Event {
                at: 0,
                value: 0.0,
                curve: Curve::Set,
            }; EVENT_CAPACITY],
            len: 0,
            initial,
            lost: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn lost(&self) -> u32 {
        self.lost
    }

    pub fn initial(&self) -> f32 {
        self.initial
    }

    /// Forget every scheduled event and start again from `value`. What a voice
    /// does when it is taken for a new note.
    pub fn reset(&mut self, value: f32) {
        self.len = 0;
        self.initial = value;
        self.lost = 0;
    }

    /// Drop events at or after `from` — what a note-off does before scheduling a
    /// release, so a decay scheduled into the future cannot fight it.
    pub fn cancel_from(&mut self, from: u64) {
        self.len = self.event[..self.len]
            .iter()
            .position(|e| e.at >= from)
            .unwrap_or(self.len);
    }

    pub fn set_value_at_time(&mut self, value: f32, at: u64) -> bool {
        self.insert(Event {
            at,
            value,
            curve: Curve::Set,
        })
    }

    pub fn linear_ramp_to_value_at_time(&mut self, value: f32, at: u64) -> bool {
        self.insert(Event {
            at,
            value,
            curve: Curve::Linear,
        })
    }

    pub fn exponential_ramp_to_value_at_time(&mut self, value: f32, at: u64) -> bool {
        self.insert(Event {
            at,
            value,
            curve: Curve::Exponential,
        })
    }

    /// `tau` in frames. A zero time constant jumps, which is what the
    /// specification says and is also the only sensible reading.
    pub fn set_target_at_time(&mut self, target: f32, at: u64, tau: f32) -> bool {
        self.insert(Event {
            at,
            value: target,
            curve: Curve::Target { tau: tau.max(0.0) },
        })
    }

    /// Keep the list sorted by time. Insertion rather than append-then-sort
    /// because events usually arrive in order and this is then a single
    /// comparison; out of order is permitted, as the specification allows.
    fn insert(&mut self, event: Event) -> bool {
        if self.len == EVENT_CAPACITY {
            self.lost += 1;
            return false;
        }
        let mut index = self.len;
        while index > 0 && self.event[index - 1].at > event.at {
            self.event[index] = self.event[index - 1];
            index -= 1;
        }
        self.event[index] = event;
        self.len += 1;
        true
    }

    /// The value at a frame.
    ///
    /// Evaluated forward from the start rather than by seeking to the
    /// surrounding pair. With at most eight events that costs nothing, and it
    /// makes the awkward case — a `Target` whose effect continues past its own
    /// time until something supersedes it — fall out instead of needing a rule.
    /// ## A ramp's starting value
    ///
    /// A ramp interpolates from **the value at the previous event's time** —
    /// `T0` and `V0` in the specification's formulas — over the whole span
    /// between them.
    ///
    /// That has a consequence worth stating, because it is surprising and
    /// because our reading of the specification's text on this corner is *not*
    /// verified: **a ramp following a `setTarget` ignores the target's
    /// progress.** The target's value at its own instant is the value it started
    /// from, so that is what the ramp ramps from, and the approach in between
    /// contributes nothing.
    ///
    /// It does not arise in the voice being ported — its envelope is a linear
    /// attack ramp, then a decay target, then a release target, and no ramp ever
    /// follows a target. If a patch ever wants that combination, this is the
    /// behaviour to check first.
    pub fn value_at(&self, frame: u64) -> f32 {
        // `current` is the value at `frame` given everything so far; `anchor` is
        // the value at `previous_at`, which is what a ramp needs. They differ
        // only while a target is still approaching.
        let mut current = self.initial;
        let mut anchor = self.initial;
        let mut previous_at = 0u64;

        for index in 0..self.len {
            let event = self.event[index];

            if frame < event.at {
                // Between the last event and this one. Only ramps do anything
                // here; a set or a target has not happened yet.
                return match event.curve {
                    Curve::Linear => lerp(anchor, event.value, previous_at, event.at, frame),
                    Curve::Exponential => {
                        exponential(anchor, event.value, previous_at, event.at, frame)
                    }
                    Curve::Set | Curve::Target { .. } => current,
                };
            }

            match event.curve {
                Curve::Set | Curve::Linear | Curve::Exponential => {
                    current = event.value;
                    anchor = event.value;
                }
                Curve::Target { tau } => {
                    // A target keeps working until the next event supersedes it,
                    // so how far it got depends on where we stopped looking.
                    anchor = current;
                    let until = if index + 1 < self.len {
                        self.event[index + 1].at.min(frame)
                    } else {
                        frame
                    };
                    current = approach(current, event.value, until.saturating_sub(event.at), tau);
                }
            }
            previous_at = event.at;
        }
        current
    }
}

fn lerp(from: f32, to: f32, start: u64, end: u64, frame: u64) -> f32 {
    if end <= start {
        return to;
    }
    let progress = (frame - start) as f32 / (end - start) as f32;
    from + (to - from) * progress
}

/// The specification's exponential ramp: `v(t) = V0 · (V1/V0)^((t−T0)/(T1−T0))`.
///
/// **With its stated special case**, which is easy to miss and audible when it
/// is: if `V0` is zero or the two have opposite signs, the value *holds* at `V0`
/// until the end and then jumps. Exponential interpolation through zero has no
/// meaning, and the specification says so rather than producing a NaN.
fn exponential(from: f32, to: f32, start: u64, end: u64, frame: u64) -> f32 {
    if end <= start {
        return to;
    }
    if from == 0.0 || to == 0.0 || from.signum() != to.signum() {
        return from;
    }
    let progress = (frame - start) as f32 / (end - start) as f32;
    from * (to / from).powf(progress)
}

/// `v(t) = V1 + (V0 − V1) · e^(−t/τ)` — the specification's `setTargetAtTime`.
///
/// Evaluated in closed form rather than as a one-pole recursion. The recursion
/// is the usual optimization and would be cheaper, but it accumulates its own
/// state and is therefore not the same function at every sample rate; the closed
/// form is exact, matches the specification's own words, and at the measured
/// headroom (68 µs of a 10 ms budget) costs nothing we have.
fn approach(from: f32, target: f32, elapsed: u64, tau: f32) -> f32 {
    if tau <= 0.0 {
        return target;
    }
    target + (from - target) * (-(elapsed as f32) / tau).exp()
}

#[cfg(test)]
mod test;
