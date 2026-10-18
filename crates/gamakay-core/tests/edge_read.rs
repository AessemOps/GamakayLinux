// SPDX-License-Identifier: GPL-3.0-or-later
//! Replay the captured underglow (`0x88`) replies through the real read.
//!
//! The LK67 has a bottom-mesh and side-LED underglow, and its firmware answers
//! `0x88` with the stored parameter. That reply is what makes the feature
//! readable at all — the vendor software offers no control for these LEDs,
//! which is why the board looked like it had none — so the capture is pinned
//! here and replayed through `Session::edge_param`, exercising the request
//! alongside the decode.

use gamakay_core::cmd;
use gamakay_core::guard::Guard;
use gamakay_core::light::LedState;
use gamakay_core::packet::REPORT_LEN;
use gamakay_core::transport::{MockTransport, Session};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/lk67-edge-light.txt"
);

/// The opcode column is read, not decoration: a line labelled with the wrong
/// opcode has to fail here rather than pass unnoticed.
fn load() -> Vec<(u8, [u8; REPORT_LEN])> {
    let text = std::fs::read_to_string(FIXTURE).expect("fixture is readable");
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (op_hex, bytes_hex) = line.split_once(':').expect("opcode: bytes");
        let op = u8::from_str_radix(op_hex.trim(), 16).expect("opcode hex");
        assert_eq!(op, cmd::GET_SLEDPARAM, "these are underglow reads");
        let mut buf = [0u8; REPORT_LEN];
        for (i, b) in bytes_hex.split_whitespace().enumerate() {
            buf[i] = u8::from_str_radix(b, 16).expect("byte hex");
        }
        out.push((op, buf));
    }
    out
}

/// Decode a captured state through the read the tools use, so the request path
/// goes through the guard and `MockTransport` too, not just the field parsing.
fn decode(op: u8, buf: &[u8; REPORT_LEN]) -> LedState {
    let mut mock = MockTransport::new();
    mock.expect(op, *buf);
    let mut session = Session::new(&mut mock, Guard::new(false)).with_settle(0);
    session.edge_param().expect("the edge reply decodes")
}

/// Every state written in that session comes back, field for field. A write
/// that round-trips is what lets the underglow be treated as controllable.
#[test]
fn edge_replies_round_trip_the_written_parameter() {
    let states = load();
    assert_eq!(states.len(), 4, "four captured states");
    let decoded: Vec<LedState> = states.iter().map(|(op, buf)| decode(*op, buf)).collect();

    let solid = &decoded[0];
    assert_eq!(solid.mode, 1);
    assert_eq!(solid.speed, 2, "edge speed is not inverted");
    assert_eq!(solid.brightness, 4);
    assert_eq!(solid.flags, 7, "fixed colour");
    assert_eq!(solid.rgb, [0x12, 0x34, 0x56]);

    let off = &decoded[1];
    assert_eq!(off.mode, 0, "mode 0 is the underglow's off switch");
    assert_eq!(
        off.rgb,
        [0x12, 0x34, 0x56],
        "the rest survives the off write"
    );

    let breathing = &decoded[2];
    assert_eq!(breathing.mode, 2);
    assert_eq!(breathing.speed, 3);
    assert_eq!(breathing.rgb, [0xff, 0x00, 0xff]);

    let cycling = &decoded[3];
    assert_eq!(cycling.mode, 3);
    assert_eq!(cycling.flags, 8);
    assert_eq!(cycling.rgb, [0xff, 0x00, 0xb6]);
}

/// The two light replies share a layout but not an opcode, and decoding one as
/// the other would silently mis-apply a colour to the wrong LEDs.
#[test]
fn the_two_light_replies_do_not_decode_as_each_other() {
    let (op, buf) = load()[0];
    assert!(
        LedState::from_reply(&buf).is_none(),
        "an edge reply is not a backlight reply"
    );
    assert!(LedState::from_edge_reply(&buf).is_some());
    assert_eq!(
        decode(op, &buf).mode,
        1,
        "the replayed read is the edge one"
    );
}
