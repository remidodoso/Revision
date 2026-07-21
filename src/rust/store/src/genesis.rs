//! Project genesis: the first gesture of every project's life.
//!
//! Because the journal is the only write path, creation is itself journaled —
//! builtins arrive through the same commands a user's own tuning would. Three
//! things follow for free: replay from empty reconstructs the whole project,
//! the interchange serializer embeds tunings with no special case (R-506), and
//! "builtin" is a provenance label rather than a privileged status (R-505).

use rev_core::Command;
use rev_core::id::TuningId;
use rev_core::note::NoteNumber;
use rev_core::scale::ScaleSpec;
use rev_core::tuning::{Ratio, TuningKind, TuningNote, TuningNoteValue, TuningSpec};

use crate::error::StoreError;
use crate::exec::now_ms;
use crate::project::Project;
use crate::schema;

/// Middle C: 440 Hz divided by 2^(9/12). Every builtin anchors note 60 here, so
/// switching a phrase's tuning keeps its home pitch (the property Notorolla
/// established, now a stated convention).
pub const MIDDLE_C_HZ: f64 = 261.625_565_300_598_6;

/// The anchor note for the builtins. Note numbers are signed, so a dense tuning
/// running below zero is ordinary; 60 is convention, not structure.
pub const ANCHOR_NOTE: i32 = 60;

const DOMAIN_MIN: i32 = 0;
const DOMAIN_MAX: i32 = 127;

/// 5-limit just intonation, one canonical period from the anchor. Exact
/// integer pairs (R-504): a just fifth is 3/2, not 701.955 cents.
const JUST_5LIMIT: [(i64, i64); 12] = [
    (1, 1),
    (16, 15),
    (9, 8),
    (6, 5),
    (5, 4),
    (4, 3),
    (45, 32),
    (3, 2),
    (8, 5),
    (5, 3),
    (9, 5),
    (15, 8),
];

/// Scale masks, ported from the Notorolla lab's library. Root-relative shapes
/// keyed by notes-per-period, so one mask serves every tuning of that
/// periodicity (R-509) — the 12-note masks cover 12-ET and 5-limit just
/// intonation alike. Chromatic is deliberately absent: a NULL scale binding is
/// chromatic (R-510).
const SCALE_12: &[(&str, &[i32])] = &[
    ("Major (Ionian)", &[0, 2, 4, 5, 7, 9, 11]),
    ("Dorian", &[0, 2, 3, 5, 7, 9, 10]),
    ("Phrygian", &[0, 1, 3, 5, 7, 8, 10]),
    ("Lydian", &[0, 2, 4, 6, 7, 9, 11]),
    ("Mixolydian", &[0, 2, 4, 5, 7, 9, 10]),
    ("Minor (Aeolian)", &[0, 2, 3, 5, 7, 8, 10]),
    ("Locrian", &[0, 1, 3, 5, 6, 8, 10]),
    ("Harmonic minor", &[0, 2, 3, 5, 7, 8, 11]),
    ("Melodic minor", &[0, 2, 3, 5, 7, 9, 11]),
    ("Whole-tone", &[0, 2, 4, 6, 8, 10]),
    ("Octatonic (W-H)", &[0, 2, 3, 5, 6, 8, 9, 11]),
    ("Octatonic (H-W)", &[0, 1, 3, 4, 6, 7, 9, 10]),
    ("Augmented", &[0, 3, 4, 7, 8, 11]),
    ("Blues (minor)", &[0, 3, 5, 6, 7, 10]),
    ("Major pentatonic", &[0, 2, 4, 7, 9]),
    ("Minor pentatonic", &[0, 3, 5, 7, 10]),
];

/// 16-ET's native families: the Mavila chain off the flat ~675-cent fifth (the
/// anti-diatonic), and the symmetric engines 16 = 2·2·2·2 is rich in.
const SCALE_16: &[(&str, &[i32])] = &[
    ("Mavila (7)", &[0, 2, 4, 6, 9, 11, 13]),
    ("Mavila (9)", &[0, 2, 4, 6, 8, 9, 11, 13, 15]),
    ("Mavila pentatonic", &[0, 2, 4, 9, 11]),
    ("Octatonic (16)", &[0, 1, 4, 5, 8, 9, 12, 13]),
    ("Whole-tone (8)", &[0, 2, 4, 6, 8, 10, 12, 14]),
    ("Lemba (6)", &[0, 3, 6, 8, 11, 14]),
];

