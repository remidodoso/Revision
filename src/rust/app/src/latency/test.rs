use super::*;

fn position(block_frames: u32, sample_rate: u32, latency_out: u32) -> Position {
    Position {
        block_frames,
        sample_rate,
        latency_out,
        ..Position::default()
    }
}

#[test]
fn there_is_nothing_to_say_before_the_first_block() {
    assert!(Estimate::from(&position(0, 48_000, 0)).is_none());
    assert!(Estimate::from(&position(128, 0, 0)).is_none());
}

#[test]
fn the_floor_is_two_block_periods() {
    // 128 frames at 48 kHz is 2.667 ms a block; the floor is scheduling + output.
    let e = Estimate::from(&position(128, 48_000, 0)).expect("an estimate");
    assert!(
        (e.scheduling_ms - 2.667).abs() < 0.01,
        "{}",
        e.scheduling_ms
    );
    assert!((e.output_ms - 2.667).abs() < 0.01);
    assert!((e.floor_ms() - 5.333).abs() < 0.01, "{}", e.floor_ms());
}

#[test]
fn a_bigger_buffer_is_more_latency() {
    let small = Estimate::from(&position(128, 48_000, 0))
        .unwrap()
        .floor_ms();
    let large = Estimate::from(&position(512, 48_000, 0))
        .unwrap()
        .floor_ms();
    assert!(large > small * 3.9, "512 vs 128: {large} vs {small}");
}

#[test]
fn a_reported_device_latency_is_added() {
    // When a device does report its output latency, it joins the floor.
    let e = Estimate::from(&position(128, 48_000, 240)).unwrap();
    assert!(
        (e.device_ms - 5.0).abs() < 0.01,
        "240 frames = 5 ms: {}",
        e.device_ms
    );
    assert!(e.floor_ms() > 10.0);
}

#[test]
fn the_summary_says_at_least_and_admits_the_unmeasured() {
    let e = Estimate::from(&position(480, 48_000, 0)).unwrap();
    let s = e.summary(480, 48_000);
    assert!(s.contains("≥"), "an honest floor: {s}");
    assert!(s.contains("unmeasured"), "admits what it cannot see: {s}");
    assert!(s.contains("480 frames"), "shows the block: {s}");
}
