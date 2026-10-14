// SPDX-License-Identifier: GPL-3.0-or-later
//! Read operations. Each is a thin wrap of `Session::roundtrip` plus a
//! decoder. Writes live in the feature modules added in later phases.

use crate::cmd;
use crate::error::{Error, Result};
use crate::packet::Checksum;
use crate::transport::Session;

/// `0x8F` identify: `[0x8F, id:u32 LE, ..]`.
pub fn parse_device_id(reply: &[u8]) -> Option<u32> {
    if reply.len() >= 5 && reply[0] == cmd::GET_USB_VERSION {
        Some(u32::from_le_bytes([reply[1], reply[2], reply[3], reply[4]]))
    } else {
        None
    }
}

/// `0x87` LED parameters. Shared opcode; the checksum is Bit7 on GET.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedParam {
    pub mode: u8,
    /// Wire speed. For yc500 this is 1..5 (host 0..4 inverted).
    pub speed: u8,
    pub brightness: u8,
    /// The option nibble (`(byte4 >> 4) & 0x0F`).
    pub option: u8,
    /// The flags nibble (`byte4 & 0x0F`): 7 fixed colour, 8 rainbow.
    pub flags: u8,
    pub rgb: [u8; 3],
    /// On GET, flags 0..=6 are a firmware preset-colour index overriding RGB.
    pub preset: Option<u8>,
}

impl LedParam {
    pub fn from_reply(reply: &[u8]) -> Option<Self> {
        if reply.len() < 9 || reply[0] != cmd::GET_LEDPARAM {
            return None;
        }
        let option = (reply[4] >> 4) & 0x0F;
        let flags = reply[4] & 0x0F;
        Some(Self {
            mode: reply[1],
            speed: reply[2],
            brightness: reply[3],
            option,
            flags,
            rgb: [reply[5], reply[6], reply[7]],
            preset: if flags <= 6 { Some(flags) } else { None },
        })
    }
}

/// `0x86` yc500 keyboard options bitfield. Read-modify-write to preserve the
/// bits we do not model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Options {
    pub win_lock: bool,
    pub mac_mode: bool,
    pub wasd_swap: bool,
    pub backlight_off: bool,
    pub side_light_off: bool,
    pub keyboard_mode: bool,
    pub keyboard_lock: bool,
    pub fn_matrix: u8,
    pub power_save: u8,
    /// The raw flags byte, kept so writes can round-trip unmodelled bits.
    pub raw: u8,
}

impl Options {
    pub fn from_reply(reply: &[u8]) -> Option<Self> {
        if reply.len() < 5 || reply[0] != cmd::GET_KBOPTION {
            return None;
        }
        let raw = reply[2];
        Some(Self {
            win_lock: raw & 0x01 != 0,
            mac_mode: raw & 0x02 != 0,
            wasd_swap: raw & 0x08 != 0,
            backlight_off: raw & 0x10 != 0,
            side_light_off: raw & 0x20 != 0,
            keyboard_mode: raw & 0x40 != 0,
            keyboard_lock: raw & 0x80 != 0,
            fn_matrix: reply[3],
            power_save: reply[4],
            raw,
        })
    }
}

/// `0x92` sleep timers: four u16 LE seconds, at bytes 8..16 of the reply —
/// the same offsets the SET uses. (Reference docs place the GET reply at
/// bytes 1..9, but on the LK67 byte 7 is the request's checksum byte, so the
/// 1..9 layout would overwrite it; hardware wins.) **[HW]**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SleepTimers {
    pub sleep_bt: u16,
    pub sleep_24g: u16,
    pub deep_bt: u16,
    pub deep_24g: u16,
}

impl SleepTimers {
    pub fn from_reply(reply: &[u8]) -> Option<Self> {
        if reply.len() < 16 || reply[0] != cmd::GET_SLEEPTIME {
            return None;
        }
        let u = |o: usize| u16::from_le_bytes([reply[o], reply[o + 1]]);
        Some(Self {
            sleep_bt: u(8),
            sleep_24g: u(10),
            deep_bt: u(12),
            deep_24g: u(14),
        })
    }
}

