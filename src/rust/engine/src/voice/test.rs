use super::*;

use crate::graph::{BiquadMode, GraphSpec, NodeKind, NodeSpec, ParamId};
use crate::table::{Table, TableId, TableSet};

const RATE: u32 = 48_000;

/// A one-cycle sine table at 100 Hz, so a voice asked for 100 Hz reads it at
/// exactly its natural rate and the arithmetic stays checkable.
fn tables() -> TableSet {
    let mut set = TableSet::new();
    let len = (RATE / 100) as usize;
    let sample: Vec<f32> = (0..len)
        .map(|n| (std::f32::consts::TAU * n as f32 / len as f32).sin())
        .collect();
    set.add(Table::new(sample, 100.0));
    set
}

/// Source → gain, the smallest useful graph.
fn simple() -> Graph {
    let mut spec = GraphSpec::new();
    let source = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let gain = spec.add(NodeSpec::new(NodeKind::Gain).with(ParamId::Gain, 1.0));
    spec.connect(source, gain).output(gain);
    Graph::build(&spec).expect("build")
}

fn render(pool: &mut VoicePool, table: &TableSet, quanta: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; QUANTUM * 2];
    let mut collected = Vec::new();
    for _ in 0..quanta {
        out.fill(0.0);
        pool.render(
            table,
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
        collected.extend_from_slice(&out[..QUANTUM]);
    }
    collected
}

#[test]
fn a_fresh_pool_is_entirely_free() {
    let pool = VoicePool::new(simple(), 8, RATE);
    assert_eq!(pool.len(), 8);
    assert_eq!(pool.free_count(), 8);
    assert_eq!(pool.sounding(), 0);
}

#[test]
fn a_note_takes_a_voice_and_makes_a_sound() {
    let table = tables();
    let mut pool = VoicePool::new(simple(), 4, RATE);
    assert!(pool.start(100.0, 1.0, QUANTUM as u64 * 4, 0, 1, None));
    assert_eq!(pool.free_count(), 3);

    let out = render(&mut pool, &table, 1);
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.5, "the voice should be audible: {peak}");
}

#[test]
fn a_start_offset_delays_the_onset_within_the_quantum() {
    // The mechanism that makes onsets sample-accurate without splitting the
    // block. A voice starting at frame 64 writes silence before it.
    let table = tables();
    let mut pool = VoicePool::new(simple(), 4, RATE);
    pool.start(100.0, 1.0, QUANTUM as u64 * 4, 64, 1, None);

    let mut out = vec![0.0f32; QUANTUM * 2];
    pool.render(
        &table,
        Span {
            phase: 0,
            frames: QUANTUM,
            stride: QUANTUM,
        },
        &mut out,
    );

    let before = out[..64].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let after = out[64..QUANTUM].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert_eq!(before, 0.0, "silent until its offset");
    assert!(after > 0.0, "sounding after it");
}

#[test]
fn a_voice_returns_when_its_release_finishes_not_when_its_duration_expires() {
    // The rule the whole pool rests on. A duration reaching zero begins the
    // release; the voice is reclaimed a step later, and only in `render`.
    let table = tables();
    let mut pool = VoicePool::new(simple(), 1, RATE);
    pool.start(100.0, 1.0, QUANTUM as u64, 0, 1, None);
    assert_eq!(pool.free_count(), 0);

    // First quantum: the duration expires, so the voice enters release — and is
    // still sounding, which is exactly the point.
    render(&mut pool, &table, 1);
    assert_eq!(pool.free_count(), 0, "released, not finished");
    assert_eq!(pool.sounding(), 1);

    // Second: the release completes and the voice comes back.
    render(&mut pool, &table, 1);
    assert_eq!(
        pool.free_count(),
        1,
        "reclaimed after the release, not before"
    );
    assert_eq!(pool.sounding(), 0);
}

#[test]
fn reclamation_happens_only_in_render() {
    // Which is what makes the invariant checkable: a voice that is not free at
    // the end of a render is still making a sound. If reclamation happened
    // anywhere else, "not free" would stop meaning anything.
    let mut pool = VoicePool::new(simple(), 2, RATE);
    pool.start(100.0, 1.0, 0, 0, 1, None);
    pool.start(100.0, 1.0, 0, 0, 2, None);
    assert_eq!(pool.free_count(), 0, "nothing is reclaimed by starting");
}

