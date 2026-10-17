// SPDX-License-Identifier: GPL-3.0-or-later
//! Lighting: onboard modes, brightness/speed/direction and a colour.
//!
//! Shared opcodes `0x07` (backlight) / `0x08` (edge), both Bit8 packets. The
//! layout is `[op, mode, speed, brightness, (option<<4)|flags, R, G, B]`.
//! Backlight speed is inverted on the wire (host 0..4 -> 5..1); edge speed is
//! not. See `docs/protocol.md`.
//!
//! [`LedParam`] is what you set; [`LedState`] is what a GET returns. They
//! differ because SET and GET use opposite speed scales and GET reports a
//! flags/preset nibble.

use crate::cmd;
use crate::error::{Error, Result};
use crate::packet::Checksum;
#[cfg(test)]
use crate::packet::REPORT_LEN;
#[cfg(test)]
use crate::packet::packet;
use crate::transport::Session;

/// Backlight modes, wire numbers, shared by the family. Modes that need
/// host-side data (13, 20..22, 25) are intentionally absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Static = 1,
    Breathing = 2,
    Spectrum = 3,
    Wave = 4,
    Ripple = 5,
    StarDots = 6,
    Flow = 7,
    KeyShadow = 8,
    Layers = 9,
    Sine = 10,
    Spring = 11,
    Neon = 12,
    Radiant = 14,
    Loop = 15,
    ColourGrid = 16,
    Snowfall = 17,
    Meteor = 18,
    SilentSnow = 19,
}

impl Mode {
    pub const ALL: &'static [Mode] = &[
        Mode::Static,
        Mode::Breathing,
        Mode::Spectrum,
        Mode::Wave,
        Mode::Ripple,
        Mode::StarDots,
        Mode::Flow,
        Mode::KeyShadow,
        Mode::Layers,
        Mode::Sine,
        Mode::Spring,
        Mode::Neon,
        Mode::Radiant,
        Mode::Loop,
        Mode::ColourGrid,
        Mode::Snowfall,
        Mode::Meteor,
        Mode::SilentSnow,
    ];

    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn name(self) -> &'static str {
        match self {
            Mode::Static => "static",
            Mode::Breathing => "breathing",
            Mode::Spectrum => "spectrum",
            Mode::Wave => "wave",
            Mode::Ripple => "ripple",
            Mode::StarDots => "star-dots",
            Mode::Flow => "flow",
            Mode::KeyShadow => "key-shadow",
            Mode::Layers => "layers",
            Mode::Sine => "sine",
            Mode::Spring => "spring",
            Mode::Neon => "neon",
            Mode::Radiant => "radiant",
            Mode::Loop => "loop",
            Mode::ColourGrid => "colour-grid",
            Mode::Snowfall => "snowfall",
            Mode::Meteor => "meteor",
            Mode::SilentSnow => "silent-snow",
        }
    }

    pub fn from_name(s: &str) -> Option<Mode> {
        let want = s.trim().to_ascii_lowercase().replace('_', "-");
        Mode::ALL.iter().copied().find(|m| m.name() == want)
    }

    pub fn from_code(code: u8) -> Option<Mode> {
        Mode::ALL.iter().copied().find(|m| m.code() == code)
    }
}

/// Edge-light modes. Different set, same packet layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeMode {
    Off = 0,
    Static = 1,
    Breathing = 2,
    Neon = 3,
    Wave = 4,
    Snake = 5,
}

