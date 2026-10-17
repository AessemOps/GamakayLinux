// SPDX-License-Identifier: GPL-3.0-or-later
//! The keymap: 128 slots of 4 bytes per layer.
//!
//! yc500 reads a layer as eight raw 64-byte pages (`0x89` base, `0x90` Fn) and
//! writes one slot at a time (`0x13` base, `0x15` Fn), value at bytes 8..12.
//! A write is fire-and-forget, like every SET (see `docs/protocol.md`).

use crate::cmd;
use crate::error::{Error, Result};
use crate::keycode;
use crate::packet::Checksum;
use crate::transport::Session;

/// Slots in a layer.
pub const SLOTS: usize = 128;
/// Bytes in a layer: 128 slots x 4.
pub const LAYER_BYTES: usize = SLOTS * 4;
/// Pages a layer is read in: 8 x 64.
pub const PAGES: u8 = 8;
/// Payload bytes per bulk-write page.
pub const BULK_PAGE_DATA: usize = 56;
/// Bulk-write pages: eight data pages then the commit page.
pub const BULK_PAGES: u8 = 9;

/// Slots a bulk layer write actually reaches: the nine pages carry 504 bytes,
/// so slots 126 and 127 are never transmitted and cannot be remapped. A write
/// to them silently does nothing, so front-ends refuse them instead.
pub const TX_SLOTS: usize = BULK_PAGE_DATA * BULK_PAGES as usize / 4;

/// Extra passes allowed to confirm a layer read (see [`Session::read_layer`]).
pub const LAYER_READ_TRIES: u32 = 2;

/// The name of a `0x0D` firmware action. The vendor app encodes these as
/// `[0x0D, parameter, how, 0]` and names two literally (`LEDBIRINC =
/// [13,2,1,0]`, `LEDBIRDEC = [13,2,2,0]`); the rest follow the same shape and
/// the LK67's Fn layer uses exactly these, matching the manual's Fn combos
/// (Fn+Del, Fn+PgUp/PgDn, Fn+arrows).
fn action_name(param: u8, how: u8) -> String {
    let param = match param {
        1 => "Light",
        2 => "Bright",
        3 => "Speed",
        8 => "Side",
        _ => return format!("fn {param:02x} {how:02x}"),
    };
    match how {
        0 => format!("{param} mode"),
        1 => format!("{param} +"),
        2 => format!("{param} -"),
        _ => format!("{param} {how}"),
    }
}

// The keymap is **per profile**: `0x05` (`SET_PROFILE`) selects the active
// profile and the keymap commands always address it, so a profile is what the
// vendor UI calls a "layer". The yc500 device table's `layer` field is this
// profile count — the LK67 has 8, each with its own keymap (identical clones
// until edited).
//
// This board rejects the newer packet shape (`0x89 [profile, 0xFF, page,
// subProfile]`, which the vendor app 3.0.27 sends) by echoing it back, so the
// active profile is the only way to address a layer. See "Layers" in
// `docs/protocol.md`.

/// A slot's decoded value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotValue {
    /// All zero: on the Fn layer this falls through; on the base layer the key
    /// is dead.
    Empty,
    /// A HID keyboard/keypad usage (byte 2).
    Hid(u8),
    /// A consumer usage (page 0x0C), little-endian in bytes 2..4.
    Consumer(u16),
    /// A macro binding: `[mode, index, 0]`; mode 0 repeat, 1 toggle, 2 hold.
    Macro { mode: u8, index: u8 },
    /// A special function: 1 Fn, 12 backlight on/off, 17 Caps LED swap.
    Special(u8),
    /// A firmware action (tag `0x0D`): the lighting controls the firmware runs
    /// itself. `a` is the parameter, `b` how to move it.
    Action { a: u8, b: u8 },
    /// A profile action (tag `0x08`): the vendor's `profile_value` /
    /// `profile_loop` / `profile_exchange` family. `kind` 4 selects a profile,
    /// `index` which one (zero based).
    Profile { kind: u8, index: u8 },
    /// An OS-mode switch (tag `0x0E`): 0 Windows, 1 macOS, 2 Android, 3 iOS —
    /// the app's `fnSys` encoding.
    OsMode(u8),
    /// A tag we do not model; kept verbatim so writes round-trip.
    Unknown([u8; 4]),
}

/// One 4-byte slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot(pub [u8; 4]);

impl Slot {
    pub const EMPTY: Slot = Slot([0, 0, 0, 0]);

    pub fn value(self) -> SlotValue {
        let [tag, a, b, c] = self.0;
        match tag {
            0x00 if a == 0 && b == 0 && c == 0 => SlotValue::Empty,
            0x00 => SlotValue::Hid(b),
            0x03 => SlotValue::Consumer(u16::from_le_bytes([b, c])),
            0x09 => SlotValue::Macro { mode: a, index: b },
            0x0A => SlotValue::Special(a),
            0x0D => SlotValue::Action { a, b },
            0x08 => SlotValue::Profile { kind: b, index: c },
            0x0E => SlotValue::OsMode(b),
            _ => SlotValue::Unknown(self.0),
        }
    }

