//! Checked against the W3C formulas rather than against what the code does.

use super::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn nothing_scheduled_holds_the_initial_value() {
    let a = Automation::new(0.5);
    assert_eq!(a.value_at(0), 0.5);
    assert_eq!(a.value_at(1_000_000), 0.5);
}

#[test]
fn set_value_jumps_at_its_time_and_not_before() {
    let mut a = Automation::new(0.0);
    a.set_value_at_time(1.0, 100);

    assert_eq!(a.value_at(99), 0.0, "not a frame early");
    assert_eq!(a.value_at(100), 1.0);
    assert_eq!(a.value_at(10_000), 1.0, "and holds");
}

#[test]
fn a_linear_ramp_is_a_straight_line() {
    // v(t) = V0 + (V1 − V0)·(t − T0)/(T1 − T0)
    let mut a = Automation::new(0.0);
    a.set_value_at_time(0.0, 0);
    a.linear_ramp_to_value_at_time(1.0, 100);

    assert!(close(a.value_at(0), 0.0));
    assert!(close(a.value_at(25), 0.25));
    assert!(close(a.value_at(50), 0.5));
    assert!(close(a.value_at(100), 1.0), "arrives exactly");
    assert!(close(a.value_at(200), 1.0), "and stays");
}

#[test]
fn an_exponential_ramp_has_a_constant_ratio() {
    // v(t) = V0·(V1/V0)^((t−T0)/(T1−T0)). The half-way point of a ramp from 1 to
    // 4 is 2, not 2.5 — which is the whole reason to use it for a cutoff.
    let mut a = Automation::new(1.0);
    a.set_value_at_time(1.0, 0);
    a.exponential_ramp_to_value_at_time(4.0, 100);

    assert!(close(a.value_at(0), 1.0));
    assert!(close(a.value_at(50), 2.0), "got {}", a.value_at(50));
    assert!(close(a.value_at(100), 4.0));

    // Equal ratios over equal spans: the defining property.
    let quarter = a.value_at(25) / a.value_at(0);
    let half = a.value_at(50) / a.value_at(25);
    assert!(close(quarter, half), "{quarter} vs {half}");
}

#[test]
fn an_exponential_ramp_through_zero_holds_and_then_jumps() {
    // The specification's special case, easy to miss and audible when missed:
    // exponential interpolation through zero has no meaning, so the value holds
    // and then steps rather than producing a NaN.
    let mut a = Automation::new(0.0);
    a.set_value_at_time(0.0, 0);
    a.exponential_ramp_to_value_at_time(1.0, 100);

    assert_eq!(a.value_at(50), 0.0, "held, not interpolated");
    assert!(a.value_at(50).is_finite(), "and certainly not NaN");
    assert!(close(a.value_at(100), 1.0), "then arrives");

    // Same for opposite signs.
    let mut b = Automation::new(-1.0);
    b.set_value_at_time(-1.0, 0);
    b.exponential_ramp_to_value_at_time(1.0, 100);
    assert_eq!(b.value_at(50), -1.0);
    assert!(close(b.value_at(100), 1.0));
}

#[test]
fn set_target_approaches_without_arriving() {
    // v(t) = V1 + (V0 − V1)·e^(−t/τ). After one τ the remaining distance is 1/e.
    let mut a = Automation::new(1.0);
    a.set_target_at_time(0.0, 0, 100.0);

    assert!(close(a.value_at(0), 1.0));
    assert!(
        close(a.value_at(100), (-1.0f32).exp()),
        "one tau leaves 1/e: {}",
        a.value_at(100)
    );
    assert!(close(a.value_at(200), (-2.0f32).exp()));
    assert!(
        a.value_at(10_000) > 0.0,
        "asymptotic — it never truly arrives"
    );
    assert!(a.value_at(10_000) < 1e-6, "but gets arbitrarily close");
}

#[test]
fn a_zero_time_constant_jumps() {
    let mut a = Automation::new(1.0);
    a.set_target_at_time(0.0, 50, 0.0);
    assert_eq!(a.value_at(49), 1.0);
    assert_eq!(a.value_at(50), 0.0);
}

#[test]
fn a_target_is_superseded_by_the_next_event() {
    // The awkward case: a target's effect continues past its own time until
    // something else happens. Where it got to when the next event arrives is
    // what that event ramps *from*.
    let mut a = Automation::new(1.0);
    a.set_target_at_time(0.0, 0, 100.0);
    a.set_value_at_time(0.25, 200);

    assert!(close(a.value_at(100), (-1.0f32).exp()));
    assert_eq!(a.value_at(200), 0.25, "the set wins from its time");
    assert_eq!(a.value_at(400), 0.25, "and the target is over");
}

