// SPDX-License-Identifier: GPL-3.0-or-later
//! The LK67 physical layout: where each key sits and which keymap slot drives
//! it. Coordinates are in key units (1u = one standard key), `y` is the row.
//!
//! Derived from a photo of the board and the slot->key mapping read from the
//! default keymap (`docs/protocol.md`). The GUI draws this; the CLI ignores it.

/// One physical key.
#[derive(Clone, Copy, Debug)]
pub struct LayoutKey {
    /// The legend printed on the key (its default function).
    pub label: &'static str,
    /// The keymap slot this key is wired to. 0 for the decorative knob.
    pub slot: u8,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The rotary encoder, drawn as a circle and not mapped to a slot.
    pub knob: bool,
}

impl LayoutKey {
    const fn k(label: &'static str, slot: u8, x: f32, y: f32, w: f32) -> Self {
        Self {
            label,
            slot,
            x,
            y,
            w,
            h: 1.0,
            knob: false,
        }
    }
}

/// Board width in key units.
pub const LK67_WIDTH: f32 = 17.0;
/// Board height in key units.
pub const LK67_HEIGHT: f32 = 5.0;

/// The LK67, a 65% board: five rows plus a right column and an arrow cluster.
pub const LK67_LAYOUT: &[LayoutKey] = &[
    // Row 0 — number row.
    LayoutKey::k("Esc", 1, 0.0, 0.0, 1.0),
    LayoutKey::k("1", 7, 1.0, 0.0, 1.0),
    LayoutKey::k("2", 13, 2.0, 0.0, 1.0),
    LayoutKey::k("3", 19, 3.0, 0.0, 1.0),
    LayoutKey::k("4", 25, 4.0, 0.0, 1.0),
    LayoutKey::k("5", 31, 5.0, 0.0, 1.0),
    LayoutKey::k("6", 37, 6.0, 0.0, 1.0),
    LayoutKey::k("7", 43, 7.0, 0.0, 1.0),
    LayoutKey::k("8", 49, 8.0, 0.0, 1.0),
    LayoutKey::k("9", 55, 9.0, 0.0, 1.0),
    LayoutKey::k("0", 61, 10.0, 0.0, 1.0),
    LayoutKey::k("-", 67, 11.0, 0.0, 1.0),
    LayoutKey::k("=", 73, 12.0, 0.0, 1.0),
    LayoutKey::k("Backspace", 79, 13.0, 0.0, 2.0),
    // Row 1.
    LayoutKey::k("Tab", 2, 0.0, 1.0, 1.5),
    LayoutKey::k("Q", 8, 1.5, 1.0, 1.0),
    LayoutKey::k("W", 14, 2.5, 1.0, 1.0),
    LayoutKey::k("E", 20, 3.5, 1.0, 1.0),
    LayoutKey::k("R", 26, 4.5, 1.0, 1.0),
    LayoutKey::k("T", 32, 5.5, 1.0, 1.0),
    LayoutKey::k("Y", 38, 6.5, 1.0, 1.0),
    LayoutKey::k("U", 44, 7.5, 1.0, 1.0),
    LayoutKey::k("I", 50, 8.5, 1.0, 1.0),
    LayoutKey::k("O", 56, 9.5, 1.0, 1.0),
    LayoutKey::k("P", 62, 10.5, 1.0, 1.0),
    LayoutKey::k("[", 68, 11.5, 1.0, 1.0),
    LayoutKey::k("]", 74, 12.5, 1.0, 1.0),
    LayoutKey::k("\\", 80, 13.5, 1.0, 1.5),
    LayoutKey::k("PgUp", 86, 16.0, 1.0, 1.0),
    // Row 2.
    LayoutKey::k("Caps Lock", 3, 0.0, 2.0, 1.75),
    LayoutKey::k("A", 9, 1.75, 2.0, 1.0),
    LayoutKey::k("S", 15, 2.75, 2.0, 1.0),
    LayoutKey::k("D", 21, 3.75, 2.0, 1.0),
    LayoutKey::k("F", 27, 4.75, 2.0, 1.0),
    LayoutKey::k("G", 33, 5.75, 2.0, 1.0),
    LayoutKey::k("H", 39, 6.75, 2.0, 1.0),
    LayoutKey::k("J", 45, 7.75, 2.0, 1.0),
    LayoutKey::k("K", 51, 8.75, 2.0, 1.0),
    LayoutKey::k("L", 57, 9.75, 2.0, 1.0),
    LayoutKey::k(";", 63, 10.75, 2.0, 1.0),
    LayoutKey::k("'", 69, 11.75, 2.0, 1.0),
    LayoutKey::k("Enter", 81, 12.75, 2.0, 2.25),
    LayoutKey::k("Del", 85, 16.0, 2.0, 1.0),
    // Row 3.
    LayoutKey::k("Shift", 4, 0.0, 3.0, 2.25),
    LayoutKey::k("Z", 16, 2.25, 3.0, 1.0),
    LayoutKey::k("X", 22, 3.25, 3.0, 1.0),
    LayoutKey::k("C", 28, 4.25, 3.0, 1.0),
    LayoutKey::k("V", 34, 5.25, 3.0, 1.0),
    LayoutKey::k("B", 40, 6.25, 3.0, 1.0),
    LayoutKey::k("N", 46, 7.25, 3.0, 1.0),
    LayoutKey::k("M", 52, 8.25, 3.0, 1.0),
    LayoutKey::k(",", 58, 9.25, 3.0, 1.0),
    LayoutKey::k(".", 64, 10.25, 3.0, 1.0),
    LayoutKey::k("/", 70, 11.25, 3.0, 1.25),
    LayoutKey::k("Shift", 76, 12.5, 3.0, 1.25),
    LayoutKey::k("PgDn", 87, 16.0, 3.0, 1.0),
    LayoutKey::k("Up", 82, 15.0, 3.0, 1.0),
    // Row 4.
    LayoutKey::k("Ctrl", 5, 0.0, 4.0, 1.25),
    LayoutKey::k("Win", 11, 1.25, 4.0, 1.25),
    LayoutKey::k("Alt", 17, 2.5, 4.0, 1.25),
    LayoutKey::k("Space", 41, 3.75, 4.0, 6.25),
    LayoutKey::k("Alt", 65, 10.0, 4.0, 1.25),
    LayoutKey::k("Fn", 71, 11.25, 4.0, 1.25),
    LayoutKey::k("Ctrl", 77, 12.5, 4.0, 1.25),
    LayoutKey::k("Left", 83, 14.0, 4.0, 1.0),
    LayoutKey::k("Down", 88, 15.0, 4.0, 1.0),
    LayoutKey::k("Right", 89, 16.0, 4.0, 1.0),
    // The rotary knob (decorative).
    LayoutKey {
        label: "",
        slot: 0,
        x: 16.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
        knob: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_has_no_duplicate_slots() {
        let mut seen = std::collections::HashSet::new();
        for k in LK67_LAYOUT.iter().filter(|k| !k.knob) {
            assert!(
                seen.insert(k.slot),
                "slot {} used twice ({})",
                k.slot,
                k.label
            );
        }
    }

    #[test]
    fn layout_keys_are_within_the_board() {
        for k in LK67_LAYOUT {
            assert!(
                k.x + k.w <= LK67_WIDTH + 0.01,
                "{} overflows width",
                k.label
            );
            assert!(
                k.y + k.h <= LK67_HEIGHT + 0.01,
                "{} overflows height",
                k.label
            );
        }
    }
}