    /// Whether the firmware runs this entry itself, so the Fn-layer key it
    /// sits on cannot be repurposed. The knob's own keys are the other such
    pub fn from_value(v: SlotValue) -> Self {
        match v {
            SlotValue::Empty => Self::EMPTY,
            SlotValue::Hid(u) => Slot([0x00, 0, u, 0]),
            SlotValue::Consumer(c) => {
                let [lo, hi] = c.to_le_bytes();
                Slot([0x03, 0, lo, hi])
            }
            SlotValue::Macro { mode, index } => Slot([0x09, mode, index, 0]),
            SlotValue::Special(s) => Slot([0x0A, s, 0, 0]),
            SlotValue::Action { a, b } => Slot([0x0D, a, b, 0]),
            SlotValue::Profile { kind, index } => Slot([0x08, 0, kind, index]),
            SlotValue::OsMode(n) => Slot([0x0E, 0, n, 0]),
            SlotValue::Unknown(raw) => Slot(raw),
        }
    }

    /// A short human description.
    pub fn describe(self) -> String {
        match self.value() {
            SlotValue::Empty => "-".to_string(),
            SlotValue::Hid(u) => match keycode::name(u) {
                Some(n) => n.to_string(),
                None => format!("hid 0x{u:02X}"),
            },
            SlotValue::Consumer(c) => match keycode::consumer_name(c) {
                Some(n) => n.to_string(),
                None => format!("consumer 0x{c:04X}"),
            },
            SlotValue::Macro { mode, index } => {
                let m = ["repeat", "toggle", "hold"]
                    .get(mode as usize)
                    .copied()
                    .unwrap_or("?");
                format!("macro {index} ({m})")
            }
            SlotValue::Action { a, b } => action_name(a, b),
            SlotValue::Profile { kind, index } => match kind {
                // `index` comes off the board, so it can be 255: the sum must
                // not overflow (debug panics, release wraps to a wrong label).
                4 => format!("Profile {}", u16::from(index) + 1),
                3 => "Profile loop".to_string(),
                0 => "Profile value".to_string(),
                _ => format!("Profile {kind}:{index}"),
            },
            SlotValue::OsMode(n) => match n {
                0 => "Windows mode".to_string(),
                1 => "macOS mode".to_string(),
                2 => "Android mode".to_string(),
                3 => "iOS mode".to_string(),
                _ => format!("OS mode {n}"),
            },
            SlotValue::Special(s) => match s {
                1 => "Fn".to_string(),
                3 => "Lock Win Key".to_string(),
                12 => "Backlight on/off".to_string(),
                17 => "Caps LED".to_string(),
                // 8 sits on Space in the Fn layer and is named nowhere — not in
                // the vendor app, and its driver does not label it either. Show
                // nothing rather than a placeholder.
                8 => String::new(),
                _ => format!("Fn function {s}"),
            },
            SlotValue::Unknown(r) => {
                format!("raw {:02x} {:02x} {:02x} {:02x}", r[0], r[1], r[2], r[3])
            }
        }
    }
}

/// A whole layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    pub slots: [Slot; SLOTS],
}

impl Keymap {
    /// Parse a whole layer. Panics are not an option on data read from the
    /// board, so a short slice yields the slots it covers and leaves the rest
    /// empty rather than indexing out of bounds.
    pub fn from_bytes(b: &[u8]) -> Self {
        let mut slots = [Slot::EMPTY; SLOTS];
        for (i, slot) in slots.iter_mut().enumerate() {
            let at = i * 4;
            if at + 4 <= b.len() {
                slot.0.copy_from_slice(&b[at..at + 4]);
            }
        }
        Self { slots }
    }

    pub fn to_bytes(&self) -> [u8; LAYER_BYTES] {
        let mut out = [0u8; LAYER_BYTES];
        for (i, slot) in self.slots.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&slot.0);
        }
        out
    }

    /// The slot, or `None` when the index is past the layer. Callers pass an
    /// index parsed from `--slot` or from a profile's keymap table, so this
    /// must not be an indexing panic (`panic = "abort"` would kill the run).
    pub fn get(&self, slot: usize) -> Option<Slot> {
        self.slots.get(slot).copied()
    }

    /// Set a slot. Returns `false`, changing nothing, when the index is past
    /// the layer — see [`Self::get`].
    pub fn set(&mut self, slot: usize, v: Slot) -> bool {
        match self.slots.get_mut(slot) {
            Some(s) => {
                *s = v;
                true
            }
            None => false,
        }
    }

    /// Map a key name to the slot that currently holds it, for `--key NAME`.
    /// Derived from the layer itself, so it needs no board layout table.
    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        let want = keycode::usage(name)?;
        self.slots
            .iter()
            .position(|s| matches!(s.value(), SlotValue::Hid(u) if u == want))
    }
}

