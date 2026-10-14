// SPDX-License-Identifier: GPL-3.0-or-later
//! 64-byte HID feature reports, report ID 0.

use crate::error::{Error, Result};

/// A HID feature report is 64 data bytes. The hidraw ioctl carries one extra
/// leading report-ID byte (0), so the device sees 65.
pub const REPORT_LEN: usize = 64;

/// Checksum layout. Bit7 covers bytes 0..=6 and stores at byte 7; Bit8 covers
/// 0..=7 and stores at byte 8 (used only by LEDPARAM/SLEDPARAM). None is the
/// USB boot path, which we never use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checksum {
    Bit7,
    Bit8,
    None,
}

impl Checksum {
    /// Recompute the checksum byte the packet must carry.
    pub fn compute(self, buf: &[u8; REPORT_LEN]) -> Option<u8> {
        match self {
            Checksum::Bit7 => Some(sum_complement(&buf[..7])),
            Checksum::Bit8 => Some(sum_complement(&buf[..8])),
            Checksum::None => None,
        }
    }

    /// The byte position the checksum lives at, if any.
    pub fn at(self) -> Option<usize> {
        match self {
            Checksum::Bit7 => Some(7),
            Checksum::Bit8 => Some(8),
            Checksum::None => None,
        }
    }
}

fn sum_complement(bytes: &[u8]) -> u8 {
    let sum: u32 = bytes.iter().map(|&b| u32::from(b)).sum();
    0xFF - (sum & 0xFF) as u8
}

/// Write the checksum byte in place. Idempotent.
pub fn apply_checksum(buf: &mut [u8; REPORT_LEN], mode: Checksum) {
    if let (Some(at), Some(ck)) = (mode.at(), mode.compute(buf)) {
        buf[at] = ck;
    }
}

/// Build a zero-padded report from an opcode and payload, then checksum it.
pub fn packet(opcode: u8, payload: &[u8], mode: Checksum) -> Result<[u8; REPORT_LEN]> {
    if payload.len() > REPORT_LEN - 1 {
        return Err(Error::PayloadTooLong { len: payload.len() });
    }
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = opcode;
    buf[1..1 + payload.len()].copy_from_slice(payload);
    apply_checksum(&mut buf, mode);
    Ok(buf)
}

/// Verify a fully-built packet against the checksum mode it claims.
pub fn verify_checksum(buf: &[u8; REPORT_LEN], mode: Checksum) -> Result<()> {
    if let (Some(at), Some(expected)) = (mode.at(), mode.compute(buf)) {
        if buf[at] != expected {
            return Err(Error::ChecksumMismatch {
                expected,
                got: buf[at],
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit7_checksum_is_complement_of_low_byte_sum() {
        let buf = packet(0x8F, &[], Checksum::Bit7).unwrap();
        // 0xFF - 0x8F = 0x70
        assert_eq!(buf[0], 0x8F);
        assert_eq!(buf[7], 0x70);
        verify_checksum(&buf, Checksum::Bit7).unwrap();
    }

    #[test]
    fn bit8_covers_the_seventh_byte() {
        // payload byte at index 1 must be included when computing byte 8.
        let buf = packet(0x07, &[4, 3, 4, 0x87, 0xFF, 0x00, 0x00], Checksum::Bit8).unwrap();
        verify_checksum(&buf, Checksum::Bit8).unwrap();
        let sum: u32 = buf[..8].iter().map(|&b| u32::from(b)).sum();
        assert_eq!(buf[8], 0xFF - (sum & 0xFF) as u8);
    }

    #[test]
    fn oversized_payload_is_rejected() {
        let err = packet(0x01, &[0u8; 64], Checksum::Bit7).unwrap_err();
        assert!(matches!(err, Error::PayloadTooLong { len: 64 }));
    }
}