#[test]
fn stealing_takes_a_releasing_voice_before_a_held_one() {
    // A releasing voice is making a sound nobody is playing any more. That
    // distinction only exists because release is a state rather than a fade.
    let table = tables();
    let mut pool = VoicePool::new(simple(), 2, RATE);

    // One long note, one that expires immediately.
    pool.start(100.0, 1.0, QUANTUM as u64 * 1000, 0, 1, None);
    pool.start(200.0, 1.0, 0, 0, 2, None);

    // Render enough for the short one to enter release but not to finish: one
    // quantum moves it from Held to Releasing.
    let mut out = vec![0.0f32; QUANTUM * 2];
    pool.render(
        &table,
        Span {
            phase: 0,
            frames: QUANTUM,
            stride: QUANTUM,
        },
        &mut out,
    );

    // Both slots busy; the pool must steal, and must take the releasing one.
    assert_eq!(pool.free_count(), 0);
    assert!(pool.start(300.0, 1.0, QUANTUM as u64 * 100, 0, 3, None));
    assert_eq!(pool.stolen(), 1, "a steal happened and was recorded");

    // Two voices sounding: the long held note and the new one. If the held note
    // had been taken instead, the releasing one would have finished on the next
    // render and only one would remain.
    assert_eq!(pool.sounding(), 2);
    render(&mut pool, &table, 2);
    assert_eq!(
        pool.sounding(),
        2,
        "neither of these is near its end; the releasing voice was the one taken"
    );
}

#[test]
fn stealing_is_counted_because_it_is_the_signal_that_matters() {
    // Starvation is unreachable — stealing always succeeds while the pool is
    // non-empty — so counting it would count nothing. What is worth knowing is
    // how often a note displaced another, which is how you learn the voice count
    // is too low.
    let mut pool = VoicePool::new(simple(), 1, RATE);
    assert!(pool.start(100.0, 1.0, u64::MAX, 0, 1, None));
    assert_eq!(pool.stolen(), 0, "the first note took a free voice");

    assert!(pool.start(200.0, 1.0, u64::MAX, 0, 2, None));
    assert_eq!(pool.stolen(), 1, "the second displaced it");
}

#[test]
fn release_all_releases_every_held_voice() {
    let table = tables();
    let mut pool = VoicePool::new(simple(), 4, RATE);
    for n in 0..4 {
        pool.start(100.0 * (n + 1) as f32, 0.5, u64::MAX, 0, n as u64, None);
    }
    assert_eq!(pool.sounding(), 4);

    pool.release_all();
    render(&mut pool, &table, 1);
    assert_eq!(
        pool.free_count(),
        4,
        "unconditional silence, always available"
    );
}

#[test]
fn seeds_decorrelate_the_read_heads() {
    // Two heads from one table with different start offsets is how one bake
    // becomes a wide stereo image. Seeded, unlike the source this is ported
    // from, so a render is reproducible (R-706).
    let table = tables();
    let mut spec = GraphSpec::new();
    let a = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let b = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let mix = spec.add(NodeSpec::new(NodeKind::Gain).with(ParamId::Gain, 0.5));
    spec.connect(a, mix).connect(b, mix).output(mix);
    let graph = Graph::build(&spec).expect("build");

    let sound = |seed: u64| {
        let mut pool = VoicePool::new(graph.clone(), 1, RATE);
        pool.start(100.0, 1.0, QUANTUM as u64 * 8, 0, seed, None);
        render(&mut pool, &table, 1)
    };

    // The same seed renders identically; a different one does not.
    assert_eq!(sound(7), sound(7), "seeded, so reproducible");
    assert_ne!(sound(7), sound(8), "and genuinely decorrelated");
}

