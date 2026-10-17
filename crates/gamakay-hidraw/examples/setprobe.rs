// SPDX-License-Identifier: GPL-3.0-or-later
//! Writes unless SETPROBE_ALLOW_WRITE is set: this example can send the
//! factory reset, so it is read-only by default.
//!
//! Diagnostic: identify, send one SET, then drain reads and read back.
//!
//! ```text
//! cargo run -p gamakay-hidraw --example setprobe -- /dev/hidraw0 07 010405079507e1
//! ```
//!
//! Used to characterise how the LK67 answers a feature SET (it applies the
//! value but can time the control transfer out). Prints timings.

use std::time::{Duration, Instant};

use gamakay_core::cmd;
use gamakay_core::guard::Guard;
use gamakay_core::packet::{Checksum, REPORT_LEN};
use gamakay_core::transport::Session;
use gamakay_hidraw::HidrawDevice;

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "/dev/hidraw0".into());
    let opcode = args
        .next()
        .map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).expect("opcode hex"));
    let payload: Vec<u8> = args
        .next()
        .map(|s| {
            s.split_whitespace()
                .flat_map(|w| w.as_bytes().chunks(2))
                .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
                .collect()
        })
        .unwrap_or_default();

    let mut dev = HidrawDevice::open(&path).expect("open");
    let mut s = Session::new(
        &mut dev,
        Guard::new(!std::env::var("SETPROBE_ALLOW_WRITE").is_ok()),
    )
    .with_settle(0);

    // Identify first.
    match s.roundtrip(cmd::GET_USB_VERSION, &[], Checksum::Bit7) {
        Ok(r) => println!(
            "identify ok, id={}",
            u32::from_le_bytes([r[1], r[2], r[3], r[4]])
        ),
        Err(e) => println!("identify failed: {e}"),
    }

    if let Some(op) = opcode {
        let mode = if op == 0x07 || op == 0x08 {
            Checksum::Bit8
        } else {
            Checksum::Bit7
        };
        let t = Instant::now();
        let res = s.send(op, &payload, mode);
        println!("SET 0x{op:02X} -> {:?} in {:?}", res, t.elapsed());
    }

    // With SETPROBE_NODRAIN set, do nothing after the SET (fire-and-forget,
    // as WebHID sendFeatureReport would).
    if std::env::var("SETPROBE_NODRAIN").is_ok() {
        println!("(no drain, no read-back)");
        return;
    }

    // Drain a few feature reads to see if an ack is waiting.
    for i in 0..3 {
        let mut buf = [0u8; REPORT_LEN];
        let t = Instant::now();
        match s.read(&mut buf) {
            Ok(n) => println!(
                "drain[{i}] ok {n} bytes {:?}: {}",
                t.elapsed(),
                hex(&buf[..8])
            ),
            Err(e) => println!("drain[{i}] failed in {:?}: {e}", t.elapsed()),
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    // Read the current LED state.
    match s.led_param() {
        Ok(l) => println!(
            "led: mode {} speed {} brightness {} flags {} #{:02x}{:02x}{:02x}",
            l.mode, l.speed, l.brightness, l.flags, l.rgb[0], l.rgb[1], l.rgb[2]
        ),
        Err(e) => println!("led read failed: {e}"),
    }
}