impl Session<'_> {
    /// `0x8F`: the vendor-table device id.
    pub fn device_id(&mut self) -> Result<u32> {
        let reply = self.roundtrip(cmd::GET_USB_VERSION, &[], Checksum::Bit7)?;
        parse_device_id(&reply).ok_or(Error::ShortReply {
            opcode: cmd::GET_USB_VERSION,
            len: reply.len(),
        })
    }

    /// `0x80`: firmware revision `(reply[2] << 8) | reply[1]`.
    pub fn revision(&mut self) -> Result<u16> {
        let reply = self.roundtrip(cmd::GET_REVISION, &[], Checksum::Bit7)?;
        Ok((u16::from(reply[2]) << 8) | u16::from(reply[1]))
    }

    /// `0x85`: active profile index, zero based, in `reply[1]`.
    pub fn profile(&mut self) -> Result<u8> {
        let reply = self.roundtrip(cmd::GET_PROFILE, &[], Checksum::Bit7)?;
        Ok(reply[1])
    }

    /// `0x97`: auto-OS, `reply[1] == 1`.
    pub fn auto_os(&mut self) -> Result<bool> {
        let reply = self.roundtrip(cmd::GET_AUTO_OS, &[], Checksum::Bit7)?;
        Ok(reply[1] == 1)
    }

    /// `0x91`: debounce, value in `reply[2]`, range 1..10.
    pub fn debounce(&mut self) -> Result<u8> {
        let reply = self.roundtrip(cmd::GET_DEBOUNCE, &[], Checksum::Bit7)?;
        Ok(reply[2])
    }

    /// `0x92`: sleep timers.
    pub fn sleep_timers(&mut self) -> Result<SleepTimers> {
        let reply = self.roundtrip(cmd::GET_SLEEPTIME, &[], Checksum::Bit7)?;
        SleepTimers::from_reply(&reply).ok_or(Error::ShortReply {
            opcode: cmd::GET_SLEEPTIME,
            len: reply.len(),
        })
    }

    /// `0x86`: keyboard options bitfield.
    pub fn options(&mut self) -> Result<Options> {
        let reply = self.roundtrip(cmd::GET_KBOPTION, &[], Checksum::Bit7)?;
        Options::from_reply(&reply).ok_or(Error::ShortReply {
            opcode: cmd::GET_KBOPTION,
            len: reply.len(),
        })
    }

    /// `0x87`: LED parameters.
    pub fn led_param(&mut self) -> Result<LedParam> {
        let reply = self.roundtrip(cmd::GET_LEDPARAM, &[], Checksum::Bit7)?;
        LedParam::from_reply(&reply).ok_or(Error::ShortReply {
            opcode: cmd::GET_LEDPARAM,
            len: reply.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn led_param_decodes_nibbles() {
        // LEDPARAM is a Bit8 packet: bytes 0..8 are op+payload, checksum at 8.
        let reply = crate::packet::packet(
            cmd::GET_LEDPARAM,
            &[4, 3, 4, 0x87, 0xFF, 0x00, 0x00],
            Checksum::Bit8,
        )
        .unwrap();
        let led = LedParam::from_reply(&reply).unwrap();
        assert_eq!(led.mode, 4);
        assert_eq!(led.speed, 3);
        assert_eq!(led.brightness, 4);
        assert_eq!(led.option, 8);
        assert_eq!(led.flags, 7);
        assert_eq!(led.rgb, [0xFF, 0x00, 0x00]);
        assert_eq!(led.preset, None);
    }

    #[test]
    fn options_bitfield_maps() {
        let reply =
            crate::packet::packet(cmd::GET_KBOPTION, &[0, 0x80 | 0x08, 2, 3], Checksum::Bit7)
                .unwrap();
        let o = Options::from_reply(&reply).unwrap();
        assert!(o.keyboard_lock);
        assert!(o.wasd_swap);
        assert!(!o.mac_mode);
        assert_eq!(o.fn_matrix, 2);
        assert_eq!(o.power_save, 3);
        assert_eq!(o.raw, 0x88);
    }
}
