//! The realization view: window semantics, looping, transposition — and the
//! 16-ET party trick.
//!
//! core-03 owns the exhaustive hand-computed fixtures; these establish that the
//! view exists and behaves as core-01 ruled.

use rev_core::phrase::{
    Change, Container, EventSpec, InstanceContainer, PhraseInstancePatch, PhraseInstanceSpec,
    PhrasePatch, PhraseSpec,
};
use rev_core::tick::{PPQ, Tick};
use rev_core::{Command, NoteNumber, PhraseId, PhraseInstanceId, TrackId};
use rev_store::query;
use rev_testkit::{TempProject, fixture};

/// A phrase of four quarter notes (60, 62, 64, 66) whose window is two beats —
/// so the last two notes are dormant material until the window widens.
struct Windowed {
    track: TrackId,
    melody: PhraseId,
    instance: PhraseInstanceId,
}

fn windowed(temp: &mut TempProject) -> Windowed {
    temp.project_mut()
        .gesture(|g| {
            let melody = match g.exec(Command::CreatePhrase {
                id: None,
                phrase: PhraseSpec::new("Four", Tick(PPQ * 2)),
            })? {
                Command::CreatePhrase { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            g.exec(Command::AddEvent {
                container: Container::Phrase(melody),
                event: (0..4)
                    .map(|i| EventSpec::note(Tick(PPQ * i), Tick(PPQ), 60 + 2 * i as i32, 40_000))
                    .collect(),
            })?;

            let arrangement = match g.exec(Command::CreatePhrase {
                id: None,
                phrase: PhraseSpec::new("Arr", Tick(PPQ * 16)),
            })? {
                Command::CreatePhrase { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            let track = match g.exec(Command::CreateTrack {
                id: None,
                track: rev_core::phrase::TrackSpec::new(arrangement, "T", 0),
            })? {
                Command::CreateTrack { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            let instance = match g.exec(Command::CreatePhraseInstance {
                id: None,
                phrase_instance: PhraseInstanceSpec::new(
                    melody,
                    InstanceContainer::Track(track),
                    Tick::ZERO,
                ),
            })? {
                Command::CreatePhraseInstance { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            Ok(Windowed {
                track,
                melody,
                instance,
            })
        })
        .unwrap()
}

fn realized_note(temp: &TempProject, track: TrackId) -> Vec<(i64, i32)> {
    query::realized(temp.project().reader(), track)
        .unwrap()
        .into_iter()
        .map(|e| (e.at_tick.get(), e.note_number.unwrap().get()))
        .collect()
}

#[test]
fn the_window_gates_onsets_and_keeps_dormant_material() {
    let mut temp = TempProject::create().unwrap();
    let built = windowed(&mut temp);

    // Four events exist; the two-beat window admits the first two.
    assert_eq!(
        query::event_in_phrase(temp.project().reader(), built.melody)
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        realized_note(&temp, built.track),
        vec![(0, 60), (PPQ, 62)],
        "events beyond the window are retained but silent"
    );

    // Widening the window reveals what was always there — non-destructively.
    temp.project_mut()
        .apply(Command::SetPhrase {
            id: built.melody,
            patch: PhrasePatch {
                length_tick: Some(Tick(PPQ * 4)),
                ..Default::default()
            },
        })
        .unwrap();
    assert_eq!(
        realized_note(&temp, built.track),
        vec![(0, 60), (PPQ, 62), (PPQ * 2, 64), (PPQ * 3, 66)]
    );

    // And narrowing it again mutes the tail rather than deleting it.
    temp.project_mut().undo().unwrap();
    assert_eq!(realized_note(&temp, built.track), vec![(0, 60), (PPQ, 62)]);
}

#[test]
fn the_window_start_selects_material() {
    // Vision's "play bars 8 to 16 of 32", in miniature: offset_tick moves the
    // window into the material.
    let mut temp = TempProject::create().unwrap();
    let built = windowed(&mut temp);

    temp.project_mut()
        .apply(Command::SetPhraseInstanceParam {
            id: built.instance,
            patch: PhraseInstancePatch {
                offset_tick: Some(Tick(PPQ * 2)),
                ..Default::default()
            },
        })
        .unwrap();

    // The third and fourth notes now play, and they play from the start.
    assert_eq!(realized_note(&temp, built.track), vec![(0, 64), (PPQ, 66)]);
}

#[test]
fn looping_repeats_by_the_window_length() {
    let mut temp = TempProject::create().unwrap();
    let built = windowed(&mut temp);

    temp.project_mut()
        .apply(Command::SetPhraseInstanceParam {
            id: built.instance,
            patch: PhraseInstancePatch {
                loop_count: Some(3),
                ..Default::default()
            },
        })
        .unwrap();

    assert_eq!(
        realized_note(&temp, built.track),
        vec![
            (0, 60),
            (PPQ, 62),
            (PPQ * 2, 60),
            (PPQ * 3, 62),
            (PPQ * 4, 60),
            (PPQ * 5, 62),
        ],
        "the loop stride is the window length, not the material extent"
    );
}

#[test]
fn instance_length_clips_the_loop() {
    let mut temp = TempProject::create().unwrap();
    let built = windowed(&mut temp);

    temp.project_mut()
        .apply(Command::SetPhraseInstanceParam {
            id: built.instance,
            patch: PhraseInstancePatch {
                loop_count: Some(3),
                length_tick: Change::Set(Tick(PPQ * 3)),
                ..Default::default()
            },
        })
        .unwrap();

    // Three beats of a two-beat loop: two full notes plus the next downbeat.
    assert_eq!(
        realized_note(&temp, built.track),
        vec![(0, 60), (PPQ, 62), (PPQ * 2, 60)]
    );
}

#[test]
fn transpose_is_chromatic_and_muting_removes_the_instance() {
    let mut temp = TempProject::create().unwrap();
    let built = windowed(&mut temp);

    temp.project_mut()
        .apply(Command::SetPhraseInstanceParam {
            id: built.instance,
            patch: PhraseInstancePatch {
                transpose: Some(7),
                ..Default::default()
            },
        })
        .unwrap();
    assert_eq!(realized_note(&temp, built.track), vec![(0, 67), (PPQ, 69)]);

    temp.project_mut()
        .apply(Command::SetPhraseInstanceParam {
            id: built.instance,
            patch: PhraseInstancePatch {
                mute: Some(true),
                ..Default::default()
            },
        })
        .unwrap();
    assert!(realized_note(&temp, built.track).is_empty());
}

#[test]
fn editing_a_phrase_changes_every_instance_of_it() {
    // R-404: instances share material, they do not copy it.
    let mut temp = TempProject::create().unwrap();
    let built = windowed(&mut temp);
    temp.project_mut()
        .apply(Command::CreatePhraseInstance {
            id: None,
            phrase_instance: PhraseInstanceSpec::new(
                built.melody,
                InstanceContainer::Track(built.track),
                Tick(PPQ * 8),
            ),
        })
        .unwrap();

    temp.project_mut()
        .apply(Command::AddEvent {
            container: Container::Phrase(built.melody),
            event: vec![EventSpec::note(Tick(PPQ / 2), Tick(PPQ / 2), 61, 40_000)],
        })
        .unwrap();

    let realized = realized_note(&temp, built.track);
    assert!(realized.contains(&(PPQ / 2, 61)), "first instance");
    assert!(
        realized.contains(&(PPQ * 8 + PPQ / 2, 61)),
        "second instance sees the same edit"
    );
}

#[test]
fn mhall_realizes_in_order() {
    let mut temp = TempProject::create().unwrap();
    let built = fixture::mhall(temp.project_mut()).unwrap();

    let realized = query::realized(temp.project().reader(), built.track).unwrap();
    let note: Vec<NoteNumber> = realized.iter().map(|e| e.note_number.unwrap()).collect();
    assert_eq!(note, fixture::mhall_note_number());

    // The tune spans eight 4/4 bars and ends with a whole note.
    assert_eq!(realized.first().unwrap().at_tick, Tick::ZERO);
    assert_eq!(realized.last().unwrap().at_tick, Tick(PPQ * 28));
    assert_eq!(realized.last().unwrap().dur_tick, Tick(PPQ * 4));
    assert_eq!(realized[0].tuning_id, Some(built.tuning_12et));
    assert!(realized[0].phrase_instance_id.is_some());
}

#[test]
fn the_sixteen_equal_party_trick_is_one_command() {
    // Degree-native pitch, demonstrated: swapping the tuning changes not one
    // event row. The melody is made of positions in a tuning, so its shape
    // survives while every interval is reinterpreted.
    let mut temp = TempProject::create().unwrap();
    let built = fixture::mhall(temp.project_mut()).unwrap();

    let before = query::event_in_phrase(temp.project().reader(), built.melody).unwrap();
    let twelve = resolve(&temp, built.tuning_12et, NoteNumber(64));

    temp.project_mut()
        .apply(Command::SetPhrase {
            id: built.melody,
            patch: PhrasePatch {
                tuning_id: Change::Set(built.tuning_16et),
                ..Default::default()
            },
        })
        .unwrap();

    let after = query::event_in_phrase(temp.project().reader(), built.melody).unwrap();
    assert_eq!(before, after, "the material is untouched");

    let realized = query::realized(temp.project().reader(), built.track).unwrap();
    assert_eq!(realized[0].tuning_id, Some(built.tuning_16et));
    assert_eq!(
        realized
            .iter()
            .map(|e| e.note_number.unwrap())
            .collect::<Vec<_>>(),
        fixture::mhall_note_number(),
        "note numbers are unchanged"
    );

    // Same note number, different pitch: four steps above the anchor is a major
    // third in 12-ET and 300 cents in 16-ET.
    let sixteen = resolve(&temp, built.tuning_16et, NoteNumber(64));
    assert!(sixteen < twelve, "16-ET's fourth step is flatter");
    let cents = 1200.0 * (sixteen / twelve).ln() / std::f64::consts::LN_2;
    assert!((cents - -100.0).abs() < 1e-6, "difference is {cents} cents");

    // And it is one command, so it undoes in one gesture.
    temp.project_mut().undo().unwrap();
    let restored = query::realized(temp.project().reader(), built.track).unwrap();
    assert_eq!(restored[0].tuning_id, Some(built.tuning_12et));
}

/// The frequency a note number resolves to in a tuning — what the schedule
/// compiler will do in stage 2.
fn resolve(temp: &TempProject, tuning: rev_core::TuningId, note: NoteNumber) -> f64 {
    let reader = temp.project().reader();
    let instance = query::latest_materialized_instance(reader, tuning)
        .unwrap()
        .unwrap();
    query::materialized_tuning(reader, instance)
        .unwrap()
        .unwrap()
        .freq(note)
        .unwrap()
}

/// **A nested instance does not realize.** Recorded as a failing test rather than
/// as prose, because R-407 permits a phrase to contain instances of other phrases
/// and R-422 requires a structured instance to realize as though it had been baked
/// — while `v_realized` unions direct events with the events of an *instanced*
/// phrase, one level deep. There is no recursion, so events reached only through a
/// nested instance are absent from the arrangement.
///
/// Ignored, not deleted: the fix changes a checkpointed view definition (a
/// recursive CTE), which is not a thing to slip in under a test's cover. Run it
/// with `cargo test -- --ignored` to see the gap.
#[test]
#[ignore = "R-407/R-422: v_realized is one level deep; nesting does not realize (core-03 finding)"]
fn a_nested_instance_realizes_as_though_baked() {
    let mut temp = TempProject::create().unwrap();

    // inner holds the notes; outer holds an instance of inner; the arrangement
    // holds an instance of outer. The notes are therefore two levels down.
    let track = temp
        .project_mut()
        .gesture(|g| {
            let mut phrase = |name: &str, len: i64| match g.exec(Command::CreatePhrase {
                id: None,
                phrase: PhraseSpec::new(name, Tick(len)),
            }) {
                Ok(Command::CreatePhrase { id: Some(id), .. }) => id,
                _ => unreachable!(),
            };
            let inner = phrase("Inner", PPQ * 2);
            let outer = phrase("Outer", PPQ * 2);
            let arrangement = phrase("Arr", PPQ * 4);

            g.exec(Command::AddEvent {
                container: Container::Phrase(inner),
                event: vec![
                    EventSpec::note(Tick(0), Tick(PPQ), 60, 40_000),
                    EventSpec::note(Tick(PPQ), Tick(PPQ), 64, 40_000),
                ],
            })?;

            let mut track_in = |phrase, name: &str| match g.exec(Command::CreateTrack {
                id: None,
                track: rev_core::phrase::TrackSpec::new(phrase, name, 0),
            }) {
                Ok(Command::CreateTrack { id: Some(id), .. }) => id,
                _ => unreachable!(),
            };
            let outer_track = track_in(outer, "Nested");
            let top_track = track_in(arrangement, "Top");

            g.exec(Command::CreatePhraseInstance {
                id: None,
                phrase_instance: PhraseInstanceSpec::new(
                    inner,
                    InstanceContainer::Track(outer_track),
                    Tick::ZERO,
                ),
            })?;
            g.exec(Command::CreatePhraseInstance {
                id: None,
                phrase_instance: PhraseInstanceSpec::new(
                    outer,
                    InstanceContainer::Track(top_track),
                    Tick::ZERO,
                ),
            })?;
            Ok(top_track)
        })
        .unwrap();

    assert_eq!(
        realized_note(&temp, track),
        vec![(0, 60), (PPQ, 64)],
        "the inner phrase's notes should sound through the outer instance (R-422)"
    );
}
