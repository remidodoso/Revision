//! MHALL as a project, for the demo binaries.
//!
//! Shared by `rev-mhall` (which plays it) and `rev-roll` (which draws it), so
//! there is one tune rather than two that drift. It is a *demo* fixture, not a
//! test one — `rev-testkit` is dev-only and cannot be linked into a shipping
//! binary, which is the constraint that put this here.

use rev_core::phrase::{
    Container, EventSpec, InstanceContainer, PhraseInstanceSpec, PhraseSpec, TempoPoint, TrackSpec,
};
use rev_core::tick::{PPQ, Tick, bpm_to_usec_per_quarter};
use rev_core::{Command as ModelCommand, PhraseId, TrackId};
use rev_store::{Project, StoreError, query};

/// Three quarters of full scale: the same value the test fixture uses, written
/// here because `rev-testkit` is dev-only and a shipping binary cannot see it.
pub const MEZZO_FORTE: i32 = 49_152;

/// The tune: (note number, length in quarters), 12-ET.
#[rustfmt::skip]
pub const MHALL: &[(i32, i64)] = &[
    (64, 1), (62, 1), (60, 1), (62, 1),
    (64, 1), (64, 1), (64, 2),
    (62, 1), (62, 1), (62, 2),
    (64, 1), (67, 1), (67, 2),
    (64, 1), (62, 1), (60, 1), (62, 1),
    (64, 1), (64, 1), (64, 1), (64, 1),
    (62, 1), (62, 1), (64, 1), (62, 1),
    (60, 4),
];

/// Build the tune into a project. One phrase, one arrangement, one instance.
pub fn build(
    project: &mut Project,
    bpm: f64,
    tuning: &str,
) -> Result<(PhraseId, TrackId), StoreError> {
    let tuning_id = query::tuning_by_name(project.reader(), tuning)?.map(|t| t.id);
    if tuning_id.is_none() {
        eprintln!("no tuning named {tuning:?}; using the project default");
    }

    project.gesture(|g| {
        let bar = PPQ * 4;
        let mut melody = PhraseSpec::new("Mary Had a Little Lamb", Tick(bar * 8));
        melody.tuning_id = tuning_id;
        let melody = match g.exec(ModelCommand::CreatePhrase {
            id: None,
            phrase: melody,
        })? {
            ModelCommand::CreatePhrase { id: Some(id), .. } => id,
            _ => unreachable!(),
        };

        let mut at = Tick::ZERO;
        let mut event = Vec::with_capacity(MHALL.len());
        for &(note, quarters) in MHALL {
            let duration = Tick(PPQ * quarters);
            event.push(EventSpec::note(at, duration, note, MEZZO_FORTE));
            at = Tick(at.get() + duration.get());
        }
        g.exec(ModelCommand::AddEvent {
            container: Container::Phrase(melody),
            event,
        })?;

        let arrangement = match g.exec(ModelCommand::CreatePhrase {
            id: None,
            phrase: PhraseSpec::new("Arrangement", Tick(bar * 8)),
        })? {
            ModelCommand::CreatePhrase { id: Some(id), .. } => id,
            _ => unreachable!(),
        };
        g.exec(ModelCommand::SetTempo {
            phrase_id: arrangement,
            point: vec![TempoPoint {
                at_tick: Tick::ZERO,
                usec_per_quarter: bpm_to_usec_per_quarter(bpm),
            }],
        })?;

        let track = match g.exec(ModelCommand::CreateTrack {
            id: None,
            track: TrackSpec::new(arrangement, "Melody", 0),
        })? {
            ModelCommand::CreateTrack { id: Some(id), .. } => id,
            _ => unreachable!(),
        };
        g.exec(ModelCommand::CreatePhraseInstance {
            id: None,
            phrase_instance: PhraseInstanceSpec::new(
                melody,
                InstanceContainer::Track(track),
                Tick::ZERO,
            ),
        })?;

        Ok((arrangement, track))
    })
}
