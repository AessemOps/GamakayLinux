// SPDX-License-Identifier: GPL-3.0-or-later
//! The key picker: every key the board can send, grouped into the same kind of
//! categories VIA uses, including keys that are not on the LK67 (function row,
//! keypad, navigation, media).

use gamakay_core::keymap::SlotValue;

/// One clickable picker entry: the button label and the slot value it writes.
pub type Entry = (&'static str, SlotValue);

/// A HID keyboard/keypad usage.
const fn hid(label: &'static str, usage: u8) -> Entry {
    (label, SlotValue::Hid(usage))
}

/// A consumer (media) usage, page 0x0C.
const fn con(label: &'static str, usage: u16) -> Entry {
    (label, SlotValue::Consumer(usage))
}

pub const BASIC: &[Entry] = &[
    hid("Esc", 0x29),
    hid("Tab", 0x2B),
    hid("Caps", 0x39),
    hid("Enter", 0x28),
    hid("Space", 0x2C),
    hid("Bksp", 0x2A),
    hid("`", 0x35),
    hid("1", 0x1E),
    hid("2", 0x1F),
    hid("3", 0x20),
    hid("4", 0x21),
    hid("5", 0x22),
    hid("6", 0x23),
    hid("7", 0x24),
    hid("8", 0x25),
    hid("9", 0x26),
    hid("0", 0x27),
    hid("-", 0x2D),
    hid("=", 0x2E),
    hid("A", 0x04),
    hid("B", 0x05),
    hid("C", 0x06),
    hid("D", 0x07),
    hid("E", 0x08),
    hid("F", 0x09),
    hid("G", 0x0A),
    hid("H", 0x0B),
    hid("I", 0x0C),
    hid("J", 0x0D),
    hid("K", 0x0E),
    hid("L", 0x0F),
    hid("M", 0x10),
    hid("N", 0x11),
    hid("O", 0x12),
    hid("P", 0x13),
    hid("Q", 0x14),
    hid("R", 0x15),
    hid("S", 0x16),
    hid("T", 0x17),
    hid("U", 0x18),
    hid("V", 0x19),
    hid("W", 0x1A),
    hid("X", 0x1B),
    hid("Y", 0x1C),
    hid("Z", 0x1D),
    hid("[", 0x2F),
    hid("]", 0x30),
    hid("\\", 0x31),
    hid(";", 0x33),
    hid("'", 0x34),
    hid(",", 0x36),
    hid(".", 0x37),
    hid("/", 0x38),
];

pub const FUNCTION: &[Entry] = &[
    hid("F1", 0x3A),
    hid("F2", 0x3B),
    hid("F3", 0x3C),
    hid("F4", 0x3D),
    hid("F5", 0x3E),
    hid("F6", 0x3F),
    hid("F7", 0x40),
    hid("F8", 0x41),
    hid("F9", 0x42),
    hid("F10", 0x43),
    hid("F11", 0x44),
    hid("F12", 0x45),
];

pub const NAVIGATION: &[Entry] = &[
    hid("PrtSc", 0x46),
    hid("ScrLk", 0x47),
    hid("Pause", 0x48),
    hid("Ins", 0x49),
    hid("Home", 0x4A),
    hid("PgUp", 0x4B),
    hid("Del", 0x4C),
    hid("End", 0x4D),
    hid("PgDn", 0x4E),
    hid("→", 0x4F),
    hid("←", 0x50),
    hid("↓", 0x51),
    hid("↑", 0x52),
    hid("Menu", 0x65),
];

pub const KEYPAD: &[Entry] = &[
    hid("Num", 0x53),
    hid("KP /", 0x54),
    hid("KP *", 0x55),
    hid("KP -", 0x56),
    hid("KP +", 0x57),
    hid("KP ⏎", 0x58),
    hid("KP 1", 0x59),
    hid("KP 2", 0x5A),
    hid("KP 3", 0x5B),
    hid("KP 4", 0x5C),
    hid("KP 5", 0x5D),
    hid("KP 6", 0x5E),
    hid("KP 7", 0x5F),
    hid("KP 8", 0x60),
    hid("KP 9", 0x61),
    hid("KP 0", 0x62),
    hid("KP .", 0x63),
    hid("KP =", 0x67),
];

pub const MODIFIERS: &[Entry] = &[
    hid("L Ctrl", 0xE0),
    hid("L Shift", 0xE1),
    hid("L Alt", 0xE2),
    hid("L GUI", 0xE3),
    hid("R Ctrl", 0xE4),
    hid("R Shift", 0xE5),
    hid("R Alt", 0xE6),
    hid("R GUI", 0xE7),
];

pub const MEDIA: &[Entry] = &[
    con("Vol-", 0x00EA),
    con("Vol+", 0x00E9),
    con("Mute", 0x00E2),
    // 0x00CD is Play/Pause, not plain Play: the label has to say so, since the
    // slot list and the legend are rendered from `consumer_name`.
    con("Play/Pause", 0x00CD),
    con("Next", 0x00B5),
    con("Prev", 0x00B6),
];

pub const SPECIAL: &[Entry] = &[
    ("Fn", SlotValue::Special(1)),
    ("Caps LED", SlotValue::Special(17)),
    // Named to match `Slot::describe`, which is what the slot list and the
    // keymap legend show for this value.
    ("Backlight on/off", SlotValue::Special(12)),
    ("None", SlotValue::Empty),
];

/// The picker, in display order.
pub const CATEGORIES: &[(&str, &[Entry])] = &[
    ("Basic", BASIC),
    ("Function", FUNCTION),
    ("Navigation", NAVIGATION),
    ("Keypad", KEYPAD),
    ("Modifiers", MODIFIERS),
    ("Media", MEDIA),
    ("Special", SPECIAL),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every HID entry must resolve to a name, so the slot list reads well and
    /// the picker matches `gamakay keymap set --key`.
    #[test]
    fn hid_entries_have_names() {
        for (_, entries) in CATEGORIES {
            for (label, value) in *entries {
                if let SlotValue::Hid(u) = value {
                    assert!(
                        gamakay_core::keycode::name(*u).is_some(),
                        "picker {label} -> 0x{u:02X} has no name"
                    );
                }
            }
        }
    }

    /// The picker holds its own copy of the HID usage table, so a renumbering
    /// in `gamakay_core::keycode` would have it silently bind a different key.
    /// Where the label is a name core knows, insist the two agree; and in every
    /// case insist the usage round-trips through core's name table.
    #[test]
    fn entries_agree_with_the_core_usage_table() {
        for (_, entries) in CATEGORIES {
            for (label, value) in *entries {
                match value {
                    SlotValue::Hid(u) => {
                        if let Some(canonical) = gamakay_core::keycode::usage(label) {
                            assert_eq!(
                                canonical, *u,
                                "picker {label} binds 0x{u:02X}, core says 0x{canonical:02X}"
                            );
                        }
                        let name = gamakay_core::keycode::name(*u).expect("named");
                        assert_eq!(
                            gamakay_core::keycode::usage(name),
                            Some(*u),
                            "0x{u:02X} ({name}) does not round-trip"
                        );
                    }
                    SlotValue::Consumer(c) => {
                        let name = gamakay_core::keycode::consumer_name(*c)
                            .unwrap_or_else(|| panic!("picker {label} has no consumer name"));
                        assert_eq!(
                            name, *label,
                            "picker label {label:?} does not match consumer name {name:?}"
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    /// No usage is offered twice.
    #[test]
    fn entries_are_unique() {
        let mut seen: Vec<SlotValue> = Vec::new();
        for (_, entries) in CATEGORIES {
            for (label, value) in *entries {
                assert!(!seen.contains(value), "duplicate picker entry {label}");
                seen.push(*value);
            }
        }
    }
}
