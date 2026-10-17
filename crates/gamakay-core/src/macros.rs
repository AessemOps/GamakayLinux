// SPDX-License-Identifier: GPL-3.0-or-later
//! Onboard macros: 50 slots x 256 bytes.
//!
//! A macro blob is a u16 LE repeat count, a stream of events, and a four-byte
//! zero terminator. Writes are paginated (`0x0B`, 56 bytes a page, all five
//! pages); reads are four raw pages (`0x8B`, pages 0..3). A
//! macro is bound to a key through the keymap, slot value `[0x09, mode, index,
//! 0]`. See `docs/protocol.md`.

use crate::cmd;
use crate::error::{Error, Result};
use crate::packet::Checksum;
use crate::transport::Session;

/// Macro slots.
pub const MACRO_SLOTS: u8 = 50;
/// Bytes in a macro blob.
pub const MACRO_BYTES: usize = 256;
/// Payload bytes per write page.
pub const MACRO_PAGE_DATA: usize = 56;
/// Pages a macro is read in: 4 x 64.
pub const MACRO_PAGES: u8 = 4;
/// Pages a macro write sends: 5 x 56 (>= 256 bytes).
pub const MACRO_WRITE_PAGES: u8 = 5;
/// Bytes the event stream may occupy: the blob less the two repeat bytes and
/// the four-byte terminator.
pub const MACRO_STREAM_MAX: usize = MACRO_BYTES - 6;

const MOUSE_BTN_BASE: u8 = 0xF0; // ..=0xF4: left, right, middle, back, forward
const MOUSE_MOVE: u8 = 0xF9;
const KEY_USAGE_MAX: u8 = 0xEF;

/// A macro event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroEvent {
    Key {
        usage: u8,
        pressed: bool,
        delay_ms: u16,
    },
    MouseButton {
        button: u8,
        pressed: bool,
        delay_ms: u16,
    },
    MouseMove {
        dx: i8,
        dy: i8,
        delay_ms: u8,
    },
}

/// A whole macro.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Macro {
    pub repeat: u16,
    pub events: Vec<MacroEvent>,
}

impl MacroEvent {
    /// Bytes this event adds to the stream: four for a mouse move, two for a
    /// press whose delay fits the short form, otherwise four.
    pub fn encoded_len(&self) -> usize {
        match *self {
            MacroEvent::MouseMove { .. } => 4,
            MacroEvent::Key { delay_ms, .. } | MacroEvent::MouseButton { delay_ms, .. } => {
                if (1..=127).contains(&delay_ms) { 2 } else { 4 }
            }
        }
    }
}

impl Macro {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Bytes the stream occupies, which is what the 256-byte blob limits.
    pub fn stream_len(&self) -> usize {
        self.events.iter().map(MacroEvent::encoded_len).sum()
    }

    /// Encode to the 256-byte blob.
    pub fn to_blob(&self) -> Result<[u8; MACRO_BYTES]> {
        let mut stream = Vec::new();
        for e in &self.events {
            match *e {
                MacroEvent::Key {
                    usage,
                    pressed,
                    delay_ms,
                } => {
                    if !(0x04..=KEY_USAGE_MAX).contains(&usage) {
                        return Err(Error::OutOfRange {
                            field: "key usage",
                            value: i64::from(usage),
                        });
                    }
                    encode_press(&mut stream, usage, pressed, delay_ms);
                }
                MacroEvent::MouseButton {
                    button,
                    pressed,
                    delay_ms,
                } => {
                    if button > 4 {
                        return Err(Error::OutOfRange {
                            field: "mouse button",
                            value: i64::from(button),
                        });
                    }
                    encode_press(&mut stream, MOUSE_BTN_BASE + button, pressed, delay_ms);
                }
                MacroEvent::MouseMove { dx, dy, delay_ms } => {
                    stream.extend_from_slice(&[MOUSE_MOVE, delay_ms, dx as u8, dy as u8]);
                }
            }
        }
        if stream.len() > MACRO_STREAM_MAX {
            return Err(Error::OutOfRange {
                field: "macro length",
                value: stream.len() as i64,
            });
        }
        let mut blob = [0u8; MACRO_BYTES];
        blob[..2].copy_from_slice(&self.repeat.to_le_bytes());
        blob[2..2 + stream.len()].copy_from_slice(&stream);
        Ok(blob)
    }

