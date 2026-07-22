//! rev-midi integration: the one thing that needs the library rather than the
//! types — a real `midir` open. It proves the callback signature and the port
//! API against `midir`, and it must pass on a machine with no MIDI at all.

/// Listing ports touches the real MIDI stack and must never panic, whether or
/// not a device is present. On CI there is none; the answer is an empty list,
/// not a failure.
#[test]
fn listing_ports_is_safe_with_or_without_a_device() {
    let ports = rev_midi::ports::list();
    // No assertion on the contents — the point is that the call into `midir`
    // returns rather than crashing. On the dev box with the Oxygen connected
    // this is non-empty; on CI it is empty.
    println!("MIDI input ports: {ports:?}");
}

/// Opening a port that does not exist fails cleanly rather than panicking — the
/// degrade-not-crash posture, before any hardware is involved.
#[test]
fn opening_a_nonexistent_port_is_an_error_not_a_panic() {
    use rev_engine::session_with_thru;
    use rev_midi::{Fork, NoteHz};
    use std::time::Instant;

    let (_app, _rt, thru) = session_with_thru();
    let (fork, _events) = Fork::new(thru, NoteHz::silent(), Instant::now());
    // Index far past any real port count.
    let result = rev_midi::ports::open(9999, fork);
    assert!(result.is_err(), "no such port should be a clean error");
}
