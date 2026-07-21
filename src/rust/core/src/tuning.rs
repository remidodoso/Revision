//! Tunings: the note-number-to-frequency mapping (R-501).
//!
//! Three layers, per the core-01 design. The **definition** (`TuningSpec` plus
//! exact `TuningNote` rows) is authoring truth — rules stay rules, ratios stay
//! exact. **Materialization** compiles a definition into a frozen table of
//! frequencies over the tuning's whole domain; that table is playback truth and
//! is persisted, so a project sounds identical years later regardless of what
//! the builder has since learned. Everything downstream resolves through one
//! materialization, which is the funnel dynamic tuning (R-515) would use: a
//! dynamic tuning is a function of *which* frozen table, never a change to this
//! layer.
//!
//! Two kinds cover the rule-to-arbitrary spectrum. `Equal` is the only rule kept
//! in the schema, because it is the one whose steps cannot be materialized
//! exactly. `Table` carries per-note data — one canonical period for periodic
//! tunings (extended by the period ratio), or the entire domain for aperiodic
//! ones — and covers just intonation, historical temperaments, measured
//! instruments, hand-assigned frequencies, and the output of bespoke generators
//! (which run at authoring time and emit rows; R-413 provenance records the
//! recipe).

pub mod equal;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::id::TuningId;
use crate::note::NoteNumber;

/// An exact frequency ratio. Ratios stay integer pairs (R-504) — never cents,
/// never floats — so just intonation is stored as it is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ratio {
    pub num: i64,
    pub den: i64,
}

impl Ratio {
    pub const OCTAVE: Ratio = Ratio { num: 2, den: 1 };

    pub fn new(num: i64, den: i64) -> Ratio {
        Ratio { num, den }
    }

