use super::*;

fn v(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_first_poll_is_all_arrivals() {
    let mut d = Devices::new();
    let change = d.update(v(&["Oxygen Pro Mini", "AG06"]));
    assert_eq!(change.arrived, v(&["Oxygen Pro Mini", "AG06"]));
    assert!(change.removed.is_empty());
    assert_eq!(d.ports().len(), 2);
}

#[test]
fn an_unchanged_list_reports_nothing() {
    let mut d = Devices::new();
    d.update(v(&["Oxygen"]));
    let change = d.update(v(&["Oxygen"]));
    assert!(change.is_empty(), "nothing changed: {change:?}");
}

#[test]
fn a_plugged_in_device_arrives() {
    let mut d = Devices::new();
    d.update(v(&["Oxygen"]));
    let change = d.update(v(&["Oxygen", "MIDIPLUS 4x4"]));
    assert_eq!(change.arrived, v(&["MIDIPLUS 4x4"]));
    assert!(change.removed.is_empty());
}

#[test]
fn an_unplugged_device_is_removed() {
    let mut d = Devices::new();
    d.update(v(&["Oxygen", "AG06"]));
    let change = d.update(v(&["AG06"]));
    assert!(change.arrived.is_empty());
    assert_eq!(change.removed, v(&["Oxygen"]));
}

#[test]
fn arrival_and_removal_in_one_poll() {
    let mut d = Devices::new();
    d.update(v(&["Oxygen"]));
    let change = d.update(v(&["MIDIPLUS 4x4"]));
    assert_eq!(change.arrived, v(&["MIDIPLUS 4x4"]));
    assert_eq!(change.removed, v(&["Oxygen"]));
}

#[test]
fn a_remembered_device_rebinds_when_it_returns() {
    // R-602: a device referenced by name is found again by index when it comes
    // back, which is what makes reconnection automatic.
    let mut d = Devices::new();
    d.update(v(&["AG06"]));
    assert!(d.index_of("Oxygen").is_none(), "gone: no index");
    d.update(v(&["AG06", "Oxygen"]));
    assert_eq!(d.index_of("Oxygen"), Some(1), "returned: found again");
}
