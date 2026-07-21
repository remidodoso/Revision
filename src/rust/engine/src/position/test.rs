use super::*;

#[test]
fn a_reader_sees_the_newest_whole_value() {
    let cell = PositionCell::new();
    assert_eq!(cell.read(), Position::default());

    for n in 1..=100u64 {
        cell.publish(Position {
            at: SampleTime(n * 480),
            block: n,
            ..Position::default()
        });
    }
    let seen = cell.read();
    assert_eq!(seen.block, 100, "latest wins, no queueing");
    assert_eq!(seen.at, SampleTime(48_000));
}

#[test]
fn concurrent_reads_never_tear() {
    // The property that makes a seqlock the right structure here: a reader can
    // run flat out against a writer and never observe a half-written value.
    // Every published Position is internally consistent (block * 480 == at), so
    // a torn read would show up as a violated invariant.
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let cell = Arc::new(PositionCell::new());
    let stop = Arc::new(AtomicBool::new(false));

    let writer = {
        let cell = Arc::clone(&cell);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                n += 1;
                cell.publish(Position {
                    at: SampleTime(n * 480),
                    block: n,
                    play: SampleTime(n),
                    ..Position::default()
                });
            }
            n
        })
    };

    let mut reads = 0u64;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
    while std::time::Instant::now() < deadline {
        let seen = cell.read();
        assert_eq!(
            seen.at.0,
            seen.block * 480,
            "torn read: at and block disagree"
        );
        assert_eq!(
            seen.play.0, seen.block,
            "torn read: play and block disagree"
        );
        reads += 1;
    }
    stop.store(true, Ordering::Relaxed);
    let writes = writer.join().expect("writer");

    assert!(reads > 100, "the test should actually have read: {reads}");
    assert!(
        writes > 100,
        "the test should actually have written: {writes}"
    );
}
