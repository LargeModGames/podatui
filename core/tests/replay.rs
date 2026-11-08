//! Replays the owner's Pro 3 probe captures (`tests/fixtures`, paired MAC scrubbed).

use podatui_core::fixture::{self, Event, Line};
use podatui_core::proximity;

const X2AC: &str = include_str!("fixtures/pro3-x2ac-case-and-buds.txt");
const X2D: &str = include_str!("fixtures/pro3-x2d-playback.txt");

fn lines(text: &str) -> Vec<Line> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| fixture::parse(l).unwrap_or_else(|| panic!("unparsed: {l}")))
        .collect()
}

fn is_status(data: &[u8]) -> bool {
    data.len() > 2 && data[1] == 0x19 && data[2] == 0x01
}

/// (last NOTE label, addr, advert) for every status frame.
fn status_frames(text: &str) -> Vec<(String, u64, proximity::Advert)> {
    let mut note = String::new();
    let mut out = Vec::new();
    for line in lines(text) {
        match line.event {
            Event::Note(label) => note = label,
            Event::Adv { addr, data, .. } if is_status(&data) => {
                out.push((note.clone(), addr, proximity::parse(&data).unwrap()));
            }
            _ => {}
        }
    }
    out
}

#[test]
fn every_line_round_trips() {
    for text in [X2AC, X2D] {
        for raw in text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
        {
            assert_eq!(fixture::parse(raw).unwrap().to_string(), raw);
        }
    }
}

#[test]
fn status_frames_parse_and_the_rest_are_known_rejects() {
    for text in [X2AC, X2D] {
        for line in lines(text) {
            if let Event::Adv { data, .. } = line.event {
                if is_status(&data) {
                    assert_eq!(proximity::parse(&data).unwrap().model, 0x2027);
                } else {
                    assert_eq!(&data[..2], &[0x07, 0x11]);
                    assert!(proximity::parse(&data).is_none());
                    assert!(proximity::parse_identity(&data).is_none());
                }
            }
        }
    }
}

#[test]
fn closed_case_keeps_advertising() {
    let closed = status_frames(X2AC)
        .into_iter()
        .filter(|(note, ..)| note == "closed case")
        .count();
    assert!(closed > 20, "{closed}");
}

/// Pro 3: after the closed case rotates its addresses the lid byte reads 0x11, which parses as
/// open. Lid edges cannot come from bit 3 alone.
#[test]
fn closed_case_lid_byte_reads_open_after_rotation() {
    let lids: Vec<proximity::Lid> = status_frames(X2AC)
        .into_iter()
        .filter(|(note, ..)| note == "closed case")
        .map(|(_, _, a)| a.lid)
        .collect();
    assert!(lids.contains(&proximity::Lid::Closed(2)));
    assert!(lids.contains(&proximity::Lid::Open(1)));
}

#[test]
fn only_one_address_advertises_once_worn() {
    let mut addrs: Vec<u64> = status_frames(X2AC)
        .into_iter()
        .filter(|(note, ..)| note == "done")
        .map(|(_, addr, _)| addr)
        .collect();
    addrs.dedup();
    assert_eq!(addrs.len(), 1);
}

#[test]
fn audio_byte_follows_pc_playback() {
    let mut audio: Vec<u8> = status_frames(X2D).iter().map(|(_, _, a)| a.audio).collect();
    audio.dedup();
    assert_eq!(audio, [0x04, 0x05, 0x04, 0x05]);
}
