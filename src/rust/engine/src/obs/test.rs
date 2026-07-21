use super::*;

#[test]
fn every_code_renders_to_prose() {
    // Someone reads this log. A code that renders to a bare number, or panics,
    // is a defect — so every variant is exercised rather than trusted.
    let codes = [
        Code::TransportStart,
        Code::TransportStop,
        Code::Locate,
        Code::ToneOn,
        Code::ToneOff,
        Code::AllNotesOff,
        Code::Xrun,
        Code::ChunkTaken,
        Code::ChunkReleased,
        Code::PendingFull,
        Code::ObsDropped,
        Code::BlockTrace,
    ];
    for code in codes {
        let text = Obs::new(Creator::Stream, Level::Info, code)
            .arg0(440_000)
            .arg1(7)
            .render(48_000);
        assert!(!text.is_empty(), "{code:?} renders nothing");
    }
}

#[test]
fn frequency_survives_the_integer_crossing() {
    // Only integers cross the ring, so a frequency travels as millihertz. The
    // round trip has to be good enough to read.
    let text = Obs::new(Creator::Transport, Level::Info, Code::ToneOn)
        .arg0(440_000)
        .render(48_000);
    assert_eq!(text, "tone on: 440.000 Hz");
}

#[test]
fn creators_are_dotted_and_engine_scoped() {
    for creator in [
        Creator::Stream,
        Creator::Transport,
        Creator::Sched,
        Creator::Timing,
    ] {
        let name = creator.as_str();
        assert!(name.starts_with("engine."), "{name} is not engine-scoped");
    }
}
