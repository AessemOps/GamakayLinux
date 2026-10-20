// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux hidraw backend for GamaKay keyboards.
//!
//! The board is driven through `/dev/hidraw*` with `HIDIOCSFEATURE` /
//! `HIDIOCGFEATURE`. There is deliberately no libusb and no libudev: the
//! kernel's `hid-generic` driver stays bound, so the keyboard keeps typing
//! while we configure it, and there is no extra dependency to keep current.
//!
//! Enumeration walks `/sys/class/hidraw/*/device/uevent` for the USB ids and
//! parses the report descriptor to find the node that carries the 64-byte
//! vendor feature report. Scope is the USB cable: the 2.4 GHz receiver declares
//! no feature report and Bluetooth was never captured, so neither is supported
//! (see `docs/protocol.md`, "Scope: USB cable").

mod device;
mod enumerate;

pub use device::HidrawDevice;
pub use enumerate::{HidrawInfo, connection_of, enumerate, find, find_supported};
