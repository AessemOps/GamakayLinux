// SPDX-License-Identifier: GPL-3.0-or-later
//! Decode the captured underglow (`0x88`) replies.
//!
//! The LK67 has a bottom-mesh and side-LED underglow, and its firmware answers
//! `0x88` with the stored parameter. That reply is what makes the feature
//! readable at all, so the capture is pinned here as a hardware fixture: the
//! vendor software offers no control for these LEDs, which is why the board
//! looked like it had none.

use gamakay_core::light::LedState;
use gamakay_core::packet::REPORT_LEN;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/lk67-edge-light.txt"
);

fn replies() -> Vec<[u8; REPORT_LEN]> {
    let text = std::fs::read_to_string(FIXTURE).expect("fixture is readable");
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (_op_hex, bytes_hex) = line.split_once(':').expect("opcode: bytes");
        let mut buf = [0u8; REPORT_LEN];
        for (i, b) in bytes_hex.split_whitespace().enumerate() {
            buf[i] = u8::from_str_radix(b, 16).expect("byte hex");
        }
        out.push(buf);
    }
    out
}

/// Every state written in that session comes back, field for field. A write
/// that round-trips is the gate for treating the underglow as controllable.
#[test]
fn edge_replies_round_trip_the_written_parameter() {
    let r = replies();
    assert_eq!(r.len(), 4, "four captured states");

    let solid = LedState::from_edge_reply(&r[0]).expect("static decodes");
    assert_eq!(solid.mode, 1);
    assert_eq!(solid.speed, 2, "edge speed is not inverted");
    assert_eq!(solid.brightness, 4);
    assert_eq!(solid.flags, 7, "fixed colour");
    assert_eq!(solid.rgb, [0x12, 0x34, 0x56]);

    let off = LedState::from_edge_reply(&r[1]).expect("off decodes");
    assert_eq!(off.mode, 0, "mode 0 is the underglow's off switch");
    assert_eq!(
        off.rgb,
        [0x12, 0x34, 0x56],
        "the rest survives the off write"
    );

    let breathing = LedState::from_edge_reply(&r[2]).expect("breathing decodes");
    assert_eq!(breathing.mode, 2);
    assert_eq!(breathing.speed, 3);
    assert_eq!(breathing.rgb, [0xff, 0x00, 0xff]);

    let cycling = LedState::from_edge_reply(&r[3]).expect("mode 3 decodes");
    assert_eq!(cycling.mode, 3);
    assert_eq!(cycling.flags, 8);
    assert_eq!(cycling.rgb, [0xff, 0x00, 0xb6]);
}

/// The two light replies share a layout but not an opcode, and decoding one as
/// the other would silently mis-apply a colour to the wrong LEDs.
#[test]
fn the_two_light_replies_do_not_decode_as_each_other() {
    let r = replies();
    assert!(
        LedState::from_reply(&r[0]).is_none(),
        "an edge reply is not a backlight reply"
    );
    assert!(LedState::from_edge_reply(&r[0]).is_some());
}
