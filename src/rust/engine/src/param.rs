//! A node's parameters.
//!
//! **The math is eng-04's.** This is where a parameter *attaches*: its default,
//! whether it is evaluated per sample or per quantum, and — later — the schedule
//! of ramps and the optional node driving it.
//!
//! The vocabulary eng-04 will implement was settled by census rather than by
//! reading the specification: the voice being ported uses exactly four methods
//! (`setValueAtTime`, `linearRampToValueAtTime`, `exponentialRampToValueAtTime`,
//! `setTargetAtTime`), and that is the whole scope.

use crate::automation::Automation;
use crate::graph::ParamId;

/// One parameter of one node.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub id: ParamId,
    /// The value when nothing is scheduled and nothing drives it.
    pub value: f32,
    /// Per-sample or per-quantum, fixed at build. Never dynamic: the cost
    /// difference is large and the answer never changes at run time.
    pub audio_rate: bool,
    /// The schedule (eng-04). Empty means the constant `value`.
    pub automation: Automation,
    // Still deferred: an optional node driving this parameter at audio rate.
    // A field added later costs nothing; a wrong one costs a migration.
}

impl Param {
    pub fn constant(id: ParamId, value: f32) -> Param {
        Param {
            id,
            value,
            audio_rate: id.is_audio_rate(),
            automation: Automation::new(value),
        }
    }

    /// The value at a frame of the voice's life.
    ///
    /// A parameter with nothing scheduled skips the evaluation entirely, which
    /// is most of them most of the time — a filter's Q is set once and never
    /// automated, and it should not pay for the envelope machinery.
    pub fn at(&self, frame: u64) -> f32 {
        if self.automation.is_empty() {
            self.value
        } else {
            self.automation.value_at(frame)
        }
    }

    /// Set the constant value and forget any schedule.
    pub fn set(&mut self, value: f32) {
        self.value = value;
        self.automation.reset(value);
    }

    pub fn schedule(&mut self) -> &mut Automation {
        &mut self.automation
    }
}
