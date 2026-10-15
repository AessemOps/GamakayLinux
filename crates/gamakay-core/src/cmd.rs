// SPDX-License-Identifier: GPL-3.0-or-later
//! Command opcodes. The yc500 column (the LK67's family) plus the shared
//! ones. `docs/protocol.md` is the reference.

// --- Shared across both families -------------------------------------------

/// Identify handshake. Reply: `[0x8F, id:u32 LE, ..]`.
pub const GET_USB_VERSION: u8 = 0x8F;
/// Firmware revision. `(reply[2] << 8) | reply[1]`, each nibble a decimal digit.
pub const GET_REVISION: u8 = 0x80;
/// LED (backlight) parameters. GET echoes the opcode in byte 0.
pub const SET_LEDPARAM: u8 = 0x07;
pub const GET_LEDPARAM: u8 = 0x87;
/// Edge-light parameters. Same layout as LEDPARAM, speed is not inverted.
pub const SET_SLEDPARAM: u8 = 0x08;
pub const GET_SLEDPARAM: u8 = 0x88;
/// Macro blob, paginated (50 slots x 256 bytes).
/// Macro write. The reference's yc500 value is `0x16`, but the LK67 (both
/// firmwares) accepts the **base-class** `0x0B` and ignores `0x16`; verified
/// by capture. Header `[slot, 0, 1, page, 0, 0]`, 56 bytes at byte 8, 5 pages.
pub const SET_MACRO: u8 = 0x0B;
/// The reference yc500 macro write; ignored by the LK67. Kept for reference.
pub const SET_MACRO_YC500: u8 = 0x16;
pub const GET_MACRO: u8 = 0x8B;
/// Auto-OS: `[0|1]` on SET, `reply[1] == 1` on GET.
pub const SET_AUTO_OS: u8 = 0x17;
pub const GET_AUTO_OS: u8 = 0x97;

// --- yc500 -----------------------------------------------------------------

/// Select profile. GET returns the index in `reply[1]`, zero based.
pub const SET_PROFILE: u8 = 0x05;
pub const GET_PROFILE: u8 = 0x85;
/// Keyboard options bitfield. Read-modify-write to keep unmodelled bits.
pub const SET_KBOPTION: u8 = 0x06;
pub const GET_KBOPTION: u8 = 0x86;
/// Whole keymap layer, 9 x 56-byte pages, commit on page 8.
pub const SET_KEYMATRIX: u8 = 0x09;
pub const GET_KEYMATRIX: u8 = 0x89;
/// Fn layer, bulk shape (yc500).
pub const SET_FN: u8 = 0x10;
pub const GET_FN: u8 = 0x90;
/// Debounce, range 1..10, value at byte 2.
pub const SET_DEBOUNCE: u8 = 0x11;
pub const GET_DEBOUNCE: u8 = 0x91;
/// Sleep timers: four u16 LE seconds at bytes 8..16 on SET; the GET reply
/// places them at bytes 1..9 (an asymmetry in the firmware).
pub const SET_SLEEPTIME: u8 = 0x12;
pub const GET_SLEEPTIME: u8 = 0x92;
/// One keymap slot, `[profile, slot]`, value at bytes 8..12.
pub const SET_KEY_ONE: u8 = 0x13;
pub const SET_FN_ONE: u8 = 0x15;
/// Per-key colour upload, 7 x 56-byte pages, data from byte 8.
pub const SET_USERPIC: u8 = 0x0C;
pub const GET_USERPIC: u8 = 0x8C;
/// Factory reset. Needs ~4 s. Bare opcode.
pub const SET_RESET: u8 = 0x02;

// --- Blocked: destructive or bootloader ------------------------------------

/// Flash-chip erase (both families), ~55 s. Also `0x2C` (yc500 display erase).
pub const FLASH_CHIP_ERASE: u8 = 0xAC;
pub const DISPLAY_ERASE: u8 = 0x2C;
/// USB boot entry, only with trailer `55 AA 55 AA`.
pub const USB_BOOT: u8 = 0x7F;
/// Display-chip boot entry. `0xB1` verify, `0xB2` TFT probe, `0x31`/`0x32`
/// start/data, `0xB0` poll.
pub const OLED_BOOT: u8 = 0x30;
/// Sensor recalibration; on-then-off commits zeroed tables.
pub const CALIBRATION_ON: u8 = 0x1C;
pub const CALIBRATION_OFF: u8 = 0x1E;
/// RF boot entry.
pub const RF_BOOT: u8 = 0xF8;
/// Boot-mode image transfer opcodes.
pub const BOOT_INIT: u8 = 0xBA;
