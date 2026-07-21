//! The query catalog, and the read-only connection it runs on.
//!
//! Every catalog function is exercised here — SQL is checked at runtime, so a
//! column rename that misses a query has to fail a test rather than ship.

use rev_core::phrase::{
    Container, EventSpec, InstanceContainer, PhraseInstanceSpec, PhraseSpec, TempoPoint, TrackSpec,
};
use rev_core::tick::{PPQ, Tick, bpm_to_usec_per_quarter};
use rev_core::{Command, PhraseId};
use rev_store::{StoreError, query};
use rev_testkit::{TempProject, fixture};

#[test]
fn material_lookups_round_trip() {
    let mut temp = TempProject::create().unwrap();
    let built = fixture::mhall(temp.project_mut()).unwrap();
    let reader = temp.project().reader();

    let phrase = query::phrase(reader, built.melody).unwrap().unwrap();
    assert_eq!(phrase.spec.name, "Mary Had a Little Lamb");
    assert_eq!(phrase.spec.length_tick, Tick(PPQ * 32));
    assert_eq!(phrase.spec.tuning_id, Some(built.tuning_12et));
    assert_eq!(
        query::phrase_by_name(reader, "Mary Had a Little Lamb")
            .unwrap()
            .unwrap()
            .id,
        built.melody
    );
    assert!(query::phrase(reader, PhraseId(999_999)).unwrap().is_none());

    let track = query::track(reader, built.track).unwrap().unwrap();
    assert_eq!(track.spec.name, "Melody");
    let track_list = query::track_in_phrase(reader, built.arrangement).unwrap();
    assert_eq!(track_list.len(), 1);

    let event = query::event_in_phrase(reader, built.melody).unwrap();
    assert_eq!(event.len(), fixture::mhall_note_number().len());
    assert_eq!(event[0].at_tick, Tick::ZERO);
    assert_eq!(event[0].note_number.unwrap().get(), 64);
    assert_eq!(event[0].velocity, Some(fixture::MEZZO_FORTE));
    assert_eq!(
        query::event(reader, event[0].id).unwrap().unwrap().id,
        event[0].id
    );
    // The melody lives in a phrase, so the track carries no direct events.
    assert!(
        query::event_on_track(reader, built.track)
            .unwrap()
            .is_empty()
    );

    let instance = query::phrase_instance_of(reader, built.melody).unwrap();
    assert_eq!(instance.len(), 1, "where-used (R-411)");
    assert_eq!(
        query::phrase_instance(reader, instance[0].id)
            .unwrap()
            .unwrap()
            .id,
        instance[0].id
    );

    let tempo = query::tempo_point(reader, built.arrangement).unwrap();
    assert_eq!(tempo.len(), 1);
    assert_eq!(tempo[0].usec_per_quarter, bpm_to_usec_per_quarter(120.0));
}

#[test]
fn direct_events_on_a_track_are_found() {
    let mut temp = TempProject::create().unwrap();
    let built = fixture::mhall(temp.project_mut()).unwrap();
    temp.project_mut()
        .apply(Command::RecordBatch {
            track_id: built.track,
            event: vec![EventSpec::note(Tick::ZERO, Tick(PPQ), 48, 30_000)],
        })
        .unwrap();

    let direct = query::event_on_track(temp.project().reader(), built.track).unwrap();
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].container, Container::Track(built.track));
}

#[test]
fn nesting_reachability_answers_the_cycle_question() {
    let mut temp = TempProject::create().unwrap();
    let outer = new_phrase(&mut temp, "Outer");
    let inner = new_phrase(&mut temp, "Inner");

    // Nothing nested yet: a phrase reaches only itself.
    assert!(query::phrase_reaches(temp.project().reader(), outer, outer).unwrap());
    assert!(!query::phrase_reaches(temp.project().reader(), outer, inner).unwrap());

    temp.project_mut()
        .apply(Command::CreatePhraseInstance {
            id: None,
            phrase_instance: PhraseInstanceSpec::new(
                inner,
                InstanceContainer::ParentPhrase(outer),
                Tick::ZERO,
            ),
        })
        .unwrap();
    assert!(query::phrase_reaches(temp.project().reader(), outer, inner).unwrap());
}

