// SPDX-License-Identifier: GPL-3.0-or-later
//! High-level, transport-independent values returned to callers.

/// How the board is attached. Derived from the USB node, not the protocol: the
/// handshake does not report it.
///
/// **Scope: the USB cable only.** 2.4 GHz and Bluetooth were investigated and
/// dropped — the receiver declares no HID feature report, so the protocol cannot
/// be addressed over it, and Bluetooth was never captured. See
/// `docs/protocol.md`, "Scope: USB cable".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    /// Wired USB (the LK67's `3151:4003`, `0461:4002` before the 2026-10 update).
    Usb,
    /// A node that is not on a USB bus.
    Unknown,
}

impl Connection {
    pub const fn label(self) -> &'static str {
        match self {
            Connection::Usb => "USB",
            Connection::Unknown => "unknown",
        }
    }
}
