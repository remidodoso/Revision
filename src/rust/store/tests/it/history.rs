//! The journal: durability, undo/redo, and replay.

use rev_core::phrase::{Container, EventSpec, PhrasePatch, PhraseSpec};
use rev_core::tick::{PPQ, Tick};
use rev_core::{Command, PhraseId};
use rev_store::{Project, journal, query, replay};
use rev_testkit::{TempProject, fixture, state};

fn create_phrase(project: &mut Project, name: &str) -> PhraseId {
    let resolved = project
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
fn a_gesture_writes_rows_and_history_together() {
    let mut temp = TempProject::create().unwrap();
    let id = create_phrase(temp.project_mut(), "Riff");

    let phrase = query::phrase(temp.project().reader(), id).unwrap().unwrap();
    assert_eq!(phrase.spec.name, "Riff");

    // The resolved command in the journal carries the id the executor assigned,
    // which is what makes replay exact.
    let redo: String = temp
        .project()
        .reader()
        .query_row(
            "SELECT redo FROM journal WHERE command = 'create_phrase' ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(redo.contains(&format!("\"id\":{}", id.get())));
}

#[test]
fn a_failed_gesture_leaves_nothing_behind() {
    let mut temp = TempProject::create().unwrap();
    let before = state::model_text(temp.project().reader()).unwrap();

    let outcome = temp.project_mut().gesture(|g| {
        g.exec(Command::CreatePhrase {
            id: None,
            phrase: PhraseSpec::new("Doomed", Tick(PPQ)),
        })?;
        // Referring to a phrase that does not exist fails the whole gesture.
        g.exec(Command::SetPhrase {
            id: PhraseId(999_999),
            patch: PhrasePatch::default(),
        })
    });

    assert!(outcome.is_err());
    let after = state::model_text(temp.project().reader()).unwrap();
    assert_eq!(before, after, "a rolled-back gesture changed state");
}

#[test]
fn committed_state_survives_a_reopen() {
    let mut temp = TempProject::create().unwrap();
    create_phrase(temp.project_mut(), "Durable");
    let before = state::model_text(temp.project().reader()).unwrap();

    temp.reopen().unwrap();

    let after = state::model_text(temp.project().reader()).unwrap();
    assert_eq!(before, after);
    assert!(
        query::phrase_by_name(temp.project().reader(), "Durable")
            .unwrap()
            .is_some()
    );
}

#[test]
fn undo_and_redo_restore_state_exactly() {
    let mut temp = TempProject::create().unwrap();
    let baseline = state::model_text(temp.project().reader()).unwrap();

    let id = create_phrase(temp.project_mut(), "Sketch");
    temp.project_mut()
        .apply(Command::AddEvent {
            container: Container::Phrase(id),
            event: vec![
                EventSpec::note(Tick::ZERO, Tick(PPQ), 60, 49_152),
                EventSpec::note(Tick(PPQ), Tick(PPQ), 64, 49_152),
            ],
        })
        .unwrap();
    let populated = state::model_text(temp.project().reader()).unwrap();

    assert!(temp.project_mut().undo().unwrap()); // the events
    assert!(temp.project_mut().undo().unwrap()); // the phrase
    assert_eq!(
        state::model_text(temp.project().reader()).unwrap(),
        baseline,
        "undo did not return to the starting state"
    );

    assert!(temp.project_mut().redo().unwrap());
    assert!(temp.project_mut().redo().unwrap());
    assert_eq!(
        state::model_text(temp.project().reader()).unwrap(),
        populated,
        "redo did not restore the edited state"
    );
}

#[test]
fn undo_restores_removed_rows_from_the_journal() {
    // The inverse of a removal carries the removed rows: they cannot be
    // re-derived later, so they are captured when the command runs.
    let mut temp = TempProject::create().unwrap();
    let id = create_phrase(temp.project_mut(), "Fragile");
    temp.project_mut()
        .apply(Command::AddEvent {
            container: Container::Phrase(id),
            event: vec![EventSpec::note(Tick(PPQ * 2), Tick(PPQ), 67, 40_000)],
        })
        .unwrap();

    let event = query::event_in_phrase(temp.project().reader(), id).unwrap();
    assert_eq!(event.len(), 1);
    let event_id = event[0].id;

    temp.project_mut()
        .apply(Command::RemoveEvent { id: vec![event_id] })
        .unwrap();
    assert!(
        query::event_in_phrase(temp.project().reader(), id)
            .unwrap()
            .is_empty()
    );

    temp.project_mut().undo().unwrap();
    let restored = query::event_in_phrase(temp.project().reader(), id).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, event_id, "the row came back with its id");
    assert_eq!(restored[0].note_number.unwrap().get(), 67);
    assert_eq!(restored[0].velocity, Some(40_000));
}