#[test]
fn a_cycle_is_refused_at_the_model_level() {
    // R-407: realization of a cyclic reference would not terminate, so the
    // model refuses to represent one.
    let mut temp = TempProject::create().unwrap();
    let outer = new_phrase(&mut temp, "Outer");
    let inner = new_phrase(&mut temp, "Inner");

    temp.project_mut()
        .apply(Command::CreatePhraseInstance {
            id: None,
            phrase_instance: PhraseInstanceSpec::new(
                inner,
                InstanceContainer::ParentPhrase(outer),
                Tick::ZERO,
            ),
        })
        .unwrap();

    // Putting the outer phrase inside the inner one closes the loop.
    let outcome = temp.project_mut().apply(Command::CreatePhraseInstance {
        id: None,
        phrase_instance: PhraseInstanceSpec::new(
            outer,
            InstanceContainer::ParentPhrase(inner),
            Tick::ZERO,
        ),
    });
    assert!(matches!(outcome, Err(StoreError::PhraseCycle { .. })));

    // And so does putting a phrase inside itself.
    let direct = temp.project_mut().apply(Command::CreatePhraseInstance {
        id: None,
        phrase_instance: PhraseInstanceSpec::new(
            outer,
            InstanceContainer::ParentPhrase(outer),
            Tick::ZERO,
        ),
    });
    assert!(matches!(direct, Err(StoreError::PhraseCycle { .. })));
}

#[test]
fn the_reader_cannot_write() {
    // "Commands are the only writer" is enforced by SQLite, not by convention.
    let temp = TempProject::create().unwrap();
    let reader = temp.project().reader();
    assert!(reader.execute("DELETE FROM phrase", []).is_err());
    assert!(
        reader
            .execute("INSERT INTO meta (key, value) VALUES ('x', 'y')", [])
            .is_err()
    );
}

#[test]
fn the_reader_still_supports_temp_tables() {
    // Load-bearing for the sanctioned performance escape hatch (§6f-bis): when
    // a view groans, a temp-table waypoint must be possible on the same
    // read-only connection the catalog runs on. SQLite keeps the temp schema
    // writable even when the main database is opened read-only.
    let mut temp = TempProject::create().unwrap();
    fixture::mhall(temp.project_mut()).unwrap();
    let reader = temp.project().reader();

    reader
        .execute_batch(
            "CREATE TEMP TABLE waypoint AS \
             SELECT at_tick, note_number FROM v_realized ORDER BY at_tick",
        )
        .expect("temp tables must work on a read-only connection");
    let count: i64 = reader
        .query_row("SELECT COUNT(*) FROM waypoint", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, fixture::mhall_note_number().len() as i64);
}

#[test]
fn a_gesture_can_read_its_own_uncommitted_rows() {
    // The reader deliberately cannot see mid-gesture state; the gesture's own
    // connection can, which is how one command builds on another.
    let mut temp = TempProject::create().unwrap();
    temp.project_mut()
        .gesture(|g| {
            let resolved = g.exec(Command::CreatePhrase {
                id: None,
                phrase: PhraseSpec::new("InFlight", Tick(PPQ)),
            })?;
            let id = match resolved {
                Command::CreatePhrase { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            let seen = query::phrase(g.connection(), id)?;
            assert!(seen.is_some(), "a gesture should see its own writes");
            g.exec(Command::CreateTrack {
                id: None,
                track: TrackSpec::new(id, "T", 0),
            })?;
            Ok(())
        })
        .unwrap();

    assert!(
        query::phrase_by_name(temp.project().reader(), "InFlight")
            .unwrap()
            .is_some()
    );
}

fn new_phrase(temp: &mut TempProject, name: &str) -> PhraseId {
    let resolved = temp
        .project_mut()
        .apply(Command::CreatePhrase {
            id: None,
            phrase: PhraseSpec::new(name, Tick(PPQ * 4)),
        })
        .unwrap();
    match resolved {
        Command::CreatePhrase { id: Some(id), .. } => id,
        _ => panic!("unresolved"),
    }
}

#[test]
fn tempo_map_replacement_is_total() {
    let mut temp = TempProject::create().unwrap();
    let phrase = new_phrase(&mut temp, "Tempo");
    temp.project_mut()
        .apply(Command::SetTempo {
            phrase_id: phrase,
            point: vec![
                TempoPoint {
                    at_tick: Tick::ZERO,
                    usec_per_quarter: 500_000,
                },
                TempoPoint {
                    at_tick: Tick(PPQ * 4),
                    usec_per_quarter: 400_000,
                },
            ],
        })
        .unwrap();
    assert_eq!(
        query::tempo_point(temp.project().reader(), phrase)
            .unwrap()
            .len(),
        2
    );

    temp.project_mut()
        .apply(Command::SetTempo {
            phrase_id: phrase,
            point: vec![TempoPoint {
                at_tick: Tick::ZERO,
                usec_per_quarter: 600_000,
            }],
        })
        .unwrap();
    let point = query::tempo_point(temp.project().reader(), phrase).unwrap();
    assert_eq!(point.len(), 1);
    assert_eq!(point[0].usec_per_quarter, 600_000);

    // And the inverse is the prior map, whole.
    temp.project_mut().undo().unwrap();
    assert_eq!(
        query::tempo_point(temp.project().reader(), phrase)
            .unwrap()
            .len(),
        2
    );
}
