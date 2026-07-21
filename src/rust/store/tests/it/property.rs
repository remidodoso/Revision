//! Property tests over arbitrary command sequences.
//!
//! Two invariants carry most of the store's design at once:
//!
//! * **History is a faithful recipe** — replaying a project's journal into an
//!   empty one reproduces its model state exactly. This exercises the resolved-
//!   id discipline (nothing may be renumbered), the inverse-command doctrine
//!   (undo and redo markers replay too), and determinism of materialization.
//! * **Undo is exact** — any single gesture, undone, returns the model to
//!   precisely the state before it.
//!
//! Counterexamples persist to `proptest-regressions/`, which is committed, so a
//! shrunk failure becomes a permanent regression test.

use proptest::prelude::*;

use rev_core::phrase::{
    Change, Container, EventSpec, InstanceContainer, PhraseInstancePatch, PhraseInstanceSpec,
    PhrasePatch, PhraseSpec, TrackSpec,
};
use rev_core::tick::{PPQ, Tick};
use rev_core::{Command, PhraseId, PhraseInstanceId, TrackId};
use rev_store::{Project, StoreError, query, replay};
use rev_testkit::{TempProject, state};

/// A small vocabulary of edits, chosen to touch every table the model writes
/// and every inverse shape (create, remove, patch, batch).
#[derive(Debug, Clone)]
enum Op {
    CreatePhrase,
    AddEvent { phrase: usize, note: i32, beat: i64 },
    RemoveFirstEvent { phrase: usize },
    SetLength { phrase: usize, beat: i64 },
    SetTuning { phrase: usize, sixteen: bool },
    CreateInstance { phrase: usize, beat: i64 },
    SetTranspose { instance: usize, by: i32 },
    MuteInstance { instance: usize, mute: bool },
    Undo,
    Redo,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        2 => Just(Op::CreatePhrase),
        4 => (0usize..4, 48i32..84, 0i64..8)
            .prop_map(|(phrase, note, beat)| Op::AddEvent { phrase, note, beat }),
        2 => (0usize..4).prop_map(|phrase| Op::RemoveFirstEvent { phrase }),
        2 => (0usize..4, 1i64..8).prop_map(|(phrase, beat)| Op::SetLength { phrase, beat }),
        1 => (0usize..4, any::<bool>())
            .prop_map(|(phrase, sixteen)| Op::SetTuning { phrase, sixteen }),
        3 => (0usize..4, 0i64..16).prop_map(|(phrase, beat)| Op::CreateInstance { phrase, beat }),
        2 => (0usize..4, -12i32..12).prop_map(|(instance, by)| Op::SetTranspose { instance, by }),
        1 => (0usize..4, any::<bool>())
            .prop_map(|(instance, mute)| Op::MuteInstance { instance, mute }),
        2 => Just(Op::Undo),
        2 => Just(Op::Redo),
    ]
}

/// The ids a run has produced, so operations can refer to earlier ones.
struct World {
    track: TrackId,
    phrase: Vec<PhraseId>,
    instance: Vec<PhraseInstanceId>,
    tuning_16et: rev_core::TuningId,
}