    pub fn value(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    fn check(self) -> Result<(), CoreError> {
        if self.num <= 0 || self.den <= 0 {
            return Err(CoreError::BadRatio {
                num: self.num,
                den: self.den,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuningKind {
    /// Notes-per-period equal divisions of the period. The only rule the schema
    /// keeps, because its steps are irrational and cannot be stored exactly.
    Equal,
    /// Per-note data: one canonical period (periodic) or the whole domain
    /// (aperiodic).
    Table,
}

impl TuningKind {
    fn as_str(self) -> &'static str {
        match self {
            TuningKind::Equal => "equal",
            TuningKind::Table => "table",
        }
    }
}

/// One note's value in a table tuning: an exact ratio from the anchor, or a
/// direct frequency ("this frequency is this note" — measured instruments,
/// hand-assigned pitches).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuningNoteValue {
    Ratio(Ratio),
    Freq(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TuningNote {
    pub note_number: NoteNumber,
    pub value: TuningNoteValue,
}

/// A tuning definition, without its identity (the store assigns that).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TuningSpec {
    pub name: String,
    pub description: Option<String>,
    pub kind: TuningKind,
    /// The interval of equivalence (R-502) — typically but not necessarily the
    /// octave. `None` means aperiodic: no periodicity, hence no pitch classes,
    /// and every octave-dependent feature switches off.
    pub period: Option<Ratio>,
    pub note_per_period: Option<i32>,
    /// One note bound to a reference frequency (R-503). Builtins anchor note 60
    /// at middle C so switching a phrase's tuning keeps its home pitch.
    pub anchor_note: NoteNumber,
    pub anchor_freq: f64,
    /// The materialization domain for kinds whose rule is unbounded. Aperiodic
    /// table tunings leave these `None`: their rows *are* the domain.
    pub note_min: Option<NoteNumber>,
    pub note_max: Option<NoteNumber>,
    /// R-508 naming scheme (`letter`, `hex`, `near12`, …). Presentation only.
    pub naming: Option<String>,
    pub origin: Option<String>,
    pub seed: Option<serde_json::Value>,
    pub parent_tuning_id: Option<TuningId>,
    pub extra: serde_json::Value,
}

impl TuningSpec {
    /// A minimally-populated spec; callers fill in what they need.
    pub fn new(
        name: impl Into<String>,
        kind: TuningKind,
        anchor_note: i32,
        anchor_freq: f64,
    ) -> Self {
        TuningSpec {
            name: name.into(),
            description: None,
            kind,
            period: None,
            note_per_period: None,
            anchor_note: NoteNumber(anchor_note),
            anchor_freq,
            note_min: None,
            note_max: None,
            naming: None,
            origin: None,
            seed: None,
            parent_tuning_id: None,
            extra: serde_json::json!({}),
        }
    }

    pub fn has_period(&self) -> bool {
        self.period.is_some()
    }

    fn incomplete(&self, missing: &'static str) -> CoreError {
        CoreError::TuningIncomplete {
            name: self.name.clone(),
            kind: self.kind.as_str(),
            missing,
        }
    }
}

/// A tuning definition with its store identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tuning {
    pub id: TuningId,
    pub spec: TuningSpec,
}

/// A frozen note-number-to-frequency table: playback truth.
///
/// Uniform across every kind — rules, ratio tables and measured lists all
/// collapse to the same runtime shape, so consumers never branch on how a
/// tuning was defined. Frequencies are strictly increasing (checked at build),
/// which is what makes [`nearest_note`](Self::nearest_note) a binary search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterializedTuning {
    first_note: NoteNumber,
    freq: Vec<f64>,
    note_per_period: Option<i32>,
}

impl MaterializedTuning {
    /// Build directly from frozen rows — the path used when loading a
    /// materialization back out of the store.
    pub fn from_rows(
        first_note: NoteNumber,
        freq: Vec<f64>,
        note_per_period: Option<i32>,
    ) -> MaterializedTuning {
        MaterializedTuning {
            first_note,
            freq,
            note_per_period,
        }
    }

    /// The frequency of a note, or `None` when it falls outside the tuning's
    /// domain. Out-of-domain notes are dropped at compile rather than clamped:
    /// a note that cannot sound at its pitch does not sound at another.
    pub fn freq(&self, note: NoteNumber) -> Option<f64> {
        let index = note.get().checked_sub(self.first_note.get())?;
        usize::try_from(index)
            .ok()
            .and_then(|i| self.freq.get(i))
            .copied()
    }

    /// The note whose pitch is nearest a frequency, in log-frequency terms.
    ///
    /// Frequencies are increasing, so the insertion point brackets the target;
    /// the geometric-mean test (`hz² < lo·hi`) picks the nearer neighbour
    /// without taking a logarithm.
    pub fn nearest_note(&self, hz: f64) -> Option<NoteNumber> {
        // `is_finite` also rejects NaN, which would otherwise slip past the
        // comparison and give a nonsense partition point.
        if self.freq.is_empty() || !hz.is_finite() || hz <= 0.0 {
            return None;
        }
        let upper = self.freq.partition_point(|&f| f < hz);
        if upper == 0 {
            return Some(self.first_note);
        }
        if upper == self.freq.len() {
            return Some(NoteNumber(
                self.first_note.get() + self.freq.len() as i32 - 1,
            ));
        }
        let lo = self.freq[upper - 1];
        let hi = self.freq[upper];
        let index = if hz * hz < lo * hi { upper - 1 } else { upper };
        Some(NoteNumber(self.first_note.get() + index as i32))
    }

    /// The inclusive note range this materialization covers.
    pub fn note_range(&self) -> (NoteNumber, NoteNumber) {
        (
            self.first_note,
            NoteNumber(self.first_note.get() + self.freq.len().max(1) as i32 - 1),
        )
    }

    pub fn note_per_period(&self) -> Option<i32> {
        self.note_per_period
    }

    /// The gate for every octave-dependent feature (pitch classes, octave
    /// transpose, pitch-class-set analysis). False for aperiodic tunings.
    pub fn has_period(&self) -> bool {
        self.note_per_period.is_some()
    }

    pub fn len(&self) -> usize {
        self.freq.len()
    }

    pub fn is_empty(&self) -> bool {
        self.freq.is_empty()
    }

    /// The frozen rows, in note order from `first_note` — what the store writes.
    pub fn rows(&self) -> impl Iterator<Item = (NoteNumber, f64)> + '_ {
        // Index → note number, paired with its frequency; the store inserts
        // these verbatim into materialized_tuning.
        self.freq
            .iter()
            .enumerate()
            .map(|(i, &f)| (NoteNumber(self.first_note.get() + i as i32), f))
    }
}

/// Compile a definition into a frozen table.
pub fn materialize(
    spec: &TuningSpec,
    note: &[TuningNote],
) -> Result<MaterializedTuning, CoreError> {
    let freq = match spec.kind {
        TuningKind::Equal => materialize_equal(spec)?,
        TuningKind::Table => materialize_table(spec, note)?,
    };
    let first_note = match spec.kind {
        TuningKind::Equal => spec
            .note_min
            .ok_or_else(|| spec.incomplete("has no note_min"))?,
        TuningKind::Table => {
            if spec.has_period() {
                spec.note_min
                    .ok_or_else(|| spec.incomplete("has no note_min"))?
            } else {
                note.iter()
                    .map(|n| n.note_number)
                    .min()
                    .ok_or_else(|| CoreError::TuningEmpty {
                        name: spec.name.clone(),
                    })?
            }
        }
    };
    check_monotone(spec, first_note, &freq)?;
    Ok(MaterializedTuning {
        first_note,
        freq,
        note_per_period: spec.note_per_period,
    })
}

fn materialize_equal(spec: &TuningSpec) -> Result<Vec<f64>, CoreError> {
    let period = spec
        .period
        .ok_or_else(|| spec.incomplete("has no period"))?;
    period.check()?;
    let note_per_period = spec
        .note_per_period
        .filter(|&n| n > 0)
        .ok_or_else(|| spec.incomplete("has no positive note_per_period"))?;
    let (min, max) = domain(spec)?;

    let log2_period = equal::log2_ratio(period.num, period.den);
    let anchor = spec.anchor_note.get();
    // freq(n) = anchor · period^((n − anchor)/N) — the one irrational case, so
    // the exponentiation is ours (R-501).
    Ok((min.get()..=max.get())
        .map(|n| {
            let steps = f64::from(n - anchor) / f64::from(note_per_period);
            spec.anchor_freq * equal::exp2(steps * log2_period)
        })
        .collect())
}

fn materialize_table(spec: &TuningSpec, note: &[TuningNote]) -> Result<Vec<f64>, CoreError> {
    if note.is_empty() {
        return Err(CoreError::TuningEmpty {
            name: spec.name.clone(),
        });
    }
    // Each row's value as a multiplier on the anchor frequency.
    let value_of = |n: &TuningNote| -> Result<f64, CoreError> {
        match n.value {
            TuningNoteValue::Ratio(r) => {
                r.check()?;
                Ok(r.value())
            }
            TuningNoteValue::Freq(f) => Ok(f / spec.anchor_freq),
        }
    };

    match spec.period {
        // Periodic: the rows cover one canonical period starting at the anchor,
        // and the period ratio extends them in both directions for free.
        Some(period) => {
            period.check()?;
            let note_per_period = spec
                .note_per_period
                .filter(|&n| n > 0)
                .ok_or_else(|| spec.incomplete("has no positive note_per_period"))?;
            if note.len() != note_per_period as usize {
                return Err(CoreError::TuningNoteCount {
                    name: spec.name.clone(),
                    expected: note_per_period,
                    found: note.len(),
                });
            }
            let anchor = spec.anchor_note.get();
            let mut canonical = vec![0.0; note_per_period as usize];
            let mut seen = vec![false; note_per_period as usize];
            for row in note {
                let offset = row.note_number.get() - anchor;
                if offset < 0 || offset >= note_per_period {
                    return Err(CoreError::TuningNoteGap {
                        name: spec.name.clone(),
                    });
                }
                canonical[offset as usize] = value_of(row)?;
                seen[offset as usize] = true;
            }
            if seen.iter().any(|&s| !s) {
                return Err(CoreError::TuningNoteGap {
                    name: spec.name.clone(),
                });
            }

            let (min, max) = domain(spec)?;
            Ok((min.get()..=max.get())
                .map(|n| {
                    let period_index =
                        NoteNumber(n).period_index(spec.anchor_note, note_per_period);
                    let position = (n - anchor).rem_euclid(note_per_period) as usize;
                    spec.anchor_freq
                        * equal::ratio_powi(period.num, period.den, period_index)
                        * canonical[position]
                })
                .collect())
        }
        // Aperiodic: the rows are the whole domain, and must be contiguous so
        // the frozen table stays a dense array.
        None => {
            let mut rows = note.to_vec();
            rows.sort_by_key(|r| r.note_number.get());
            let first = rows[0].note_number.get();
            for (i, row) in rows.iter().enumerate() {
                if row.note_number.get() != first + i as i32 {
                    return Err(CoreError::TuningNoteGap {
                        name: spec.name.clone(),
                    });
                }
            }
            rows.iter()
                .map(|row| value_of(row).map(|v| spec.anchor_freq * v))
                .collect()
        }
    }
}

fn domain(spec: &TuningSpec) -> Result<(NoteNumber, NoteNumber), CoreError> {
    let min = spec
        .note_min
        .ok_or_else(|| spec.incomplete("has no note_min"))?;
    let max = spec
        .note_max
        .ok_or_else(|| spec.incomplete("has no note_max"))?;
    if max.get() < min.get() {
        return Err(spec.incomplete("has note_max below note_min"));
    }
    Ok((min, max))
}

/// Frequencies must be finite, positive and strictly increasing — the invariant
/// `nearest_note`'s binary search rests on, and a genuine check on hand-written
/// tables (a mistyped ratio shows up here rather than as a silent wrong pitch).
fn check_monotone(
    spec: &TuningSpec,
    first_note: NoteNumber,
    freq: &[f64],
) -> Result<(), CoreError> {
    let mut previous = f64::NEG_INFINITY;
    for (i, &f) in freq.iter().enumerate() {
        let note = first_note.get() + i as i32;
        if !f.is_finite() || f <= 0.0 {
            return Err(CoreError::TuningBadFrequency {
                name: spec.name.clone(),
                note,
            });
        }
        if f <= previous {
            return Err(CoreError::TuningNotMonotone {
                name: spec.name.clone(),
                note,
                freq: f,
            });
        }
        previous = f;
    }
    Ok(())
}

#[cfg(test)]
mod test;
