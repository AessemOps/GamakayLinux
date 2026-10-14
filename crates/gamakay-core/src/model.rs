// SPDX-License-Identifier: GPL-3.0-or-later
//! High-level, transport-independent values returned to callers.

use crate::device::DeviceRecord;

/// How the board is attached. Derived from the USB/uhid node, not the
/// protocol: the handshake does not report it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    /// Wired USB (the LK67's `0461:4002`).
    Usb,
    /// The 2.4 GHz receiver (VID/PID unknown for the LK67; not yet tested).
    Receiver,
    /// Bluetooth HOGP, surfaced as hidraw through `uhid`.
    Bluetooth,
    Unknown,
}

impl Connection {
    pub const fn label(self) -> &'static str {
        match self {
            Connection::Usb => "USB",
            Connection::Receiver => "2.4 GHz",
            Connection::Bluetooth => "Bluetooth",
            Connection::Unknown => "unknown",
        }
    }
}

/// Everything `gamakay info` reports. Read-only, assembled from the handshake
/// and a couple of GET commands.
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    /// The numeric id from the `0x8F` handshake.
    pub id: u32,
    /// The matched table record, if the id is known.
    pub record: Option<&'static DeviceRecord>,
    /// The id is the documented mismatched-firmware LK67 (71).
    pub variant: bool,
    /// Firmware revision as returned by `0x80` (0x102 renders as `102`).
    pub revision: Option<u16>,
    /// Active profile index, zero based.
    pub profile: Option<u8>,
    /// How the board is attached.
    pub connection: Connection,
    /// A short description of the transport node.
    pub transport: String,
}

impl DeviceInfo {
    pub fn display_name(&self) -> &str {
        self.record.map(|r| r.display_name).unwrap_or("unknown")
    }

    /// The revision rendered the vendor way: each nibble is a decimal digit,
    /// so `0x102` reads as `102`.
    pub fn revision_string(&self) -> Option<String> {
        self.revision.map(|r| format!("{r:X}"))
    }
}