#[test]
fn the_panner_makes_stereo_and_keeps_its_energy() {
    let table = tables();
    let mut spec = GraphSpec::new();
    let source = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let pan = spec.add(NodeSpec::new(NodeKind::StereoPanner).with(ParamId::Pan, 0.0));
    spec.connect(source, pan).output(pan);
    let mut pool = VoicePool::new(Graph::build(&spec).expect("build"), 1, RATE);
    pool.start(100.0, 1.0, QUANTUM as u64 * 4, 0, 1, None);

    let mut out = vec![0.0f32; QUANTUM * 2];
    pool.render(
        &table,
        Span {
            phase: 0,
            frames: QUANTUM,
            stride: QUANTUM,
        },
        &mut out,
    );

    let left = out[..QUANTUM].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let right = out[QUANTUM..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(right > 0.0, "the panner writes both channels");
    assert!(
        (left - right).abs() < 1e-5,
        "centred should be equal: {left} vs {right}"
    );
    // Equal power: centred keeps 1/√2 per side rather than 1/2, so a sweep does
    // not dip in the middle.
    assert!(
        (left - 0.707).abs() < 0.02,
        "equal-power centre is 1/sqrt(2), got {left}"
    );
}

#[test]
fn the_filter_removes_what_it_should() {
    // A lowpass well below the tone should take most of it away; well above,
    // almost none. Not a precise response test — that arrives with eng-04 — but
    // enough that a filter wired backwards cannot pass.
    let table = tables();
    let build = |hz: f32| {
        let mut spec = GraphSpec::new();
        let source = spec.add(NodeSpec::new(NodeKind::BufferSource {
            table: TableId(0),
            looping: true,
        }));
        let filter = spec.add(
            NodeSpec::new(NodeKind::Biquad {
                mode: BiquadMode::Lowpass,
            })
            .with(ParamId::Frequency, hz)
            .with(ParamId::Q, 0.707),
        );
        spec.connect(source, filter).output(filter);
        Graph::build(&spec).expect("build")
    };

    let level = |hz: f32| {
        let mut pool = VoicePool::new(build(hz), 1, RATE);
        pool.start(1000.0, 1.0, QUANTUM as u64 * 64, 0, 1, None);
        let out = render(&mut pool, &table, 16);
        // Skip the first quantum: the filter's state starts at rest.
        out[QUANTUM..].iter().fold(0.0f32, |m, s| m.max(s.abs()))
    };

    let open = level(15_000.0);
    let closed = level(80.0);
    assert!(open > 0.5, "an open filter passes the tone: {open}");
    assert!(
        closed < open * 0.25,
        "a closed one should take most of it: {closed} vs {open}"
    );
}

#[test]
fn rendering_allocates_nothing() {
    // The property the whole design exists for, and the reason the guard is
    // wired into the offline path: CI enforces it on a machine with no real
    // time. Two defects were caught here while this was being written — a `Vec`
    // of parameter values built per node, and a whole buffer *cloned* per node —
    // neither of which any other test would have noticed.
    let table = tables();
    let mut spec = GraphSpec::new();
    let head_a = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let head_b = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let mix = spec.add(NodeSpec::new(NodeKind::Gain).with(ParamId::Gain, 0.707));
    let pan = spec.add(NodeSpec::new(NodeKind::StereoPanner).with(ParamId::Pan, 0.5));
    let amp = spec.add(NodeSpec::new(NodeKind::Gain));
    let filter = spec.add(
        NodeSpec::new(NodeKind::Biquad {
            mode: BiquadMode::Lowpass,
        })
        .with(ParamId::Frequency, 4_000.0),
    );
    spec.connect(head_a, mix)
        .connect(head_b, mix)
        .connect(mix, pan)
        .connect(pan, amp)
        .connect(amp, filter)
        .output(filter);

    let mut pool = VoicePool::new(Graph::build(&spec).expect("build"), 8, RATE);
    let mut out = vec![0.0f32; QUANTUM * 2];

    // Everything that allocates happens before the guard is armed.
    for n in 0..8 {
        pool.start(
            220.0 * (n + 1) as f32,
            0.5,
            QUANTUM as u64 * 4,
            n as usize * 8,
            n as u64,
            None,
        );
    }

    let _rt = crate::guard::RtScope::enter();
    for _ in 0..16 {
        out.fill(0.0);
        pool.render(
            &table,
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
    }
}

#[test]
fn starting_a_note_allocates_nothing_either() {
    // Note-on happens on the audio thread, from the command ring. If taking a
    // voice allocated, every note would be a deadline hazard.
    let table = tables();
    let mut pool = VoicePool::new(simple(), 4, RATE);
    let mut out = vec![0.0f32; QUANTUM * 2];

    let _rt = crate::guard::RtScope::enter();
    for n in 0..64u64 {
        pool.start(440.0, 0.5, QUANTUM as u64, 0, n, None);
        out.fill(0.0);
        pool.render(
            &table,
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
    }
}

#[test]
fn a_release_tail_outlives_the_note_and_decays() {
    // The rule the pool exists to serve, now with a real envelope behind it: the
    // voice keeps sounding after its duration expires, gets quieter, and is
    // reclaimed only when the tail is over.
    let table = tables();
    let mut pool = VoicePool::new(simple(), 1, RATE);
    let tail = QUANTUM as u64 * 8;
    pool.set_release(tail);

    pool.start(100.0, 1.0, QUANTUM as u64, 0, 1, None);

    // Schedule the envelope the way an instrument would: full at the start,
    // approaching silence from the moment the written duration ends.
    let gain_node = 1;
    let voice = pool.voice_mut(0).expect("the voice just started");
    let schedule = voice
        .param_mut(gain_node, ParamId::Gain)
        .expect("the gain's own parameter");
    schedule.schedule().reset(1.0);
    schedule.schedule().set_value_at_time(1.0, 0);
    schedule
        .schedule()
        .set_target_at_time(0.0, QUANTUM as u64, tail as f32 / 5.0);

    let mut level = Vec::new();
    for _ in 0..12 {
        let mut out = vec![0.0f32; QUANTUM * 2];
        pool.render(
            &table,
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
        level.push(out[..QUANTUM].iter().fold(0.0f32, |m, s| m.max(s.abs())));
    }

    assert!(level[0] > 0.5, "sounding while held: {}", level[0]);
    assert!(
        level[2] > 0.01 && level[2] < level[0],
        "still sounding after the duration expired, but quieter: {:?}",
        &level[..4]
    );
    assert!(
        level[8] < level[2],
        "and still decaying: {:?}",
        &level[..10]
    );
    assert_eq!(pool.free_count(), 1, "reclaimed once the tail is over");
    assert_eq!(level[11], 0.0, "and silent afterwards, not merely quiet");
}

#[test]
fn every_note_gets_its_own_envelope() {
    // A voice reused for a second note must not inherit the first note's
    // schedule — otherwise a voice's sound would depend on what it happened to
    // play before, which is both wrong and unreproducible.
    let table = tables();
    let mut pool = VoicePool::new(simple(), 1, RATE);
    pool.set_release(0);

    pool.start(100.0, 1.0, QUANTUM as u64 * 100, 0, 1, None);
    let voice = pool.voice_mut(0).expect("voice");
    voice
        .param_mut(1, ParamId::Gain)
        .expect("gain")
        .schedule()
        .set_value_at_time(0.0, 0);

    let mut out = vec![0.0f32; QUANTUM * 2];
    pool.render(
        &table,
        Span {
            phase: 0,
            frames: QUANTUM,
            stride: QUANTUM,
        },
        &mut out,
    );
    assert_eq!(
        out[..QUANTUM].iter().fold(0.0f32, |m, s| m.max(s.abs())),
        0.0,
        "silenced by its own schedule"
    );

    // Take the same slot for a new note; the silencing schedule must be gone.
    pool.start(100.0, 1.0, QUANTUM as u64 * 100, 0, 2, None);
    out.fill(0.0);
    pool.render(
        &table,
        Span {
            phase: 0,
            frames: QUANTUM,
            stride: QUANTUM,
        },
        &mut out,
    );
    assert!(
        out[..QUANTUM].iter().fold(0.0f32, |m, s| m.max(s.abs())) > 0.5,
        "the new note starts from the patch, not from its predecessor"
    );
}

#[test]
fn a_recycled_buffer_is_cleared_before_it_is_reused() {
    // Buffers are recycled *within* a pass: one freed when its last consumer ran
    // is handed to a later node, still holding that node's output. Accumulating
    // into it without clearing adds a stale signal that bypasses everything in
    // between.
    //
    // The graph below reproduces the shape that found it — a wide branch whose
    // buffers are freed and re-handed to a node downstream of a silencing gain.
    // If clearing were dropped, the silenced path would still be audible.
    let table = tables();
    let mut spec = GraphSpec::new();
    let source = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let pan = spec.add(NodeSpec::new(NodeKind::StereoPanner).with(ParamId::Pan, 0.5));
    // Silences everything downstream of it.
    let mute = spec.add(NodeSpec::new(NodeKind::Gain).with(ParamId::Gain, 0.0));
    let tail = spec.add(NodeSpec::new(NodeKind::Gain).with(ParamId::Gain, 1.0));
    spec.connect(source, pan)
        .connect(pan, mute)
        .connect(mute, tail)
        .output(tail);

    let mut pool = VoicePool::new(Graph::build(&spec).expect("build"), 1, RATE);
    pool.start(100.0, 1.0, QUANTUM as u64 * 8, 0, 1, None);

    let mut out = vec![0.0f32; QUANTUM * 2];
    pool.render(
        &table,
        Span {
            phase: 0,
            frames: QUANTUM,
            stride: QUANTUM,
        },
        &mut out,
    );
    assert_eq!(
        peak_of(&out),
        0.0,
        "a gain of zero silences everything after it, whatever buffer got recycled"
    );
}

fn peak_of(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}