impl EdgeMode {
    pub const ALL: &'static [EdgeMode] = &[
        EdgeMode::Off,
        EdgeMode::Static,
        EdgeMode::Breathing,
        EdgeMode::Neon,
        EdgeMode::Wave,
        EdgeMode::Snake,
    ];

    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn name(self) -> &'static str {
        match self {
            EdgeMode::Off => "off",
            EdgeMode::Static => "static",
            EdgeMode::Breathing => "breathing",
            EdgeMode::Neon => "neon",
            EdgeMode::Wave => "wave",
            EdgeMode::Snake => "snake",
        }
    }

    pub fn from_name(s: &str) -> Option<EdgeMode> {
        let want = s.trim().to_ascii_lowercase();
        EdgeMode::ALL.iter().copied().find(|m| m.name() == want)
    }

    pub fn from_code(code: u8) -> Option<EdgeMode> {
        EdgeMode::ALL.iter().copied().find(|m| m.code() == code)
    }
}

/// The fixed/rainbow flags-nibble pair. LK67: `7` fixed, `8` rainbow **[HW]**.
/// Not every board uses the same pair, so it is passed in rather than baked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlagsPair {
    pub fixed: u8,
    pub rainbow: u8,
}

/// The pair for the LK67 (and the yc500 default).
pub const LK67_FLAGS: FlagsPair = FlagsPair {
    fixed: 7,
    rainbow: 8,
};

/// The highest mode code this firmware family implements. The set has gaps
/// (13, 20..22 and 25 are reserved), so codes in this range are accepted and
/// round-tripped even when this build does not model them.
pub const MAX_MODE_CODE: u8 = 25;

/// Whether `code` is a mode code the board could have. One predicate, so the
/// two front-ends cannot disagree about the range.
pub const fn is_mode_code(code: u8) -> bool {
    code >= 1 && code <= MAX_MODE_CODE
}

/// What a caller sets. `speed` and `brightness` are the host scale (0..4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedParam {
    pub mode: u8,
    pub speed: u8,
    pub brightness: u8,
    /// Direction / effect option, 0..15.
    pub option: u8,
    /// `true` sends the rainbow flag nibble; `false` sends the fixed one.
    pub rainbow: bool,
    pub rgb: [u8; 3],
}

impl LedParam {
    /// The 7-byte payload for a backlight SET (speed inverted on the wire).
    pub fn backlight_payload(&self, flags: FlagsPair) -> Result<[u8; 7]> {
        let wire_speed = 5u8.checked_sub(self.speed).ok_or(Error::OutOfRange {
            field: "speed",
            value: i64::from(self.speed),
        })?;
        self.payload(flags, wire_speed)
    }

    /// The 7-byte payload for an edge SET (speed not inverted). Unlike the
    /// backlight, the wire speed *is* the host speed, so it is range-checked
    /// here rather than by whatever called us.
    pub fn edge_payload(&self, flags: FlagsPair) -> Result<[u8; 7]> {
        if self.speed > 5 {
            return Err(Error::OutOfRange {
                field: "speed",
                value: i64::from(self.speed),
            });
        }
        self.payload(flags, self.speed)
    }

    /// Backlight is a Bit8 packet with the speed inverted on the wire.
    ///
    /// Test-only: the shipped paths build the payload and let `Session` send it
    /// through the safety guard, and a second packet-producing API would be a
    /// way around that.
    #[cfg(test)]
    pub fn backlight_packet(&self, flags: FlagsPair) -> Result<[u8; REPORT_LEN]> {
        packet(
            cmd::SET_LEDPARAM,
            &self.backlight_payload(flags)?,
            Checksum::Bit8,
        )
    }

    /// Edge light is the same layout; speed is not inverted. Test-only, as
    /// [`Self::backlight_packet`].
    #[cfg(test)]
    pub fn edge_packet(&self, flags: FlagsPair) -> Result<[u8; REPORT_LEN]> {
        packet(
            cmd::SET_SLEDPARAM,
            &self.edge_payload(flags)?,
            Checksum::Bit8,
        )
    }

