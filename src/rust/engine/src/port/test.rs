use super::*;

use crate::command::{Chunk, ChunkHandle, What};
use crate::obs::{Code, Creator, Level, Obs};
use crate::time::SampleTime;

#[test]
fn commands_refuse_rather_than_drop_when_the_ring_is_full() {
    // The asymmetry that matters: a command is user intent, and silently
    // dropping "stop" is not acceptable. The refused command comes back to its
    // sender rather than vanishing.
    let (mut app, _rt) = session();
    for _ in 0..COMMAND_CAPACITY {
        app.send(Command::now(What::Start)).expect("fits");
    }

    let refused = app
        .send(Command::now(What::Stop))
        .expect_err("the ring is full");
    assert_eq!(refused.0.what, What::Stop, "the command comes home intact");
}

#[test]
fn observations_drop_and_count_when_the_ring_is_full() {
    // The other side of the asymmetry: an observation is not intent, and
    // blocking the audio thread to preserve a log line inverts the priority
    // order the whole design rests on.
    let (_app, mut rt) = session();
    for _ in 0..OBS_CAPACITY {
        rt.observe(Obs::new(Creator::Stream, Level::Info, Code::ToneOff));
    }
    assert_eq!(rt.dropped, 0);

    rt.observe(Obs::new(Creator::Stream, Level::Info, Code::ToneOff));
    assert_eq!(rt.dropped, 1, "counted, not blocked on");
}

#[test]
fn the_return_ring_never_drops() {
    let (mut app, mut rt) = session();

    // Distinct handles, one per slot: the session frees everything it collects,
    // so re-using one handle would be a double free — which is exactly the
    // ownership rule this ring exists to enforce.
    for n in 0..GARBAGE_CAPACITY {
        let handle = ChunkHandle::new(Chunk {
            from: SampleTime(n as u64),
            to: SampleTime(n as u64 + 1),
        });
        rt.release(Garbage::Chunk(handle)).expect("fits");
    }

    let overflow = ChunkHandle::new(Chunk {
        from: SampleTime(9_999),
        to: SampleTime(10_000),
    });
    // Full: the value comes *back*, so the engine can hold and retry. Dropping
    // it would leak; freeing it on the audio thread would allocate.
    let unsent = rt
        .release(Garbage::Chunk(overflow))
        .expect_err("the ring is full");
    match unsent {
        Garbage::Chunk(returned) => assert_eq!(returned, overflow),
    }

    // SAFETY: the overflow handle never entered the ring, so this side still
    // owns it; the other 256 are freed by `collect`.
    unsafe { overflow.release() };
    app.collect();
}

#[test]
fn a_session_frees_what_the_engine_handed_back() {
    let (mut app, mut rt) = session();
    let handle = ChunkHandle::new(Chunk {
        from: SampleTime(0),
        to: SampleTime(480),
    });
    rt.release(Garbage::Chunk(handle)).expect("fits");

    // `collect` is the only place engine-side allocations are freed. Dropping
    // the session must also do it, or tearing one down leaks every chunk still
    // in flight — which is what the Drop impl is for.
    drop(rt);
    app.collect();
}

#[test]
fn position_crosses_without_a_ring() {
    let (app, rt) = session();
    assert_eq!(app.position().block, 0);

    rt.publish(Position {
        block: 42,
        at: SampleTime(20_160),
        ..Position::default()
    });
    assert_eq!(app.position().block, 42);
}
