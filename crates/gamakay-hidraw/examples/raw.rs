// SPDX-License-Identifier: GPL-3.0-or-later
//! Dump the raw 64-byte reply to one GET command. A Phase 0 / drill helper.
//!
//! ```text
//! cargo run -p gamakay-hidraw --example raw -- /dev/hidraw0 0x92
//! ```
//!
//! Reads only; every packet still goes through the safety guard.

use gamakay_core::guard::Guard;
use gamakay_core::packet::Checksum;
use gamakay_core::transport::Session;
use gamakay_hidraw::HidrawDevice;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "/dev/hidraw0".into());
    let opcode = args
        .next()
        .map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).expect("opcode hex"))
        .unwrap_or(0x8F);
    // Every app-mode GET request uses a Bit7 checksum; only the LED *SET*
    // commands use Bit8, and this example only sends GETs.
    let mode = Checksum::Bit7;

    let mut dev = HidrawDevice::open(&path).expect("open hidraw node");
    let mut session = Session::new(&mut dev, Guard::new(true));
    let reply = session
        .roundtrip(opcode, &[], mode)
        .unwrap_or_else(|e| panic!("0x{opcode:02X}: {e}"));
    let hex: Vec<String> = reply.iter().map(|b| format!("{b:02x}")).collect();
    println!("0x{opcode:02X} -> {}", hex.join(" "));
}