    /// Lenient decode: stops at the terminator, the end, or an unknown code.
    pub fn from_blob(blob: &[u8; MACRO_BYTES]) -> Self {
        let repeat = u16::from_le_bytes([blob[0], blob[1]]);
        let mut events = Vec::new();
        let mut i = 2;
        while i + 1 < MACRO_BYTES {
            let code = blob[i];
            if code == 0 {
                break;
            }
            if code == MOUSE_MOVE {
                if i + 3 >= MACRO_BYTES {
                    break;
                }
                events.push(MacroEvent::MouseMove {
                    delay_ms: blob[i + 1],
                    dx: blob[i + 2] as i8,
                    dy: blob[i + 3] as i8,
                });
                i += 4;
                continue;
            }
            let pressed = blob[i + 1] & 0x80 != 0;
            let low = blob[i + 1] & 0x7F;
            let delay_ms = if low != 0 {
                i += 2;
                u16::from(low)
            } else {
                if i + 3 >= MACRO_BYTES {
                    break;
                }
                let d = u16::from_le_bytes([blob[i + 2], blob[i + 3]]);
                i += 4;
                d
            };
            if (0x04..=KEY_USAGE_MAX).contains(&code) {
                events.push(MacroEvent::Key {
                    usage: code,
                    pressed,
                    delay_ms,
                });
            } else if (MOUSE_BTN_BASE..=MOUSE_BTN_BASE + 4).contains(&code) {
                events.push(MacroEvent::MouseButton {
                    button: code - MOUSE_BTN_BASE,
                    pressed,
                    delay_ms,
                });
            } else {
                break;
            }
        }
        Macro { repeat, events }
    }
}

/// Short form `[code, pressed<<7 | delay]` when the delay fits in 7 bits and
/// is non-zero; otherwise the 4-byte long form. A zero low nibble is the
/// long-form marker, so a zero delay must take the long form.
fn encode_press(out: &mut Vec<u8>, code: u8, pressed: bool, delay_ms: u16) {
    let hi = (pressed as u8) << 7;
    if (1..=127).contains(&delay_ms) {
        out.extend_from_slice(&[code, hi | delay_ms as u8]);
    } else {
        out.extend_from_slice(&[code, hi]);
        out.extend_from_slice(&delay_ms.to_le_bytes());
    }
}

/// Parse the `--steps` DSL shared by the CLI and the GUI:
/// `A+@20, A-@20`, `mouse:0+@10`, `move:5:-3@10`. Each event may carry a
/// trailing `@MS` delay (default 0).
pub fn parse_steps(s: &str) -> Result<Vec<MacroEvent>> {
    let mut events = Vec::new();
    for raw in s.split(',') {
        let tok = raw.trim();
        if tok.is_empty() {
            continue;
        }
        let (body, delay) = match tok.rsplit_once('@') {
            Some((b, d)) => (
                b.trim(),
                d.trim()
                    .parse::<u16>()
                    .map_err(|_| Error::Transport(format!("bad delay in {tok:?}")))?,
            ),
            None => (tok, 0u16),
        };
        if let Some(rest) = body.strip_prefix("move:") {
            let (dx, dy) = rest
                .split_once(':')
                .ok_or_else(|| Error::Transport(format!("move needs dx:dy, got {rest:?}")))?;
            let dx: i8 = dx
                .trim()
                .parse()
                .map_err(|_| Error::Transport("move dx".into()))?;
            let dy: i8 = dy
                .trim()
                .parse()
                .map_err(|_| Error::Transport("move dy".into()))?;
            if delay > 255 {
                return Err(Error::OutOfRange {
                    field: "move delay",
                    value: i64::from(delay),
                });
            }
            events.push(MacroEvent::MouseMove {
                dx,
                dy,
                delay_ms: delay as u8,
            });
            continue;
        }
        if let Some(rest) = body.strip_prefix("mouse:") {
            let (n, pressed) = split_press(rest)?;
            let button: u8 = n
                .parse()
                .map_err(|_| Error::Transport("mouse button number (0..4)".into()))?;
            events.push(MacroEvent::MouseButton {
                button,
                pressed,
                delay_ms: delay,
            });
            continue;
        }
        let (name, pressed) = split_press(body)?;
        let usage = crate::keycode::usage(name)
            .ok_or_else(|| Error::Transport(format!("unknown key {name:?}")))?;
        events.push(MacroEvent::Key {
            usage,
            pressed,
            delay_ms: delay,
        });
    }
    Ok(events)
}

