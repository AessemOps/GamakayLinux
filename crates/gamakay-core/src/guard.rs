// SPDX-License-Identifier: GPL-3.0-or-later
//! The safety guard. Every packet leaves through here.
//!
//! Two independent rules:
//! 1. A hard **block-list** of destructive and bootloader opcodes. These are
//!    refused by name and never travel, whatever the caller wants.
//! 2. A v1 **allow-list**: only commands documented in `docs/protocol.md` are
//!    permitted. Anything else is a bug or an unreviewed guess.
//!
//! `dry_run` is a property of the call site: the guard still validates, it
//! just reports the packet instead of sending it.

use crate::cmd;
use crate::error::{Error, Result};
use crate::packet::{Checksum, REPORT_LEN, packet, verify_checksum};

/// Opcodes that must never be sent by this tool, with the reason.
const BLOCKED: &[(u8, &str)] = &[
    (cmd::FLASH_CHIP_ERASE, "erases the flash chip (~55 s)"),
    (cmd::DISPLAY_ERASE, "display erase"),
    (cmd::USB_BOOT, "enters the USB bootloader"),
    (cmd::OLED_BOOT, "enters the display-chip bootloader"),
    (
        cmd::CALIBRATION_ON,
        "sensor calibration: zeroes live travel tables",
    ),
    (
        cmd::CALIBRATION_OFF,
        "sensor calibration: commits travel tables to flash",
    ),
    (cmd::RF_BOOT, "enters the RF bootloader"),
    (cmd::BOOT_INIT, "boot-mode image transfer"),
];

/// The v1 command set. Reads are always safe; writes are the four feature
/// groups (lighting, keymap, macros, settings) that the guard has reviewed.
const ALLOWED: &[u8] = &[
    // reads / identify
    cmd::GET_USB_VERSION,
    cmd::GET_REVISION,
    cmd::GET_LEDPARAM,
    cmd::GET_SLEDPARAM,
    cmd::GET_PROFILE,
    cmd::GET_KBOPTION,
    cmd::GET_KEYMATRIX,
    cmd::GET_FN,
    cmd::GET_DEBOUNCE,
    cmd::GET_SLEEPTIME,
    cmd::GET_MACRO,
    cmd::GET_USERPIC,
    cmd::GET_AUTO_OS,
    // writes
    cmd::SET_LEDPARAM,
    cmd::SET_SLEDPARAM,
    cmd::SET_PROFILE,
    cmd::SET_KBOPTION,
    cmd::SET_KEYMATRIX,
    cmd::SET_KEY_ONE,
    cmd::SET_FN,
    cmd::SET_FN_ONE,
    cmd::SET_DEBOUNCE,
    cmd::SET_SLEEPTIME,
    cmd::SET_MACRO,
    cmd::SET_USERPIC,
    cmd::SET_AUTO_OS,
    cmd::SET_RESET,
];

/// A guard with a dry-run flag. Cheap to copy.
#[derive(Clone, Copy, Debug, Default)]
pub struct Guard {
    pub dry_run: bool,
}

impl Guard {
    pub const fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    /// A block-list lookup, independent of the allow-list.
    pub fn blocked(opcode: u8) -> Option<&'static str> {
        BLOCKED
            .iter()
            .find(|(op, _)| *op == opcode)
            .map(|(_, reason)| *reason)
    }

    pub fn allowed(opcode: u8) -> bool {
        ALLOWED.contains(&opcode)
    }

    /// Check the opcode against both lists, build the packet and confirm its
    /// checksum. This is the only path that produces a packet for the wire.
    pub fn build(&self, opcode: u8, payload: &[u8], mode: Checksum) -> Result<[u8; REPORT_LEN]> {
        if let Some(reason) = Self::blocked(opcode) {
            return Err(Error::Blocked { opcode, reason });
        }
        if !Self::allowed(opcode) {
            return Err(Error::NotAllowed { opcode });
        }
        let buf = packet(opcode, payload, mode)?;
        verify_checksum(&buf, mode)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_opcodes_are_refused_by_name() {
        for (op, _) in BLOCKED {
            let err = Guard::new(false)
                .build(*op, &[], Checksum::Bit7)
                .unwrap_err();
            assert!(matches!(err, Error::Blocked { opcode, .. } if opcode == *op));
        }
    }

    #[test]
    fn unknown_opcodes_are_refused() {
        let err = Guard::new(false)
            .build(0x42, &[], Checksum::Bit7)
            .unwrap_err();
        assert!(matches!(err, Error::NotAllowed { opcode: 0x42 }));
    }

    #[test]
    fn identify_passes_and_checksums() {
        let buf = Guard::new(false)
            .build(cmd::GET_USB_VERSION, &[], Checksum::Bit7)
            .unwrap();
        assert_eq!(buf[0], 0x8F);
        verify_checksum(&buf, Checksum::Bit7).unwrap();
    }
}
