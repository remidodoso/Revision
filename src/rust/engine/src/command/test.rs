use super::*;

#[test]
fn a_command_is_pod_and_small() {
    // It crosses a ring by value. If this grows, the ring gets more expensive
    // and something has been smuggled in that should have been a handle.
    assert!(
        size_of::<Command>() <= 32,
        "command grew to {} bytes",
        size_of::<Command>()
    );
    fn assert_copy<T: Copy>() {}
    assert_copy::<Command>();
}

#[test]
fn every_command_carries_a_time() {
    // The property that lets one channel serve both the live path and future
    // scheduling: a button press sends NOW, an arpeggiator sends a stamp.
    let live = Command::now(What::Start);
    assert!(live.at.is_now());

    let scheduled = Command::at(crate::time::SampleTime(48_000), What::Start);
    assert!(!scheduled.at.is_now());
}

#[test]
fn a_chunk_crosses_by_handle_and_comes_home() {
    let handle = ChunkHandle::new(Chunk {
        from: crate::time::SampleTime(0),
        to: crate::time::SampleTime(48_000),
    });

    // SAFETY: this side owns the handle, having just made it.
    let seen = unsafe { handle.get() };
    assert_eq!(seen.to, crate::time::SampleTime(48_000));

    // SAFETY: sole owner, released once. Under Miri or a leak checker, failing
    // to do this is what the return ring exists to prevent.
    unsafe { handle.release() };
}
