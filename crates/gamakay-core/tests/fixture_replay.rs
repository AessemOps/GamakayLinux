// SPDX-License-Identifier: GPL-3.0-or-later
//! Replay the recorded LK67 read replies through the real conversation.
//!
//! This is the fixture half of the Phase 1 gate: the request builders and the
//! decoders are exercised against this board's bytes with no hardware
//! attached. The fixtures are real captures (see their header).

use gamakay_core::cmd;
use gamakay_core::guard::Guard;
use gamakay_core::packet::REPORT_LEN;
use gamakay_core::transport::{MockTransport, Session};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/lk67-usb-reads.txt"
);

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
        let mut buf = [0u8; REPORT_LEN];
        for (i, b) in bytes_hex.split_whitespace().enumerate() {
            buf[i] = u8::from_str_radix(b, 16).expect("byte hex");
        }
        out.push((op, buf));
    }
    out
}

/// The firmware does not recompute a reply checksum. Every GET reply echoes
/// the opcode and leaves byte 7 holding the request's checksum byte; the LED
/// reply is the exception, because its RGB data runs through byte 7.
#[test]
fn get_replies_echo_and_leave_the_request_checksum_byte() {
    for (op, buf) in load() {
        assert_eq!(buf[0], op, "byte 0 must echo the opcode");
        if op == cmd::GET_LEDPARAM {
            assert_eq!(buf[8], 0, "LED reply byte 8 is unused on GET");
        } else {
            assert_eq!(
                buf[7],
                0xFF - op,
                "byte 7 should hold the request checksum for 0x{op:02X}"
            );
        }
    }
}

#[test]
fn identify_revision_and_settings_decode() {
    let mut mock = MockTransport::new();
    for (op, buf) in load() {
        mock.expect(op, buf);
    }
    let guard = Guard::new(false);
    let mut session = Session::new(&mut mock, guard).with_settle(0);

    assert_eq!(session.device_id().unwrap(), 149);
    assert_eq!(session.revision().unwrap(), 0x205);
    assert_eq!(session.profile().unwrap(), 0);

    let led = session.led_param().unwrap();
    assert_eq!(led.mode, 1);
    assert_eq!(led.speed, 1); // wire 4 -> host scale 1
    assert_eq!(led.brightness, 4); // stored 5 (blank) clamps to 4
    assert_eq!(led.option, 0);
    assert_eq!(led.flags, 7); // fixed colour
    assert_eq!(led.rgb, [0x95, 0x07, 0xE1]);
    assert_eq!(led.preset, None);

    let opts = session.options().unwrap();
    assert_eq!(opts.raw, 0);
    assert!(!opts.wasd_swap && !opts.mac_mode && !opts.keyboard_lock);
    assert_eq!(opts.fn_matrix, 0);
    assert_eq!(opts.power_save, 0);

    assert_eq!(session.debounce().unwrap(), 0);

    let sleep = session.sleep_timers().unwrap();
    assert_eq!(sleep.sleep_bt, 0);
    assert_eq!(sleep.sleep_24g, 0);
    assert_eq!(sleep.deep_bt, 0);
    assert_eq!(sleep.deep_24g, 0);

    assert!(!session.auto_os().unwrap());
}

#[test]
fn every_read_command_stays_in_the_allow_list() {
    for (op, _) in load() {
        assert!(
            Guard::allowed(op),
            "read command 0x{op:02X} is missing from the allow-list"
        );
        assert!(Guard::blocked(op).is_none());
    }
}
