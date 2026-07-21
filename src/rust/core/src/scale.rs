//! Scales: named subsets of a tuning's pitch classes, or of its note numbers
//! when the tuning is aperiodic (R-509).
//!
//! A scale stores a **shape**, never a rooted result: the mask is relative to
//! the tonic, and the root is supplied at every use site. So "major" is one row
//! serving every root and every 12-notes-per-period tuning — 12-ET and 5-limit
//! just intonation alike — rather than one row per key per tuning.
//!
//! Applicability is mechanical (the mask's modulus must match the tuning's
//! notes-per-period) and is the only relationship the model enforces. Whether a
//! mask *makes musical sense* over a particular tuning and root — a diatonic
//! mask over a 12-note tuning with perverse step sizes — is idiomatic fit:
//! advisory, computed or curated, ranked in pickers, never prohibited (R-510).

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::id::{ScaleId, TuningId};
use crate::note::NoteNumber;

/// A scale definition, without its identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleSpec {
    pub name: String,
    pub description: Option<String>,
    /// The modulus a periodic mask belongs to — its structural parent. Exactly
    /// one of this and `tuning_id` is set.
    pub note_per_period: Option<i32>,
    /// Aperiodic scales are subsets of absolute note numbers, so they belong to
    /// one tuning.
    pub tuning_id: Option<TuningId>,
    /// Periodic: root-relative offsets in `[0, note_per_period)`.
    /// Aperiodic: absolute note numbers.
    pub mask: Vec<i32>,
    pub origin: Option<String>,
    pub seed: Option<serde_json::Value>,
    pub parent_scale_id: Option<ScaleId>,
    pub extra: serde_json::Value,
}

impl ScaleSpec {
    /// A periodic mask: the ordinary case.
    pub fn periodic(name: impl Into<String>, note_per_period: i32, mask: Vec<i32>) -> ScaleSpec {
        ScaleSpec {
            name: name.into(),
            description: None,
            note_per_period: Some(note_per_period),
            tuning_id: None,
            mask,
            origin: None,
            seed: None,
            parent_scale_id: None,
            extra: serde_json::json!({}),
        }
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.mask.is_empty() {
            return Err(CoreError::ScaleEmpty {
                name: self.name.clone(),
            });
        }
        if let Some(modulus) = self.note_per_period
            && let Some(&bad) = self.mask.iter().find(|&&m| m < 0 || m >= modulus)
        {
            return Err(CoreError::ScaleMaskOutOfRange {
                name: self.name.clone(),
                modulus,
                value: bad,
            });
        }
        Ok(())
    }

    /// Is this mask applicable to a tuning with the given periodicity? The
    /// mechanical test only — idiomatic fit is a separate, advisory question.
    pub fn applies_to(&self, note_per_period: Option<i32>) -> bool {
        match (self.note_per_period, note_per_period) {
            (Some(mask_modulus), Some(tuning_modulus)) => mask_modulus == tuning_modulus,
            (None, None) => true,
            _ => false,
        }
    }

    /// Membership: is `note` in this scale, rooted at pitch class `root`?
    ///
    /// A `None` scale binding means chromatic — every note is in scale — so
    /// there is no chromatic row to special-case (unlike the JS lab).
    pub fn contains(&self, note: NoteNumber, root: i32) -> bool {
        match self.note_per_period {
            Some(modulus) => {
                let class = (note.get() - root).rem_euclid(modulus);
                self.mask.contains(&class)
            }
            None => self.mask.contains(&note.get()),
        }
    }

    /// The nearest in-scale note, preferring the lower on a tie.
    pub fn nearest(&self, note: NoteNumber, root: i32) -> NoteNumber {
        if self.contains(note, root) {
            return note;
        }
        match self.note_per_period {
            // Periodic: a mask member is never more than one period away.
            Some(modulus) => {
                for distance in 1..=modulus.max(1) {
                    let below = note.offset(-distance);
                    if self.contains(below, root) {
                        return below;
                    }
                    let above = note.offset(distance);
                    if self.contains(above, root) {
                        return above;
                    }
                }
                note
            }
            // Aperiodic: the mask is an absolute set, so scan it directly.
            // Ordering by (distance, note) puts the lower note first on a tie.
            None => self
                .mask
                .iter()
                .copied()
                .min_by_key(|&m| ((note.get() - m).abs(), m))
                .map(NoteNumber)
                .unwrap_or(note),
        }
    }

    /// The next in-scale note strictly above (`direction > 0`) or below — one
    /// scale degree of motion. An off-scale note lands on the first mask member
    /// past it, so stepping snaps onto the scale as it moves.
    pub fn step(&self, note: NoteNumber, root: i32, direction: i32) -> NoteNumber {
        let ascending = direction >= 0;
        let unit = if ascending { 1 } else { -1 };
        match self.note_per_period {
            Some(modulus) => {
                // Two periods is always enough to clear any gap in a mask.
                for distance in 1..=(modulus.max(1) * 2) {
                    let candidate = note.offset(unit * distance);
                    if self.contains(candidate, root) {
                        return candidate;
                    }
                }
                note.offset(unit)
            }
            // Aperiodic: the nearest mask member strictly past `note`.
            None => {
                let found = if ascending {
                    self.mask.iter().copied().filter(|&m| m > note.get()).min()
                } else {
                    self.mask.iter().copied().filter(|&m| m < note.get()).max()
                };
                found.map(NoteNumber).unwrap_or(note)
            }
        }
    }
}

/// A scale definition with its store identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scale {
    pub id: ScaleId,
    pub spec: ScaleSpec,
}

#[cfg(test)]
mod test;
