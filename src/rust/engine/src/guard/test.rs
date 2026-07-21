use super::*;

#[test]
fn the_scope_arms_and_disarms() {
    assert!(!is_realtime());
    {
        let _rt = RtScope::enter();
        assert_eq!(is_realtime(), cfg!(debug_assertions));
    }
    assert!(!is_realtime());
}

#[test]
fn it_disarms_when_the_scope_unwinds() {
    // A guard left armed after a panic would poison every later test on this
    // thread, and — worse in the field — every later callback.
    let result = std::panic::catch_unwind(|| {
        let _rt = RtScope::enter();
        panic!("something went wrong inside the callback");
    });
    assert!(result.is_err());
    assert!(!is_realtime(), "the flag must not survive the unwind");
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "allocation on the real-time thread")]
fn allocating_while_armed_panics() {
    let _rt = RtScope::enter();
    // The whole point: this is the mistake the guard exists to catch, and it
    // must be caught loudly rather than measured later as a mysterious xrun.
    // `black_box` because an allocation whose result is unused is one the
    // optimizer is entitled to delete — and then the test would prove nothing.
    let held: Vec<u8> = Vec::with_capacity(1024);
    std::hint::black_box(held);
}

#[test]
fn the_flag_is_per_thread() {
    // The audio thread is armed; nothing else is. A global flag would make
    // every other thread's ordinary work a panic.
    //
    // Note the shape: the child arms itself and the parent checks that it was
    // unaffected. Arming the parent and *then* spawning would trip the guard on
    // the spawn, which allocates — itself a small demonstration that the flag
    // means what it says.
    let armed_inside = std::thread::spawn(|| {
        let _rt = RtScope::enter();
        is_realtime()
    })
    .join()
    .expect("thread");

    assert_eq!(armed_inside, cfg!(debug_assertions));
    assert!(!is_realtime(), "the parent thread never armed");
}

#[test]
#[cfg(debug_assertions)]
fn a_panic_inside_an_armed_scope_reports_itself() {
    // Unwinding allocates. If the guard fired on that, a double panic would
    // abort the process with no message — so a genuine bug in a voice would
    // present as a silent crash instead of a backtrace pointing at it.
    let result = std::panic::catch_unwind(|| {
        let _rt = RtScope::enter();
        // Indexed through a slice so the compiler cannot fold it into a
        // compile-time error: the point is a panic at run time.
        let empty: &[u8] = &[];
        let index = std::hint::black_box(1usize);
        let _ = empty[index];
    });
    let panic = result.expect_err("indexing past the end panics");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or_default();
    assert!(
        message.contains("index"),
        "the original panic must survive, not the guard's: {message:?}"
    );
}
