//! MHALL, from stored material to samples.
//!
//! Everything joins here: the store's realization view, the compiler's tick →
//! sample arithmetic, the tuning resolved to frequencies, the engine's
//! session-anchored quanta, and a voice pool. **Headless** — no device, no
//! window — which is the whole reason the driver-agnostic core exists.

use rev_core::tick::{Tick, bpm_to_usec_per_quarter};
use rev_dsp::BakeSpec;
use rev_dsp::bake::{BASE_HIGH, BASE_LOW, TABLE_LEN, bake, base_hz};
use rev_engine::driver::Offline;
use rev_engine::instrument::MIDDLE_C;
use rev_engine::{
    Chunk, ChunkHandle, Command, Format, Instrument, Patch, SampleTime, Table, TableSet, What,
    session,
};
use rev_sched::{Compiler, TempoMap};
use rev_store::query;
use rev_testkit::{TempProject, fixture};

const RATE: u32 = 48_000;
const QUARTER: u64 = 24_000;
/// Eight bars at 120 bpm, plus room for the last note's tail.
///
/// The tail is patch-dependent, and this had to grow: Harpington releases over
/// 0.44 s where the stand-in plucked patch released over 0.12 s, and the pool
/// holds a voice for five time constants. The last note ends at 16 s and is
/// still audibly ringing at 17 s — correctly.
const SPAN: usize = RATE as usize * 20;

/// The real Padlington set: sixteen tables, half an octave apart.
///
/// It replaces a hand-rolled band-limited sawtooth that stood in until dsp-02.
/// The stand-in was one table at 23 Hz, read up to 3.9× — which aliased audibly
/// and was the second of the two defects heard rather than measured in eng-07.
fn padlington(sample_rate: u32) -> TableSet {
    let spec = BakeSpec::harpington();
    let mut set = TableSet::new();
    for n in BASE_LOW..=BASE_HIGH {
        let base = base_hz(f64::from(MIDDLE_C), n);
        set.add(Table::new(
            bake(&spec, base, sample_rate, TABLE_LEN),
            base as f32,
        ));
    }
    set
}

/// Compile MHALL and render it through a real instrument, headless.
fn play_mhall() -> Vec<f32> {
    let mut temp = TempProject::create().expect("project");
    let built = fixture::mhall(temp.project_mut()).expect("mhall");

    let point: Vec<(Tick, i64)> = query::tempo_point(temp.project().reader(), built.arrangement)
        .expect("tempo")
        .into_iter()
        .map(|p| (p.at_tick, p.usec_per_quarter))
        .collect();
    let mut compiler = Compiler::new(TempoMap::new(point, RATE), vec![built.track]);
    let chunk = compiler
        .chunk(temp.project(), SampleTime(0), SampleTime(SPAN as u64))
        .expect("compile");
    assert_eq!(chunk.note.len(), 26, "the whole tune in one chunk");

    let (mut app, rt) = session();
    let mut offline = Offline::new(Format::stereo(RATE, 480), rt);
    offline.engine().set_instrument(
        Instrument::new(Patch::harpington(), padlington(RATE), 16, RATE).expect("instrument"),
    );

    app.send(Command::now(What::TakeChunk(ChunkHandle::new(Chunk {
        from: chunk.from,
        to: chunk.to,
        note: chunk.note,
    }))))
    .expect("send");
    app.send(Command::now(What::Start)).expect("send");

    offline.render(SPAN)
}

fn peak_near(samples: &[f32], at_frame: usize, window: usize) -> f32 {
    let from = at_frame.saturating_sub(window / 2) * 2;
    let to = ((at_frame + window / 2) * 2).min(samples.len());
    samples[from..to].iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

#[test]
fn mhall_sounds() {
    let out = play_mhall();
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "the tune should be audible: {peak}");
}

#[test]
fn every_note_lands_on_its_beat() {
    // The whole chain in one assertion: ticks became samples, the transport
    // advanced, and a voice started at each onset. Silence between the notes of
    // a plucked patch is what makes this checkable at all.
    let out = play_mhall();

    // The tune is 26 notes; the first four are one quarter each.
    for beat in 0..4usize {
        let at = beat * QUARTER as usize;
        let onset = peak_near(&out, at + 600, 800);
        assert!(
            onset > 0.02,
            "nothing sounding just after beat {beat} (sample {at}): {onset}"
        );
    }

    // And the last note is a whole note at 28 quarters.
    let last = 28 * QUARTER as usize;
    assert!(
        peak_near(&out, last + 600, 800) > 0.02,
        "the final note is missing"
    );
}

#[test]
fn the_tune_stops_when_the_tune_stops() {
    // Eight bars at 120 bpm is 16 seconds, and the last note's release runs
    // five time constants past that. After it there should be nothing — a
    // voice that never released would show up here as a note that outlives the
    // piece.
    let out = play_mhall();
    let tail = &out[(RATE as usize * 19) * 2..];
    let peak = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak < 0.001, "something is still sounding at 19 s: {peak}");
}

