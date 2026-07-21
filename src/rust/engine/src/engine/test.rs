//! The real-time contract, exercised through the offline driver — which is the
//! point of a driver-agnostic core: these all run on a machine with no sound
//! card, under the allocation guard.

use super::*;

use crate::command::{Chunk, ChunkHandle};
use crate::driver::Offline;
use crate::port::{EngineSession, session};

const RATE: u32 = 48_000;
const BLOCK: u32 = 480;

fn rig() -> (EngineSession, Offline) {
    let (app, rt) = session();
    let format = Format {
        sample_rate: RATE,
        channel_out: 2,
        channel_in: 0,
        max_block: BLOCK,
    };
    (app, Offline::new(format, rt))
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

#[test]
fn silence_until_told_otherwise() {
    let (_app, mut offline) = rig();
    let out = offline.render(RATE as usize / 10);
    assert_eq!(
        peak(&out),
        0.0,
        "an engine with nothing to do makes nothing"
    );
}

#[test]
fn a_tone_command_makes_sound() {
    let (mut app, mut offline) = rig();
    app.send(Command::now(What::ToneOn {
        hz: 440.0,
        gain: 0.5,
    }))
    .expect("send");

    let out = offline.render(RATE as usize / 10);
    let level = peak(&out);
    assert!(
        level > 0.4 && level <= 0.5,
        "audible at the gain asked for: {level}"
    );
}

#[test]
fn both_channels_are_written() {
    // Filling our channels and leaving the rest is how an HDMI endpoint that
    // reports eight channels plays whatever was in the buffer on six of them.
    let (mut app, mut offline) = rig();
    app.send(Command::now(What::ToneOn {
        hz: 440.0,
        gain: 0.5,
    }))
    .expect("send");
    let out = offline.render(4_800);

    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    let right: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();
    assert_eq!(left, right, "every channel carries the same tone");
    assert!(peak(&right) > 0.4);
}

#[test]
fn the_transport_advances_only_while_running() {
    let (mut app, mut offline) = rig();

    offline.render(BLOCK as usize * 4);
    let stopped = app.position();
    assert!(!stopped.running);
    assert_eq!(
        stopped.play,
        SampleTime(0),
        "play does not move when stopped"
    );
    assert_eq!(
        stopped.at,
        SampleTime(BLOCK as u64 * 4),
        "the clock always moves"
    );

    app.send(Command::now(What::Start)).expect("send");
    offline.render(BLOCK as usize * 4);
    let running = app.position();
    assert!(running.running);
    assert_eq!(running.play, SampleTime(BLOCK as u64 * 4));
    assert_eq!(running.at, SampleTime(BLOCK as u64 * 8));
}

#[test]
fn locate_moves_the_transport_without_moving_the_clock() {
    let (mut app, mut offline) = rig();
    app.send(Command::now(What::Locate(SampleTime(96_000))))
        .expect("send");
    offline.render(BLOCK as usize);

    let seen = app.position();
    assert_eq!(seen.play, SampleTime(96_000));
    assert_eq!(
        seen.at,
        SampleTime(BLOCK as u64),
        "the session clock is its own thing"
    );
}

#[test]
fn a_loop_wraps_by_length_rather_than_snapping() {
    // A block boundary that straddles the loop point must neither lose nor
    // repeat samples — snapping to the start would do both.
    let (mut app, mut offline) = rig();
    app.send(Command::now(What::SetLoop {
        from: SampleTime(1_000),
        to: SampleTime(1_600),
        on: true,
    }))
    .expect("send");
    app.send(Command::now(What::Locate(SampleTime(1_000))))
        .expect("send");
    app.send(Command::now(What::Start)).expect("send");

    // 600 frames of loop, 480-frame blocks: the second block straddles.
    offline.render(BLOCK as usize * 2);
    let seen = app.position();
    assert_eq!(seen.play, SampleTime(1_000 + (960 - 600)));
}

#[test]
fn a_scheduled_command_lands_on_the_block_that_contains_it() {
    // Every command carries a time; one stamped into the future must not act
    // early. This is the mechanism an arpeggiator will use unchanged.
    let (mut app, mut offline) = rig();
    app.send(Command::at(
        SampleTime(BLOCK as u64 * 3),
        What::ToneOn {
            hz: 440.0,
            gain: 1.0,
        },
    ))
    .expect("send");

    let early = offline.render(BLOCK as usize * 3);
    assert_eq!(peak(&early), 0.0, "not a sample early");

    let later = offline.render(BLOCK as usize);
    assert!(peak(&later) > 0.0, "and not late either");
}

#[test]
fn a_chunk_is_taken_and_returned_but_never_freed_by_the_engine() {
    let (mut app, mut offline) = rig();
    let first = ChunkHandle::new(Chunk {
        from: SampleTime(0),
        to: SampleTime(48_000),
        note: Vec::new(),
    });
    let second = ChunkHandle::new(Chunk {
        from: SampleTime(48_000),
        to: SampleTime(96_000),
        note: Vec::new(),
    });

    app.send(Command::now(What::TakeChunk(first)))
        .expect("send");
    offline.render(BLOCK as usize);

    // Superseding the first chunk sends it home; nothing has been freed on the
    // audio thread, which the allocation guard would have caught.
    app.send(Command::now(What::TakeChunk(second)))
        .expect("send");
    offline.render(BLOCK as usize);

    let mut returned = 0;
    app.drain_obs(|obs| {
        if obs.code == Code::ChunkReleased {
            returned += 1;
        }
    });
    assert_eq!(returned, 1, "exactly the superseded chunk came home");

    // `collect` frees it; the session's Drop frees the one still held.
    app.collect();
}

#[test]
fn the_engine_says_what_it_did() {
    let (mut app, mut offline) = rig();
    app.send(Command::now(What::Start)).expect("send");
    app.send(Command::now(What::ToneOn {
        hz: 440.0,
        gain: 0.25,
    }))
    .expect("send");
    app.send(Command::now(What::Stop)).expect("send");
    offline.render(BLOCK as usize);

    let mut said = Vec::new();
    app.drain_obs(|obs| said.push((obs.code, obs.render(RATE))));

    let codes: Vec<Code> = said.iter().map(|(code, _)| *code).collect();
    assert_eq!(
        codes,
        vec![Code::TransportStart, Code::ToneOn, Code::TransportStop],
        "in the order they happened"
    );
    assert_eq!(said[1].1, "tone on: 440.000 Hz");
}

#[test]
fn all_notes_off_silences_everything() {
    let (mut app, mut offline) = rig();
    app.send(Command::now(What::ToneOn {
        hz: 440.0,
        gain: 1.0,
    }))
    .expect("send");
    offline.render(4_800);

    app.send(Command::now(What::AllNotesOff)).expect("send");
    // Long enough for the ramp to reach zero and stay there.
    offline.render(4_800);
    let after = offline.render(4_800);
    assert_eq!(peak(&after), 0.0, "unconditional silence, always available");
}

#[test]
fn a_variable_block_size_changes_nothing() {
    // The host may hand us a different size every callback; assuming a fixed
    // block is the classic bug. Two renders of the same span in different block
    // shapes must agree exactly.
    let render = |chunk: usize| {
        let (mut app, rt) = session();
        let format = Format {
            sample_rate: RATE,
            channel_out: 2,
            channel_in: 0,
            max_block: chunk as u32,
        };
        let mut offline = Offline::new(format, rt);
        app.send(Command::now(What::ToneOn {
            hz: 440.0,
            gain: 0.5,
        }))
        .expect("send");
        offline.render(9_600)
    };
    assert_eq!(render(480), render(64), "block size is not audible");
}

#[test]
fn rendering_twice_is_bit_identical() {
    // R-1402's gate, working before there is anything complicated to render.
    // Deliberately checked with `==` on the bits, not a tolerance.
    let render = || {
        let (mut app, rt) = session();
        let mut offline = Offline::new(Format::stereo(RATE, BLOCK), rt);
        app.send(Command::now(What::Start)).expect("send");
        app.send(Command::at(
            SampleTime(2_400),
            What::ToneOn {
                hz: 261.625_57,
                gain: 0.7,
            },
        ))
        .expect("send");
        app.send(Command::at(SampleTime(19_200), What::ToneOff))
            .expect("send");
        offline.render(24_000)
    };

    let first = render();
    let second = render();
    assert_eq!(
        first.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        second.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        "identical input renders bit-identically"
    );
}

#[test]
fn callback_timing_is_measured() {
    let (app, mut offline) = rig();
    offline.render(BLOCK as usize * 10);
    let seen = app.position();
    assert_eq!(seen.block, 10);
    assert!(
        seen.callback_worst_us >= seen.callback_us,
        "worst is a running maximum"
    );
    assert_eq!(
        seen.sample_rate, RATE,
        "the rate it was told, not one assumed"
    );
}

#[test]
fn the_clock_correlation_pair_advances() {
    let (app, mut offline) = rig();
    offline.render(BLOCK as usize);
    let first = app.position();
    offline.render(BLOCK as usize);
    let second = app.position();

    assert!(second.correlate_at > first.correlate_at);
    assert!(
        second.correlate_nanos >= first.correlate_nanos,
        "monotonic, which is all a correlation needs"
    );
}

#[test]
fn xruns_are_recorded_not_corrected() {
    let (mut app, mut offline) = rig();
    offline.engine().note_xrun();
    offline.render(BLOCK as usize);

    assert_eq!(app.position().xrun, 1);
    let mut warned = false;
    app.drain_obs(|obs| {
        if obs.code == Code::Xrun {
            warned = true;
            assert_eq!(obs.level, Level::Warn);
        }
    });
    assert!(
        warned,
        "there is nothing to correct, only something to know"
    );
}

#[test]
fn overflowing_the_pending_set_is_reported_rather_than_absorbed() {
    let (mut app, mut offline) = rig();
    // More future-stamped commands than the fixed pending set can hold. The
    // callback may not allocate, so the set has a limit — and hitting it is a
    // real event, not something to swallow.
    for n in 0..(PENDING as u64 + 8) {
        app.send(Command::at(SampleTime(1_000_000 + n), What::ToneOff))
            .expect("send");
    }
    offline.render(BLOCK as usize);

    let mut lost = 0;
    app.drain_obs(|obs| {
        if obs.code == Code::PendingFull {
            lost = obs.arg[0];
        }
    });
    assert_eq!(lost, 8, "every one that could not be held is counted");
}
