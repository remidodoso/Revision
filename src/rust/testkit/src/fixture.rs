//! Material fixtures: known projects for tests to work against.

use rev_core::id::{PhraseId, TrackId, TuningId};
use rev_core::phrase::{
    Container, EventSpec, InstanceContainer, PhraseInstanceSpec, PhraseSpec, TempoPoint, TrackSpec,
};
use rev_core::tick::{PPQ, Tick, bpm_to_usec_per_quarter};
use rev_core::{Command, NoteNumber};
use rev_store::{Project, StoreError, query};

/// A mezzo-forte velocity in the MIDI 2.0 16-bit domain (R-402); a 7-bit
/// controller sending 96 arrives near here after boundary translation.
pub const MEZZO_FORTE: i32 = 49_152;

/// "Mary Had a Little Lamb": (note number, length in quarters), 12-ET.
///
/// The PoC's tune, and the 16-ET party trick's subject — switching the phrase's
/// tuning changes not one row here, because the melody is made of positions in
/// a tuning, never of frequencies.
// One line per bar: the music is the point, and rustfmt would flatten it.
#[rustfmt::skip]
const MHALL: &[(i32, i64)] = &[
    (64, 1), (62, 1), (60, 1), (62, 1),
    (64, 1), (64, 1), (64, 2),
    (62, 1), (62, 1), (62, 2),
    (64, 1), (67, 1), (67, 2),
    (64, 1), (62, 1), (60, 1), (62, 1),
    (64, 1), (64, 1), (64, 1), (64, 1),
    (62, 1), (62, 1), (64, 1), (62, 1),
    (60, 4),
];

/// What [`mhall`] created.
pub struct Mhall {
    /// The arrangement root, which owns the track.
    pub arrangement: PhraseId,
    pub track: TrackId,
    /// The melody itself — the reusable material.
    pub melody: PhraseId,
    pub tuning_12et: TuningId,
    pub tuning_16et: TuningId,
}

/// Build the tune into a project: a melody phrase, an arrangement with one
/// track, and one instance placing the melody at the start.
pub fn mhall(project: &mut Project) -> Result<Mhall, StoreError> {
    let tuning_12et = tuning_named(project, "12-ET")?;
    let tuning_16et = tuning_named(project, "16-ET")?;

    project.gesture(|g| {
        let bar = Tick(PPQ * 4);
        let mut melody_spec = PhraseSpec::new("Mary Had a Little Lamb", Tick(bar.get() * 8));
        melody_spec.tuning_id = Some(tuning_12et);
        let melody = phrase_id(g.exec(Command::CreatePhrase {
            id: None,
            phrase: melody_spec,
        })?);

        let mut at = Tick::ZERO;
        let mut event = Vec::with_capacity(MHALL.len());
        for &(note_number, quarter) in MHALL {
            let duration = Tick(PPQ * quarter);
            event.push(EventSpec::note(at, duration, note_number, MEZZO_FORTE));
            at = Tick(at.get() + duration.get());
        }
        g.exec(Command::AddEvent {
            container: Container::Phrase(melody),
            event,
        })?;

        let arrangement = phrase_id(g.exec(Command::CreatePhrase {
            id: None,
            phrase: PhraseSpec::new("Arrangement", Tick(bar.get() * 8)),
        })?);
        g.exec(Command::SetTempo {
            phrase_id: arrangement,
            point: vec![TempoPoint {
                at_tick: Tick::ZERO,
                usec_per_quarter: bpm_to_usec_per_quarter(120.0),
            }],
        })?;

        let track = track_id(g.exec(Command::CreateTrack {
            id: None,
            track: TrackSpec::new(arrangement, "Melody", 0),
        })?);
        g.exec(Command::CreatePhraseInstance {
            id: None,
            phrase_instance: PhraseInstanceSpec::new(
                melody,
                InstanceContainer::Track(track),
                Tick::ZERO,
            ),
        })?;

        Ok(Mhall {
            arrangement,
            track,
            melody,
            tuning_12et,
            tuning_16et,
        })
    })
}

/// The note numbers of the tune, in order — what a realization of the melody
/// must produce.
pub fn mhall_note_number() -> Vec<NoteNumber> {
    MHALL.iter().map(|&(n, _)| NoteNumber(n)).collect()
}

fn tuning_named(project: &Project, name: &str) -> Result<TuningId, StoreError> {
    Ok(query::tuning_by_name(project.reader(), name)?
        .expect("builtin tuning present after genesis")
        .id)
}

fn phrase_id(resolved: Command) -> PhraseId {
    match resolved {
        Command::CreatePhrase { id: Some(id), .. } => id,
        other => unreachable!("expected a resolved create_phrase, got {}", other.name()),
    }
}

fn track_id(resolved: Command) -> TrackId {
    match resolved {
        Command::CreateTrack { id: Some(id), .. } => id,
        other => unreachable!("expected a resolved create_track, got {}", other.name()),
    }
}
