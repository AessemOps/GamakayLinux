// SPDX-License-Identifier: GPL-3.0-or-later
//! Wire-level errors. No external error crate: the core stays dependency-free.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The kernel/USB layer refused the report.
    Transport(String),
    /// No reply arrived within the retry budget.
    NoReply { opcode: u8 },
    /// A reply shorter than the fields the command needs.
    ShortReply { opcode: u8, len: usize },
    /// A reply whose opcode does not echo the request.
    UnexpectedReply { wanted: u8, got: u8 },
    /// The packet we built had the wrong checksum byte. A bug, not user error.
    ChecksumMismatch { expected: u8, got: u8 },
    /// A command the safety guard refuses to send (destructive or boot).
    Blocked { opcode: u8, reason: &'static str },
    /// A command that is not in the v1 allow-list.
    NotAllowed { opcode: u8 },
    /// A payload that does not fit a 64-byte report.
    PayloadTooLong { len: usize },
    /// The identified device is not in the table.
    UnknownDevice { id: u32 },
    /// The identified device is a known mismatched firmware id.
    VariantWarning { id: u32, expected: u32 },
    /// A feature the current transport does not carry (from the matrix).
    UnsupportedOnTransport(String),
    /// No matching hidraw node was found.
    DeviceNotFound,
    /// The value the caller passed is out of range for the field.
    OutOfRange { field: &'static str, value: i64 },
    /// Two full reads of a layer disagreed, so neither can be trusted. Writing
    /// an unconfirmed layer back would persist a mixture of the two keymaps.
    UnstableLayer { tries: u32 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport error: {e}"),
            Self::NoReply { opcode } => {
                write!(f, "no reply to command 0x{opcode:02X}")
            }
            Self::ShortReply { opcode, len } => {
                write!(f, "short reply to 0x{opcode:02X} ({len} bytes)")
            }
            Self::UnexpectedReply { wanted, got } => {
                write!(f, "reply 0x{got:02X} does not echo request 0x{wanted:02X}")
            }
            Self::ChecksumMismatch { expected, got } => {
                write!(f, "checksum byte 0x{got:02X} should be 0x{expected:02X}")
            }
            Self::Blocked { opcode, reason } => {
                write!(
                    f,
                    "refusing to send blocked command 0x{opcode:02X}: {reason}"
                )
            }
            Self::NotAllowed { opcode } => {
                write!(f, "command 0x{opcode:02X} is not in the allow-list")
            }
            Self::PayloadTooLong { len } => {
                write!(f, "payload of {len} bytes does not fit a 64-byte report")
            }
            Self::UnknownDevice { id } => write!(f, "unknown device id {id}"),
            Self::VariantWarning { id, expected } => write!(
                f,
                "device reports id {id} (expected {expected}): mismatched firmware"
            ),
            Self::UnsupportedOnTransport(m) => write!(f, "{m}"),
            Self::DeviceNotFound => write!(f, "no GamaKay hidraw device found"),
            Self::OutOfRange { field, value } => {
                write!(f, "{field} = {value} is out of range")
            }
            Self::UnstableLayer { tries } => write!(
                f,
                "the board returned a different keymap on each of {tries} reads; refusing to act on an unconfirmed layer (replug the keyboard and retry)"
            ),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