    fn payload(&self, flags: FlagsPair, wire_speed: u8) -> Result<[u8; 7]> {
        // The LK67 brightness is 0..=4; sending 5 blanks the LEDs, so it is
        // rejected and reads of a stored 5 are clamped. **[HW]**
        if self.brightness > 4 {
            return Err(Error::OutOfRange {
                field: "brightness",
                value: i64::from(self.brightness),
            });
        }
        let flag_nibble = if self.rainbow {
            flags.rainbow
        } else {
            flags.fixed
        };
        // `option` is a full nibble on the wire; a value that would be
        // truncated is refused rather than silently masked.
        if self.option > 0x0F {
            return Err(Error::OutOfRange {
                field: "option",
                value: i64::from(self.option),
            });
        }
        let flags4 = ((self.option & 0x0F) << 4) | (flag_nibble & 0x0F);
        let [r, g, b] = wire_rgb(self.rgb);
        Ok([self.mode, wire_speed, self.brightness, flags4, r, g, b])
    }
}

/// What a GET returns. `speed` is converted back to the host scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedState {
    pub mode: u8,
    pub speed: u8,
    pub brightness: u8,
    pub option: u8,
    /// The raw flags nibble. `7` fixed, `8` rainbow, `0..=6` a preset index.
    pub flags: u8,
    pub rgb: [u8; 3],
    /// Set when `flags <= 6`: a firmware preset colour overriding RGB.
    pub preset: Option<u8>,
}

impl LedState {
    pub fn from_reply(reply: &[u8]) -> Option<Self> {
        // Backlight: invert the wire speed back to the host scale.
        Self::from_reply_for(reply, cmd::GET_LEDPARAM, true)
    }

    pub fn from_edge_reply(reply: &[u8]) -> Option<Self> {
        Self::from_reply_for(reply, cmd::GET_SLEDPARAM, false)
    }

    fn from_reply_for(reply: &[u8], opcode: u8, invert_speed: bool) -> Option<Self> {
        if reply.len() < 9 || reply[0] != opcode {
            return None;
        }
        let option = (reply[4] >> 4) & 0x0F;
        let flags = reply[4] & 0x0F;
        let wire_speed = reply[2];
        let speed = if invert_speed {
            5u8.saturating_sub(wire_speed).min(4)
        } else {
            wire_speed.min(4)
        };
        Some(Self {
            mode: reply[1],
            speed,
            // A stored 5 means the LEDs are blank; clamp so a write-back keeps
            // them lit. **[HW]**
            brightness: reply[3].min(4),
            option,
            flags,
            rgb: [reply[5], reply[6], reply[7]],
            preset: if flags <= 6 { Some(flags) } else { None },
        })
    }

    pub fn mode_name(&self) -> String {
        match Mode::from_code(self.mode) {
            Some(m) => m.name().to_string(),
            None => format!("mode-{}", self.mode),
        }
    }
}

/// The wire colour: white is sent as `0xFAFAFA`. A colour darker than
/// `0x080808` on every channel renders unlit, but "off" is the options bit,
/// not a black write, so it is left as given.
pub fn wire_rgb(rgb: [u8; 3]) -> [u8; 3] {
    if rgb == [0xFF, 0xFF, 0xFF] {
        [0xFA, 0xFA, 0xFA]
    } else {
        rgb
    }
}