#[test]
fn undo_survives_a_reopen() {
    // Persistent undo (R-205): history is in the database, not in memory.
    let mut temp = TempProject::create().unwrap();
    let baseline = state::model_text(temp.project().reader()).unwrap();
    create_phrase(temp.project_mut(), "Yesterday");

    temp.reopen().unwrap();

    assert!(temp.project_mut().undo().unwrap());
    assert_eq!(
        state::model_text(temp.project().reader()).unwrap(),
        baseline
    );
}

#[test]
fn a_new_command_closes_the_redo_stack() {
    let mut temp = TempProject::create().unwrap();
    create_phrase(temp.project_mut(), "First");
    assert!(temp.project_mut().undo().unwrap());
    assert_eq!(journal::depth(temp.project().reader()).unwrap().1, 1);

    create_phrase(temp.project_mut(), "Second");
    assert!(!temp.project_mut().redo().unwrap(), "redo should be closed");
}

#[test]
fn a_closed_redo_stack_stays_closed_after_a_later_undo() {
    // The subtle half of the rule above: invalidation is not merely "the last
    // journal entry is a command". Undoing again puts a marker back at the end, and
    // a redo must still not reach past the new command to the stale gesture.
    // Redo undoes the last undo, and nothing else. (Found by proptest; the symptom
    // was a NotFound on a row id that the intervening history had moved.)
    let mut temp = TempProject::create().unwrap();
    let first = create_phrase(temp.project_mut(), "First");
    assert!(temp.project_mut().undo().unwrap()); // First is undone
    let second = create_phrase(temp.project_mut(), "Second"); // stack invalidated

    assert!(temp.project_mut().undo().unwrap()); // Second undone; marker is last
    assert!(temp.project_mut().redo().unwrap());

    // Redo must have restored Second, not resurrected First.
    assert!(
        query::phrase(temp.project().reader(), second)
            .unwrap()
            .is_some(),
        "redo restored the wrong gesture"
    );
    assert!(
        query::phrase(temp.project().reader(), first)
            .unwrap()
            .is_none(),
        "a superseded undone gesture came back"
    );
    assert!(!temp.project_mut().redo().unwrap(), "redo should be spent");
}

#[test]
fn undo_reports_when_there_is_nothing_left() {
    let mut temp = TempProject::create().unwrap();
    // Genesis is itself a gesture, so exactly one undo is available.
    assert!(temp.project_mut().undo().unwrap());
    assert!(!temp.project_mut().undo().unwrap());
}

#[test]
fn replay_reproduces_model_state() {
    // The determinism gate for history: a project's journal is a faithful
    // recipe for its state, including the genesis gesture.
    let mut source = TempProject::create().unwrap();
    fixture::mhall(source.project_mut()).unwrap();
    let id = create_phrase(source.project_mut(), "Extra");
    source
        .project_mut()
        .apply(Command::SetPhrase {
            id,
            patch: PhrasePatch {
                name: Some("Renamed".to_string()),
                ..Default::default()
            },
        })
        .unwrap();
    source.project_mut().undo().unwrap();

    let mut target = TempProject::create_bare().unwrap();
    replay::replay(source.project().reader(), target.project_mut()).unwrap();

    state::assert_same_model(source.project().reader(), target.project().reader());
}

#[test]
fn replay_reproduces_materialized_frequencies_bit_for_bit() {
    let source = TempProject::create().unwrap();
    let mut target = TempProject::create_bare().unwrap();
    replay::replay(source.project().reader(), target.project_mut()).unwrap();

    // state::table_text renders reals by their bits, so equality here is
    // bit-equality — what "identical tuning" has to mean.
    assert_eq!(
        state::table_text(source.project().reader(), "materialized_tuning").unwrap(),
        state::table_text(target.project().reader(), "materialized_tuning").unwrap()
    );
}