#[test]
fn rendering_the_tune_twice_is_bit_identical() {
    // R-1402's gate, on real material rather than a test tone: the schedule, the
    // frequencies, the seeded read-head offsets and the envelopes all have to be
    // deterministic for this to hold.
    let first = play_mhall();
    let second = play_mhall();
    assert_eq!(
        first.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        second.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        "identical project state renders bit-identically"
    );
}

/// Render MHALL at a given device buffer size, with a given voice count.
fn render_at(max_block: u32, voices: usize) -> Vec<f32> {
    let mut temp = TempProject::create().expect("project");
    let built = fixture::mhall(temp.project_mut()).expect("mhall");
    let mut compiler = Compiler::new(
        TempoMap::new([(Tick(0), bpm_to_usec_per_quarter(120.0))], RATE),
        vec![built.track],
    );
    let chunk = compiler
        .chunk(temp.project(), SampleTime(0), SampleTime(SPAN as u64))
        .expect("compile");

    let (mut app, rt) = session();
    let mut offline = Offline::new(Format::stereo(RATE, max_block), rt);
    offline.engine().set_instrument(
        Instrument::new(Patch::harpington(), padlington(RATE), voices, RATE).expect("instrument"),
    );
    app.send(Command::now(What::TakeChunk(ChunkHandle::new(Chunk {
        from: chunk.from,
        to: chunk.to,
        note: chunk.note,
    }))))
    .expect("send");
    app.send(Command::now(What::Start)).expect("send");
    offline.render(RATE as usize * 4)
}

#[test]
fn the_device_block_size_is_not_audible() {
    // The reason the graph runs in quanta anchored to the session clock rather
    // than to the callback: a project must not render differently on different
    // hardware.
    //
    // With voices to spare — the ordinary case — this holds exactly. See the
    // ignored test below for the case that does not yet.
    let small = render_at(64, 64);
    let large = render_at(1024, 64);
    assert_eq!(
        small.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        large.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        "the device's buffer size must not change what anything sounds like"
    );
}

/// **A known gap, reproduced rather than described.**
///
/// When the pool is small enough that notes steal voices from one another, the
/// render is no longer independent of the device buffer size: two runs diverge
/// in the last bits and the difference then grows through a resonant filter.
///
/// What is already fixed, and what this is not: k-rate parameters are evaluated
/// at quantum boundaries, and voice state transitions happen only on the call
/// that finishes a quantum, so reclamation order no longer depends on how the
/// device chopped up time. Something in the stealing path still does. The
/// suspicion is that which slot a stolen note lands in can differ, and because
/// voices are summed in slot order and floating-point addition is not
/// associative, the sum differs.
///
/// It matters for R-1402 whenever an arrangement is dense enough to steal, which
/// is not rare. Left failing and visible rather than deleted.
#[test]
#[ignore = "R-1402: renders diverge across buffer sizes once voices are stolen (eng-07 finding)"]
fn the_device_block_size_is_not_audible_even_when_voices_are_stolen() {
    let small = render_at(64, 4);
    let large = render_at(1024, 4);
    assert_eq!(
        small.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        large.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        "stealing must not make the buffer size audible"
    );
}

#[test]
fn retuning_the_phrase_changes_the_sound_and_not_the_timing() {
    // The party trick, audible: one command moves every pitch and no onset.
    let render = |sixteen: bool| {
        let mut temp = TempProject::create().expect("project");
        let built = fixture::mhall(temp.project_mut()).expect("mhall");
        if sixteen {
            temp.project_mut()
                .apply(rev_core::Command::SetPhrase {
                    id: built.melody,
                    patch: rev_core::phrase::PhrasePatch {
                        tuning_id: rev_core::phrase::Change::Set(built.tuning_16et),
                        ..Default::default()
                    },
                })
                .expect("retune");
        }
        let mut compiler = Compiler::new(
            TempoMap::new([(Tick(0), bpm_to_usec_per_quarter(120.0))], RATE),
            vec![built.track],
        );
        let chunk = compiler
            .chunk(temp.project(), SampleTime(0), SampleTime(SPAN as u64))
            .expect("compile");
        let onsets: Vec<u64> = chunk.note.iter().map(|n| n.at.0).collect();

        let (mut app, rt) = session();
        let mut offline = Offline::new(Format::stereo(RATE, 480), rt);
        offline.engine().set_instrument(
            Instrument::new(Patch::harpington(), padlington(RATE), 16, RATE).expect("instrument"),
        );
        app.send(Command::now(What::TakeChunk(ChunkHandle::new(Chunk {
            from: chunk.from,
            to: chunk.to,
            note: chunk.note,
        }))))
        .expect("send");
        app.send(Command::now(What::Start)).expect("send");
        (onsets, offline.render(RATE as usize * 4))
    };

    let (twelve_onsets, twelve) = render(false);
    let (sixteen_onsets, sixteen) = render(true);

    assert_eq!(twelve_onsets, sixteen_onsets, "not one onset moved");
    assert_ne!(twelve, sixteen, "and it genuinely sounds different");
}