impl Session<'_> {
    /// Write the backlight. **Fire-and-forget:** the board applies the SET but
    /// stalls its control endpoint if a feature report is read while it is
    /// committing the change, so no read-back is issued here. Verify with a
    /// separate `gamakay light`. **[HW]**
    pub fn set_backlight(&mut self, p: &LedParam, flags: FlagsPair) -> Result<()> {
        let payload = p.backlight_payload(flags)?;
        self.send(cmd::SET_LEDPARAM, &payload, Checksum::Bit8)
    }

    /// Write the edge light; same fire-and-forget shape.
    pub fn set_edge(&mut self, p: &LedParam, flags: FlagsPair) -> Result<()> {
        let payload = p.edge_payload(flags)?;
        self.send(cmd::SET_SLEDPARAM, &payload, Checksum::Bit8)
    }

    /// Turn the backlight off/on through the options bit (not a black write).
    /// Read-modify-write so the unmodelled bits survive. The read happens
    /// *before* the write; the write itself is fire-and-forget.
    pub fn set_backlight_off(&mut self, off: bool) -> Result<crate::ops::Options> {
        let mut o = self.options()?;
        if off {
            o.raw |= 0x10;
        } else {
            o.raw &= !0x10;
        }
        o.backlight_off = off;
        self.send(
            cmd::SET_KBOPTION,
            &[0, o.raw, o.fn_matrix, o.power_save],
            Checksum::Bit7,
        )?;
        Ok(o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::verify_checksum;

    #[test]
    fn backlight_inverts_speed_and_checksums_bit8() {
        let p = LedParam {
            mode: Mode::Wave.code(),
            speed: 2,
            brightness: 3,
            option: 1,
            rainbow: false,
            rgb: [0xFF, 0x00, 0x00],
        };
        let buf = p.backlight_packet(LK67_FLAGS).unwrap();
        assert_eq!(buf[0], cmd::SET_LEDPARAM);
        assert_eq!(buf[1], 4); // wave
        assert_eq!(buf[2], 3); // host 2 -> wire 3
        assert_eq!(buf[3], 3); // brightness
        assert_eq!(buf[4], (1 << 4) | 7); // direction 1, fixed colour
        assert_eq!(&buf[5..8], &[0xFF, 0x00, 0x00]);
        verify_checksum(&buf, Checksum::Bit8).unwrap();
    }

    #[test]
    fn edge_does_not_invert_speed() {
        let p = LedParam {
            mode: EdgeMode::Snake.code(),
            speed: 2,
            brightness: 4,
            option: 0,
            rainbow: true,
            rgb: [0x00, 0x00, 0x00],
        };
        let buf = p.edge_packet(LK67_FLAGS).unwrap();
        assert_eq!(buf[0], cmd::SET_SLEDPARAM);
        assert_eq!(buf[2], 2); // not inverted
        assert_eq!(buf[4] & 0x0F, 8); // rainbow
    }

    #[test]
    fn decode_converts_backlight_speed_to_host_scale() {
        // The LED reply carries RGB through byte 7, so it has no Bit7 checksum.
        // wire speed 4 -> host 1; flags 7 fixed.
        let reply = crate::packet::packet(
            cmd::GET_LEDPARAM,
            &[0x01, 0x04, 0x05, 0x07, 0x95, 0x07, 0xE1],
            Checksum::None,
        )
        .unwrap();
        let s = LedState::from_reply(&reply).unwrap();
        assert_eq!(s.mode, 1);
        assert_eq!(s.speed, 1);
        assert_eq!(s.brightness, 4); // stored 5 (blank) clamps to 4
        assert_eq!(s.flags, 7);
        assert_eq!(s.rgb, [0x95, 0x07, 0xE1]);
        assert_eq!(s.preset, None);
    }

    #[test]
    fn white_is_darkened_to_fafafa() {
        assert_eq!(wire_rgb([0xFF, 0xFF, 0xFF]), [0xFA, 0xFA, 0xFA]);
    }

    #[test]
    fn brightness_above_four_is_rejected() {
        let p = LedParam {
            mode: 1,
            speed: 0,
            brightness: 5,
            option: 0,
            rainbow: false,
            rgb: [0, 0, 0],
        };
        assert!(matches!(
            p.backlight_packet(LK67_FLAGS),
            Err(Error::OutOfRange {
                field: "brightness",
                ..
            })
        ));
    }

    #[test]
    fn mode_names_round_trip() {
        for m in Mode::ALL {
            assert_eq!(Mode::from_name(m.name()), Some(*m));
            assert_eq!(Mode::from_code(m.code()), Some(*m));
        }
        for m in EdgeMode::ALL {
            assert_eq!(EdgeMode::from_name(m.name()), Some(*m));
        }
    }
}
