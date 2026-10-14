// SPDX-License-Identifier: GPL-3.0-or-later
//! HID keyboard/keypad usage ids and their names.
//!
//! Only the usages a keyboard uses: page `0x07` keyboard/keypad and a handful
//! of page `0x0C` consumer controls. Names are the ones people type; lookup is
//! case-insensitive and accepts a few aliases.

/// A HID usage id and its canonical name.
type Key = (u8, &'static str);

/// Page 0x07 (Keyboard/Keypad), `0x04..=0xE7`.
const KEYS: &[Key] = &[
    // Letters.
    (0x04, "A"),
    (0x05, "B"),
    (0x06, "C"),
    (0x07, "D"),
    (0x08, "E"),
    (0x09, "F"),
    (0x0A, "G"),
    (0x0B, "H"),
    (0x0C, "I"),
    (0x0D, "J"),
    (0x0E, "K"),
    (0x0F, "L"),
    (0x10, "M"),
    (0x11, "N"),
    (0x12, "O"),
    (0x13, "P"),
    (0x14, "Q"),
    (0x15, "R"),
    (0x16, "S"),
    (0x17, "T"),
    (0x18, "U"),
    (0x19, "V"),
    (0x1A, "W"),
    (0x1B, "X"),
    (0x1C, "Y"),
    (0x1D, "Z"),
    // Digits.
    (0x1E, "1"),
    (0x1F, "2"),
    (0x20, "3"),
    (0x21, "4"),
    (0x22, "5"),
    (0x23, "6"),
    (0x24, "7"),
    (0x25, "8"),
    (0x26, "9"),
    (0x27, "0"),
    // Editing / whitespace.
    (0x28, "Enter"),
    (0x29, "Esc"),
    (0x2A, "Backspace"),
    (0x2B, "Tab"),
    (0x2C, "Space"),
    (0x2D, "Minus"),
    (0x2E, "Equal"),
    (0x2F, "LeftBracket"),
    (0x30, "RightBracket"),
    (0x31, "Backslash"),
    (0x32, "NonUSHash"),
    (0x33, "Semicolon"),
    (0x34, "Quote"),
    (0x35, "Grave"),
    (0x36, "Comma"),
    (0x37, "Dot"),
    (0x38, "Slash"),
    (0x39, "CapsLock"),
    // Function keys.
    (0x3A, "F1"),
    (0x3B, "F2"),
    (0x3C, "F3"),
    (0x3D, "F4"),
    (0x3E, "F5"),
    (0x3F, "F6"),
    (0x40, "F7"),
    (0x41, "F8"),
    (0x42, "F9"),
    (0x43, "F10"),
    (0x44, "F11"),
    (0x45, "F12"),
    // System / navigation.
    (0x46, "PrintScreen"),
    (0x47, "ScrollLock"),
    (0x48, "Pause"),
    (0x49, "Insert"),
    (0x4A, "Home"),
    (0x4B, "PageUp"),
    (0x4C, "Delete"),
    (0x4D, "End"),
    (0x4E, "PageDown"),
    (0x4F, "Right"),
    (0x50, "Left"),
    (0x51, "Down"),
    (0x52, "Up"),
    // Keypad.
    (0x53, "NumLock"),
    (0x54, "KP/"),
    (0x55, "KP*"),
    (0x56, "KP-"),
    (0x57, "KP+"),
    (0x58, "KPEnter"),
    (0x59, "KP1"),
    (0x5A, "KP2"),
    (0x5B, "KP3"),
    (0x5C, "KP4"),
    (0x5D, "KP5"),
    (0x5E, "KP6"),
    (0x5F, "KP7"),
    (0x60, "KP8"),
    (0x61, "KP9"),
    (0x62, "KP0"),
    (0x63, "KP."),
    (0x64, "NonUSBackslash"),
    (0x65, "Application"),
    (0x66, "Power"),
    (0x67, "KP="),
    // Modifiers.
    (0xE0, "LeftCtrl"),
    (0xE1, "LeftShift"),
    (0xE2, "LeftAlt"),
    (0xE3, "LeftGUI"),
    (0xE4, "RightCtrl"),
    (0xE5, "RightShift"),
    (0xE6, "RightAlt"),
    (0xE7, "RightGUI"),
];

/// Common aliases accepted on input.
const ALIASES: &[(&str, u8)] = &[
    ("escape", 0x29),
    ("return", 0x28),
    ("backspace", 0x2A),
    ("space", 0x2C),
    ("caps", 0x39),
    ("capslock", 0x39),
    ("del", 0x4C),
    ("ins", 0x49),
    ("pgup", 0x4B),
    ("pgdn", 0x4E),
    ("ctrl", 0xE0),
    ("lctrl", 0xE0),
    ("rctrl", 0xE4),
    ("shift", 0xE1),
    ("lshift", 0xE1),
    ("rshift", 0xE5),
    ("alt", 0xE2),
    ("lalt", 0xE2),
    ("ralt", 0xE6),
    ("gui", 0xE3),
    ("win", 0xE3),
    ("meta", 0xE3),
    ("super", 0xE3),
    ("menu", 0x65),
    ("minus", 0x2D),
    ("equal", 0x2E),
    ("grave", 0x35),
    ("tilde", 0x35),
    ("comma", 0x36),
    ("dot", 0x37),
    ("period", 0x37),
    ("slash", 0x38),
    ("semicolon", 0x33),
    ("quote", 0x34),
    ("apostrophe", 0x34),
];

/// The canonical name for a HID usage, if known.
pub fn name(usage: u8) -> Option<&'static str> {
    KEYS.iter().find(|k| k.0 == usage).map(|k| k.1)
}

/// Resolve a name or alias (case-insensitive) to a HID usage.
pub fn usage(name: &str) -> Option<u8> {
    let want = name.trim();
    let lower = want.to_ascii_lowercase();
    if let Some(u) = want
        .strip_prefix("0x")
        .and_then(|h| u8::from_str_radix(h, 16).ok())
    {
        return Some(u);
    }
    if let Some((_, u)) = ALIASES.iter().find(|(a, _)| *a == lower) {
        return Some(*u);
    }
    KEYS.iter()
        .find(|k| k.1.to_ascii_lowercase() == lower)
        .map(|k| k.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_aliases_resolve() {
        assert_eq!(usage("A"), Some(0x04));
        assert_eq!(usage("a"), Some(0x04));
        assert_eq!(usage("Esc"), Some(0x29));
        assert_eq!(usage("escape"), Some(0x29));
        assert_eq!(usage("0x28"), Some(0x28));
        assert_eq!(usage("nope"), None);
    }

    #[test]
    fn names_round_trip() {
        for k in KEYS {
            assert_eq!(usage(k.1), Some(k.0), "name {}", k.1);
        }
    }
}
