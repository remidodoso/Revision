use super::*;

#[test]
fn note_on_and_off_parse() {
    assert_eq!(
        Message::parse(&[0x90, 60, 100]),
        Some(Message::NoteOn {
            channel: 0,
            note: NoteNumber(60),
            velocity: 100
        })
    );
    assert_eq!(
        Message::parse(&[0x85, 64, 0]),
        Some(Message::NoteOff {
            channel: 5,
            note: NoteNumber(64)
        })
    );
}

#[test]
fn velocity_zero_note_on_is_a_note_off() {
    assert_eq!(
        Message::parse(&[0x90, 60, 0]),
        Some(Message::NoteOff {
            channel: 0,
            note: NoteNumber(60)
        })
    );
}

#[test]
fn other_messages_are_none_for_now() {
    assert_eq!(Message::parse(&[0xB0, 7, 100]), None, "CC is midi-02");
    assert_eq!(Message::parse(&[0xE0, 0, 64]), None, "pitch-bend is midi-02");
    assert_eq!(Message::parse(&[0x90]), None, "truncated");
    assert_eq!(Message::parse(&[]), None, "empty");
}

#[test]
fn the_key_is_channel_and_note_and_pairs() {
    let on = Message::parse(&[0x93, 60, 100]).unwrap();
    let off = Message::parse(&[0x83, 60, 0]).unwrap();
    assert_eq!(on.key(), off.key(), "same channel+note pairs");
    let other_channel = Message::parse(&[0x94, 60, 100]).unwrap();
    assert_ne!(on.key(), other_channel.key(), "channel is part of identity");
}

#[test]
fn level_is_velocity_over_127() {
    assert_eq!(level_of(0), 0.0);
    assert!((level_of(127) - 1.0).abs() < 1e-6);
    assert!((level_of(64) - 64.0 / 127.0).abs() < 1e-6);
}