impl World {
    fn new(project: &mut Project) -> Result<World, StoreError> {
        let tuning_16et = query::tuning_by_name(project.reader(), "16-ET")?
            .expect("builtin")
            .id;
        let (arrangement, track) = project.gesture(|g| {
            let arrangement = match g.exec(Command::CreatePhrase {
                id: None,
                phrase: PhraseSpec::new("Arrangement", Tick(PPQ * 64)),
            })? {
                Command::CreatePhrase { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            let track = match g.exec(Command::CreateTrack {
                id: None,
                track: TrackSpec::new(arrangement, "T", 0),
            })? {
                Command::CreateTrack { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            Ok((arrangement, track))
        })?;
        Ok(World {
            track,
            phrase: vec![arrangement],
            instance: Vec::new(),
            tuning_16et,
        })
    }
}

/// The phrases that still exist — undo can remove one the run created earlier,
/// and referring to a deleted row is the test's mistake, not the store's (the
/// foreign key would rightly refuse).
fn live_phrase(project: &Project, world: &World, index: usize) -> Option<PhraseId> {
    let live: Vec<PhraseId> = world
        .phrase
        .iter()
        .copied()
        .filter(|&id| matches!(query::phrase(project.reader(), id), Ok(Some(_))))
        .collect();
    live.get(index % live.len().max(1)).copied()
}

fn live_instance(project: &Project, world: &World, index: usize) -> Option<PhraseInstanceId> {
    let live: Vec<PhraseInstanceId> = world
        .instance
        .iter()
        .copied()
        .filter(|&id| matches!(query::phrase_instance(project.reader(), id), Ok(Some(_))))
        .collect();
    live.get(index % live.len().max(1)).copied()
}

/// Apply one operation, skipping the ones that cannot apply (an index pointing
/// at something undo removed, an undo with nothing to undo). Returns whether
/// anything changed, so the undo property knows which steps to check.
fn apply(project: &mut Project, world: &mut World, op: &Op) -> Result<bool, StoreError> {
    let pick = |project: &Project, world: &World, index: usize| live_phrase(project, world, index);
    match op {
        Op::CreatePhrase => {
            let name = format!("Phrase {}", world.phrase.len());
            let resolved = project.apply(Command::CreatePhrase {
                id: None,
                phrase: PhraseSpec::new(name, Tick(PPQ * 4)),
            })?;
            if let Command::CreatePhrase { id: Some(id), .. } = resolved {
                world.phrase.push(id);
            }
            Ok(true)
        }
        Op::AddEvent { phrase, note, beat } => {
            let Some(id) = pick(project, world, *phrase) else {
                return Ok(false);
            };
            project.apply(Command::AddEvent {
                container: Container::Phrase(id),
                event: vec![EventSpec::note(Tick(PPQ * beat), Tick(PPQ), *note, 40_000)],
            })?;
            Ok(true)
        }
        Op::RemoveFirstEvent { phrase } => {
            let Some(id) = pick(project, world, *phrase) else {
                return Ok(false);
            };
            let event = query::event_in_phrase(project.reader(), id)?;
            let Some(first) = event.first() else {
                return Ok(false);
            };
            project.apply(Command::RemoveEvent { id: vec![first.id] })?;
            Ok(true)
        }
        Op::SetLength { phrase, beat } => {
            let Some(id) = pick(project, world, *phrase) else {
                return Ok(false);
            };
            project.apply(Command::SetPhrase {
                id,
                patch: PhrasePatch {
                    length_tick: Some(Tick(PPQ * beat)),
                    ..Default::default()
                },
            })?;
            Ok(true)
        }
        Op::SetTuning { phrase, sixteen } => {
            let Some(id) = pick(project, world, *phrase) else {
                return Ok(false);
            };
            if !sixteen {
                return Ok(false);
            }
            // The builtin tunings arrive in the genesis gesture, which enough Undos
            // will reverse — same shape as the track guard below. Once 16-ET is
            // gone, referring to it is the test's mistake and the foreign key is
            // right to refuse. (Found by proptest: Undo, Undo, CreatePhrase,
            // CreatePhrase, SetTuning.)
            if query::tuning(project.reader(), world.tuning_16et)?.is_none() {
                return Ok(false);
            }
            project.apply(Command::SetPhrase {
                id,
                patch: PhrasePatch {
                    tuning_id: Change::Set(world.tuning_16et),
                    ..Default::default()
                },
            })?;
            Ok(true)
        }
        Op::CreateInstance { phrase, beat } => {
            let Some(id) = pick(project, world, *phrase) else {
                return Ok(false);
            };
            // The track lives in the run's first gesture, which an Undo can
            // reverse; the store rightly refuses an instance on a gone track.
            if query::track(project.reader(), world.track)?.is_none() {
                return Ok(false);
            }
            let resolved = project.apply(Command::CreatePhraseInstance {
                id: None,
                phrase_instance: PhraseInstanceSpec::new(
                    id,
                    InstanceContainer::Track(world.track),
                    Tick(PPQ * beat),
                ),
            })?;
            if let Command::CreatePhraseInstance { id: Some(id), .. } = resolved {
                world.instance.push(id);
            }
            Ok(true)
        }
        Op::SetTranspose { instance, by } => {
            let Some(id) = live_instance(project, world, *instance) else {
                return Ok(false);
            };
            project.apply(Command::SetPhraseInstanceParam {
                id,
                patch: PhraseInstancePatch {
                    transpose: Some(*by),
                    ..Default::default()
                },
            })?;
            Ok(true)
        }
        Op::MuteInstance { instance, mute } => {
            let Some(id) = live_instance(project, world, *instance) else {
                return Ok(false);
            };
            project.apply(Command::SetPhraseInstanceParam {
                id,
                patch: PhraseInstancePatch {
                    mute: Some(*mute),
                    ..Default::default()
                },
            })?;
            Ok(true)
        }
        Op::Undo => project.undo(),
        Op::Redo => project.redo(),
    }
}

proptest! {
    // Each case builds two SQLite projects on disk, so the case count is
    // deliberately modest; shrinking still finds minimal counterexamples.
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn replay_reproduces_model_state(op in prop::collection::vec(op_strategy(), 1..14)) {
        let mut source = TempProject::create().unwrap();
        let mut world = World::new(source.project_mut()).unwrap();
        for item in &op {
            apply(source.project_mut(), &mut world, item).unwrap();
        }

        let mut target = TempProject::create_bare().unwrap();
        replay::replay(source.project().reader(), target.project_mut()).unwrap();

        for table in rev_store::schema::MODEL_TABLE {
            prop_assert_eq!(
                state::table_text(source.project().reader(), table).unwrap(),
                state::table_text(target.project().reader(), table).unwrap(),
                "table `{}` differs after replay", table
            );
        }
    }

    #[test]
    fn undo_returns_to_the_prior_state(op in prop::collection::vec(op_strategy(), 1..10)) {
        let mut temp = TempProject::create().unwrap();
        let mut world = World::new(temp.project_mut()).unwrap();

        for item in &op {
            // Undo and redo move the cursor rather than adding a gesture to
            // check, so they are applied but not themselves probed.
            if matches!(item, Op::Undo | Op::Redo) {
                apply(temp.project_mut(), &mut world, item).unwrap();
                continue;
            }
            let before = state::model_text(temp.project().reader()).unwrap();
            if !apply(temp.project_mut(), &mut world, item).unwrap() {
                continue;
            }
            prop_assert!(temp.project_mut().undo().unwrap(), "nothing to undo");
            let after_undo = state::model_text(temp.project().reader()).unwrap();
            prop_assert_eq!(before, after_undo, "undo did not restore the prior state");
            // Put it back so the sequence continues from where it was.
            prop_assert!(temp.project_mut().redo().unwrap(), "nothing to redo");
        }
    }
}