#[test]
fn a_ramp_starts_from_the_value_at_the_previous_events_time() {
    // A ramp's V0 is the value at T0, the previous event's time — so a ramp
    // after a *set* starts from what the set set.
    let mut a = Automation::new(0.0);
    a.set_value_at_time(0.2, 0);
    a.linear_ramp_to_value_at_time(1.0, 100);
    assert!(close(a.value_at(50), 0.6), "midway from 0.2 to 1.0");
}

#[test]
fn a_ramp_after_a_target_ignores_the_targets_progress() {
    // The surprising consequence of the rule above, asserted so that changing it
    // is a deliberate act. A target's value *at its own instant* is the value it
    // started from, so that is what the ramp ramps from — the approach in
    // between contributes nothing.
    //
    // Our reading of the specification on this corner is not verified, and it
    // does not arise in the voice being ported (attack ramp, decay target,
    // release target — no ramp ever follows a target). If a patch ever wants the
    // combination, check this first.
    let mut a = Automation::new(1.0);
    a.set_target_at_time(0.0, 0, 100.0);
    a.linear_ramp_to_value_at_time(1.0, 200);

    assert!(
        close(a.value_at(150), 1.0),
        "flat, because it ramps 1.0 -> 1.0: got {}",
        a.value_at(150)
    );
    assert!(close(a.value_at(200), 1.0));
}

#[test]
fn events_may_arrive_out_of_order() {
    // The specification permits it, and a voice scheduling a release before a
    // decay has finished being described would otherwise be a trap.
    let mut a = Automation::new(0.0);
    a.linear_ramp_to_value_at_time(1.0, 100);
    a.set_value_at_time(0.0, 0);

    assert!(close(a.value_at(50), 0.5), "sorted on insert");
}

#[test]
fn the_padlington_amplitude_envelope() {
    // The shape from the inventory: a *linear* attack (exponential from near
    // zero is inaudible and then snaps), a decay approaching sustain, and a
    // release approaching silence.
    let attack = 400u64;
    let mut a = Automation::new(0.0);
    a.set_value_at_time(0.0, 0);
    a.linear_ramp_to_value_at_time(1.0, attack);
    a.set_target_at_time(0.9, attack, 1_000.0);

    assert!(close(a.value_at(0), 0.0), "silent at the start");
    assert!(close(a.value_at(attack / 2), 0.5), "half way up, linearly");
    assert!(
        close(a.value_at(attack), 1.0),
        "peak at the end of the attack"
    );
    assert!(
        a.value_at(attack + 5_000) < 0.92 && a.value_at(attack + 5_000) > 0.895,
        "settling toward sustain: {}",
        a.value_at(attack + 5_000)
    );

    // Note-off: cancel what was scheduled ahead, then release from here.
    let off = attack + 5_000;
    let at_release = a.value_at(off);
    a.cancel_from(off);
    a.set_target_at_time(0.0, off, 2_000.0);

    assert!(
        close(a.value_at(off), at_release),
        "no discontinuity at note-off"
    );
    assert!(a.value_at(off + 10_000) < 0.01, "and it dies away");
}

#[test]
fn cancelling_removes_only_what_is_still_ahead() {
    let mut a = Automation::new(0.0);
    a.set_value_at_time(1.0, 0);
    a.set_value_at_time(2.0, 100);
    a.set_value_at_time(3.0, 200);

    a.cancel_from(150);
    assert_eq!(a.len(), 2);
    assert_eq!(a.value_at(500), 2.0, "the third never happens");
}

#[test]
fn overflow_is_counted_never_silent() {
    // A note-off schedules a release on the audio thread, where the list cannot
    // grow. A dropped release is an audible defect and must be attributable.
    let mut a = Automation::new(0.0);
    for n in 0..EVENT_CAPACITY {
        assert!(a.set_value_at_time(n as f32, n as u64 * 10));
    }
    assert!(!a.set_value_at_time(99.0, 1_000), "full");
    assert_eq!(a.lost(), 1);
    assert_eq!(a.len(), EVENT_CAPACITY, "and nothing was corrupted");
}

#[test]
fn a_reset_forgets_everything() {
    let mut a = Automation::new(0.0);
    a.set_value_at_time(1.0, 0);
    a.reset(0.25);
    assert!(a.is_empty());
    assert_eq!(a.value_at(1_000), 0.25);
}

#[test]
fn scheduling_and_evaluating_allocate_nothing() {
    // Note-off happens on the audio thread. If scheduling a release allocated,
    // every key release would be a deadline hazard.
    let mut a = Automation::new(0.0);
    let _rt = crate::guard::RtScope::enter();
    a.set_value_at_time(0.0, 0);
    a.linear_ramp_to_value_at_time(1.0, 400);
    a.set_target_at_time(0.9, 400, 1_000.0);
    a.cancel_from(5_000);
    a.set_target_at_time(0.0, 5_000, 2_000.0);
    let mut total = 0.0f32;
    for frame in 0..4_000 {
        total += a.value_at(frame);
    }
    assert!(total > 0.0);
}