fn split_press(s: &str) -> Result<(&str, bool)> {
    if let Some(n) = s.strip_suffix('+') {
        Ok((n.trim(), true))
    } else if let Some(n) = s.strip_suffix('-') {
        Ok((n.trim(), false))
    } else {
        Err(Error::Transport(format!(
            "event {s:?} needs + (down) or - (up)"
        )))
    }
}

/// A short human description of a macro.
pub fn describe(m: &Macro) -> String {
    if m.events.is_empty() {
        return "(empty)".to_string();
    }
    m.events
        .iter()
        .map(describe_event)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A short human description of one event.
pub fn describe_event(e: &MacroEvent) -> String {
    match *e {
        MacroEvent::Key {
            usage,
            pressed,
            delay_ms,
        } => {
            let name = crate::keycode::name(usage)
                .map(str::to_string)
                .unwrap_or_else(|| format!("0x{usage:02X}"));
            format!("{name}{}@{delay_ms}", if pressed { "+" } else { "-" })
        }
        MacroEvent::MouseButton {
            button,
            pressed,
            delay_ms,
        } => format!(
            "mouse:{button}{}@{delay_ms}",
            if pressed { "+" } else { "-" }
        ),
        MacroEvent::MouseMove { dx, dy, delay_ms } => format!("move:{dx}:{dy}@{delay_ms}"),
    }
}

impl Session<'_> {
    /// Read a macro (`0x8B [slot, page]`, four raw pages).
    pub fn read_macro(&mut self, slot: u8) -> Result<Macro> {
        if slot >= MACRO_SLOTS {
            return Err(Error::OutOfRange {
                field: "macro slot",
                value: i64::from(slot),
            });
        }
        let mut blob = [0u8; MACRO_BYTES];
        for page in 0..MACRO_PAGES {
            let reply = self.read_raw_page(cmd::GET_MACRO, &[slot, page], Checksum::Bit7)?;
            let start = page as usize * 64;
            blob[start..start + 64].copy_from_slice(&reply);
        }
        Ok(Macro::from_blob(&blob))
    }

    /// Write a macro. Verified shape (from a vendor capture): opcode `0x0B`
    /// (the base-class value, **not** the reference's yc500 `0x16`), header
    /// `[slot, 0, 1, page, 0, 0]`, 56 bytes at byte 8, five pages. **[HW]**
    pub fn write_macro(&mut self, slot: u8, m: &Macro) -> Result<()> {
        if slot >= MACRO_SLOTS {
            return Err(Error::OutOfRange {
                field: "macro slot",
                value: i64::from(slot),
            });
        }
        let blob = m.to_blob()?;
        for page in 0..MACRO_WRITE_PAGES {
            let mut payload = [0u8; 63];
            payload[0] = slot; // packet byte 1
            payload[1] = 0x00; // packet byte 2
            payload[2] = 0x01; // packet byte 3
            payload[3] = page; // packet byte 4
            let start = page as usize * MACRO_PAGE_DATA;
            if start < MACRO_BYTES {
                let end = (start + MACRO_PAGE_DATA).min(MACRO_BYTES);
                payload[7..7 + (end - start)].copy_from_slice(&blob[start..end]);
            }
            self.send_page(
                cmd::SET_MACRO,
                &payload,
                Checksum::Bit7,
                page == MACRO_WRITE_PAGES - 1,
            )?;
        }
        Ok(())
    }

    /// Clear a macro slot.
    pub fn delete_macro(&mut self, slot: u8) -> Result<()> {
        self.write_macro(slot, &Macro::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_blob_round_trips() {
        let m = Macro {
            repeat: 3,
            events: vec![
                MacroEvent::Key {
                    usage: 0x04,
                    pressed: true,
                    delay_ms: 20,
                },
                MacroEvent::Key {
                    usage: 0x04,
                    pressed: false,
                    delay_ms: 20,
                },
                MacroEvent::MouseMove {
                    dx: 5,
                    dy: -3,
                    delay_ms: 10,
                },
                MacroEvent::MouseButton {
                    button: 0,
                    pressed: true,
                    delay_ms: 0,
                },
            ],
        };
        let blob = m.to_blob().unwrap();
        assert_eq!(Macro::from_blob(&blob), m);
    }

    #[test]
    fn zero_delay_uses_the_long_form() {
        let mut out = Vec::new();
        encode_press(&mut out, 0x04, true, 0);
        assert_eq!(out, vec![0x04, 0x80, 0x00, 0x00]);
        let mut out2 = Vec::new();
        encode_press(&mut out2, 0x04, false, 200);
        assert_eq!(out2, vec![0x04, 0x00, 0xC8, 0x00]);
    }

    #[test]
    fn oversize_macro_is_rejected() {
        let ev = MacroEvent::Key {
            usage: 0x04,
            pressed: true,
            delay_ms: 1,
        };
        let m = Macro {
            repeat: 0,
            events: vec![ev; 200],
        };
        assert!(m.to_blob().is_err());
    }

    /// The byte counter the editor shows must agree with the encoder, or a
    /// take can look like it fits and then be refused at save.
    #[test]
    fn encoded_len_matches_the_blob() {
        let m = Macro {
            repeat: 1,
            events: vec![
                MacroEvent::Key {
                    usage: 0x04,
                    pressed: true,
                    delay_ms: 20, // short form
                },
                MacroEvent::Key {
                    usage: 0x04,
                    pressed: false,
                    delay_ms: 0, // long form
                },
                MacroEvent::MouseMove {
                    dx: 1,
                    dy: 2,
                    delay_ms: 3,
                },
                MacroEvent::MouseButton {
                    button: 0,
                    pressed: true,
                    delay_ms: 200, // long form
                },
            ],
        };
        let blob = m.to_blob().unwrap();
        assert_eq!(m.stream_len(), 2 + 4 + 4 + 4);
        // The stream sits after the repeat count, and the rest is terminator.
        let stream = &blob[2..2 + m.stream_len()];
        assert!(stream.iter().any(|&b| b != 0));
        assert!(blob[2 + m.stream_len()..].iter().all(|&b| b == 0));
    }

    /// A take that fills the slot exactly is accepted; one byte more is not.
    #[test]
    fn the_stream_limit_is_the_blob_boundary() {
        let ev = MacroEvent::Key {
            usage: 0x04,
            pressed: true,
            delay_ms: 1,
        };
        let m = Macro {
            repeat: 1,
            events: vec![ev; MACRO_STREAM_MAX / 2],
        };
        assert_eq!(m.stream_len(), MACRO_STREAM_MAX);
        assert!(m.to_blob().is_ok());
        let mut over = m.clone();
        over.events.push(ev);
        assert!(over.to_blob().is_err());
    }
}