fn equal_tuning(name: &str, description: &str, note_per_period: i32, naming: &str) -> TuningSpec {
    let mut spec = TuningSpec::new(name, TuningKind::Equal, ANCHOR_NOTE, MIDDLE_C_HZ);
    spec.description = Some(description.to_string());
    spec.period = Some(Ratio::OCTAVE);
    spec.note_per_period = Some(note_per_period);
    spec.note_min = Some(NoteNumber(DOMAIN_MIN));
    spec.note_max = Some(NoteNumber(DOMAIN_MAX));
    spec.naming = Some(naming.to_string());
    spec.origin = Some("builtin".to_string());
    spec
}

pub(crate) fn seed(project: &mut Project) -> Result<(), StoreError> {
    project.gesture(|g| {
        g.exec(Command::SetMeta {
            key: schema::META_SCHEMA_VERSION.to_string(),
            value: Some(schema::SCHEMA_VERSION.to_string()),
        })?;
        g.exec(Command::SetMeta {
            key: schema::META_PPQ.to_string(),
            value: Some(rev_core::PPQ.to_string()),
        })?;
        g.exec(Command::SetMeta {
            key: schema::META_CREATED.to_string(),
            value: Some(now_ms().to_string()),
        })?;

        // 12-ET first, so it takes the lowest id and reads as the default.
        let twelve = tuning_id(g.exec(Command::CreateTuning {
            id: None,
            tuning: equal_tuning(
                "12-ET",
                "Twelve equal divisions of the octave. One tuning among many, with no \
                 privileged status in the model.",
                12,
                "letter",
            ),
        })?);
        g.exec(Command::MaterializeTuning {
            id: None,
            tuning_id: twelve,
            ts: None,
        })?;

        let sixteen = tuning_id(g.exec(Command::CreateTuning {
            id: None,
            tuning: equal_tuning(
                "16-ET",
                "Sixteen equal divisions of the octave: steps of 75 cents, no good fifth, an \
                 exact tritone and a strong 7/4. Home of the Mavila anti-diatonic.",
                16,
                "hex",
            ),
        })?);
        g.exec(Command::MaterializeTuning {
            id: None,
            tuning_id: sixteen,
            ts: None,
        })?;

        let mut just = TuningSpec::new(
            "Just (5-limit)",
            TuningKind::Table,
            ANCHOR_NOTE,
            MIDDLE_C_HZ,
        );
        just.description = Some(
            "A twelve-note 5-limit just chromatic, stored as exact ratios from the anchor."
                .to_string(),
        );
        just.period = Some(Ratio::OCTAVE);
        just.note_per_period = Some(12);
        just.note_min = Some(NoteNumber(DOMAIN_MIN));
        just.note_max = Some(NoteNumber(DOMAIN_MAX));
        just.naming = Some("letter".to_string());
        just.origin = Some("builtin".to_string());
        let just_id = tuning_id(g.exec(Command::CreateTuning {
            id: None,
            tuning: just,
        })?);
        g.exec(Command::SetTuningNote {
            tuning_id: just_id,
            note: JUST_5LIMIT
                .iter()
                .enumerate()
                .map(|(i, &(num, den))| TuningNote {
                    note_number: NoteNumber(ANCHOR_NOTE + i as i32),
                    value: TuningNoteValue::Ratio(Ratio::new(num, den)),
                })
                .collect(),
        })?;
        g.exec(Command::MaterializeTuning {
            id: None,
            tuning_id: just_id,
            ts: None,
        })?;

        for (note_per_period, library) in [(12, SCALE_12), (16, SCALE_16)] {
            for &(name, mask) in library {
                let mut spec = ScaleSpec::periodic(name, note_per_period, mask.to_vec());
                spec.origin = Some("builtin".to_string());
                g.exec(Command::CreateScale {
                    id: None,
                    scale: spec,
                })?;
            }
        }

        g.exec(Command::SetMeta {
            key: schema::META_DEFAULT_TUNING_ID.to_string(),
            value: Some(twelve.get().to_string()),
        })?;
        Ok(())
    })
}

/// Pull the assigned id back out of a resolved CreateTuning.
fn tuning_id(resolved: Command) -> TuningId {
    match resolved {
        Command::CreateTuning { id: Some(id), .. } => id,
        // Unreachable: execute always resolves the id it assigned.
        other => unreachable!("expected a resolved create_tuning, got {}", other.name()),
    }
}