impl Session<'_> {
    /// Read a whole layer (`0x89` base, `0x90` Fn) as eight raw pages.
    pub fn read_layer(&mut self, profile: u8, fn_layer: bool) -> Result<Keymap> {
        // A raw page read takes whatever report is ready after the settle. When
        // the board is slower than that, a page read returns the *previous*
        // request's page, which silently mixes the two keymaps — the Fn layer's
        // values turning up on the base layer. Read twice and only trust a
        // pass that repeats; a lagging read differs between passes. **[HW]**
        let mut last = self.read_layer_once(profile, fn_layer)?;
        for _ in 0..LAYER_READ_TRIES {
            let again = self.read_layer_once(profile, fn_layer)?;
            if again == last {
                return Ok(again);
            }
            last = again;
        }
        // Never hand back a layer that did not confirm: callers write what they
        // read back (a bind is a read-modify-write of the whole layer), so an
        // unconfirmed read would persist a mixture of the base and Fn keymaps.
        Err(Error::UnstableLayer {
            tries: LAYER_READ_TRIES + 1,
        })
    }

    /// One read of every page of a layer, taken as it comes.
    fn read_layer_once(&mut self, profile: u8, fn_layer: bool) -> Result<Keymap> {
        let opcode = if fn_layer {
            cmd::GET_FN
        } else {
            cmd::GET_KEYMATRIX
        };
        let mut bytes = [0u8; LAYER_BYTES];
        for page in 0..PAGES {
            let reply = self.read_raw_page(opcode, &[profile, page], Checksum::Bit7)?;
            let start = page as usize * 64;
            bytes[start..start + 64].copy_from_slice(&reply);
        }
        Ok(Keymap::from_bytes(&bytes))
    }

    /// Write one slot (`0x13` base, `0x15` Fn). Fire-and-forget. Some yc500
    /// boards (including the LK67) ignore this and need [`Self::write_layer`].
    pub fn write_slot(&mut self, profile: u8, slot: u8, v: Slot, fn_layer: bool) -> Result<()> {
        if slot as usize >= SLOTS {
            return Err(Error::OutOfRange {
                field: "slot",
                value: i64::from(slot),
            });
        }
        let opcode = if fn_layer {
            cmd::SET_FN_ONE
        } else {
            cmd::SET_KEY_ONE
        };
        let mut payload = [0u8; 11];
        payload[0] = profile;
        payload[1] = slot;
        payload[7..11].copy_from_slice(&v.0);
        self.send(opcode, &payload, Checksum::Bit7)
    }

    /// Write a whole layer as nine 56-byte pages (`0x09` base, `0x10` Fn),
    /// committing on the last page. The caller passes the full layer
    /// (read-modify-write). Slots 126..127 never transmit. **[HW]**
    pub fn write_layer(&mut self, profile: u8, km: &Keymap, fn_layer: bool) -> Result<()> {
        let opcode = if fn_layer {
            cmd::SET_FN
        } else {
            cmd::SET_KEYMATRIX
        };
        let bytes = km.to_bytes();
        for page in 0..BULK_PAGES {
            let mut payload = [0u8; 63];
            payload[0] = profile;
            payload[1] = 0xF8;
            payload[2] = 1;
            payload[3] = page;
            let start = page as usize * BULK_PAGE_DATA;
            if start < LAYER_BYTES {
                let end = (start + BULK_PAGE_DATA).min(LAYER_BYTES);
                payload[7..7 + (end - start)].copy_from_slice(&bytes[start..end]);
            }
            self.send_page(opcode, &payload, Checksum::Bit7, page == BULK_PAGES - 1)?;
        }
        Ok(())
    }

    /// Factory reset (`0x02`). Bare opcode; the board needs ~4 s afterwards.
    /// Resets the whole keyboard, not just one layer.
    pub fn factory_reset(&mut self) -> Result<()> {
        self.send(cmd::SET_RESET, &[], Checksum::Bit7)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_values_round_trip() {
        for v in [
            SlotValue::Empty,
            SlotValue::Hid(0x04),
            SlotValue::Consumer(0x00E9),
            SlotValue::Macro { mode: 1, index: 5 },
            SlotValue::Special(1),
            SlotValue::Action { a: 2, b: 1 },
            SlotValue::Action { a: 3, b: 2 },
            SlotValue::Profile { kind: 4, index: 2 },
            SlotValue::OsMode(1),
            SlotValue::Unknown([0x42, 1, 2, 3]),
        ] {
            assert_eq!(Slot::from_value(v).value(), v, "round trip {v:?}");
        }
    }

    #[test]
    fn consumer_is_little_endian_in_bytes_2_and_3() {
        // Vol+ is [3, 0, 0xE9, 0].
        let s = Slot([0x03, 0x00, 0xE9, 0x00]);
        assert_eq!(s.value(), SlotValue::Consumer(0x00E9));
    }

    #[test]
    fn keymap_bytes_round_trip() {
        let mut k = Keymap {
            slots: [Slot::EMPTY; SLOTS],
        };
        k.set(0, Slot::from_value(SlotValue::Hid(0x04)));
        k.set(20, Slot::from_value(SlotValue::Hid(0x29)));
        let bytes = k.to_bytes();
        assert_eq!(Keymap::from_bytes(&bytes), k);
        assert_eq!(k.find_by_name("A"), Some(0));
        assert_eq!(k.find_by_name("Esc"), Some(20));
    }
}
