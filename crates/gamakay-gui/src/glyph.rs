// SPDX-License-Identifier: GPL-3.0-or-later
//! Key legends as symbols where a conventional one exists, so the keyboard
//! previews read like real keycaps instead of spelling everything out.
//!
//! A left/right pair maps to the same symbol — as on a real keycap, where the
//! key's position says which one it is.

/// The symbol for a key legend, if there is a conventional one.
pub fn of(name: &str) -> Option<&'static str> {
    Some(match name {
        "Tab" => "\u{21e5}",                                  // ⇥
        "Caps Lock" | "CapsLock" => "\u{21ea}",               // ⇪
        "Shift" | "LeftShift" | "RightShift" => "\u{21e7}",   // ⇧
        "Ctrl" | "LeftCtrl" | "RightCtrl" => "\u{2303}",      // ⌃
        "Alt" | "LeftAlt" | "RightAlt" => "\u{2387}",         // ⎇
        "Win" | "GUI" | "LeftGUI" | "RightGUI" => "\u{229e}", // ⊞
        "Enter" => "\u{23ce}",                                // ⏎
        "Backspace" => "\u{232b}",                            // ⌫
        "Space" => "\u{2423}",                                // ␣
        "Del" | "Delete" => "\u{2326}",                       // ⌦
        "PgUp" | "PageUp" => "\u{21de}",                      // ⇞
        "PgDn" | "PageDown" => "\u{21df}",                    // ⇟
        "Home" => "\u{21f1}",                                 // ⇱
        "End" => "\u{21f2}",                                  // ⇲
        "Up" => "\u{2191}",                                   // ↑
        "Down" => "\u{2193}",                                 // ↓
        "Left" => "\u{2190}",                                 // ←
        "Right" => "\u{2192}",                                // →
        "Menu" => "\u{2630}",                                 // ☰
        "Play/Pause" => "\u{25b6}",                           // ▶
        "Eject" => "\u{23cf}",                                // ⏏
        "Pause" => "\u{23f8}",                                // ⏸
        "ScrollLock" => "\u{21f3}",                           // ⇳
        // Punctuation keeps its character, which is also what the cap shows.
        "Minus" => "-",
        "Equal" => "=",
        "LeftBracket" => "[",
        "RightBracket" => "]",
        "Backslash" => "\\",
        "NonUSHash" => "#",
        "Semicolon" => ";",
        "Quote" => "'",
        "Grave" => "`",
        "Comma" => ",",
        "Dot" | "Period" => ".",
        "Slash" => "/",
        _ => return None,
    })
}

/// The legend to draw: the symbol when there is one, else the name.
pub fn legend(name: &str) -> &str {
    of(name).unwrap_or(name)
}

/// The font to draw a legend in. iced's default font has the arrows but not
/// the technical symbols (they came out as tofu), and it does not fall back to
/// the system fonts, so symbol legends are drawn in DejaVu Sans, which has the
/// whole set.
pub fn font(name: &str) -> iced::Font {
    if of(name).is_some() {
        iced::Font {
            family: iced::font::Family::Name("DejaVu Sans"),
            ..iced::Font::DEFAULT
        }
    } else {
        iced::Font::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_without_a_glyph_in_the_font_stay_text() {
        // DejaVu Sans has no ⎋ / ⎀ / ⎙, so these keep their text legend.
        assert_eq!(of("Esc"), None);
        assert_eq!(of("Insert"), None);
        assert_eq!(of("PrintScreen"), None);
    }

    #[test]
    fn names_are_mapped_and_unknown_ones_fall_back() {
        assert_eq!(of("Backspace"), Some("\u{232b}"));
        assert_eq!(of("Up"), Some("\u{2191}"));
        // A left/right pair shares the glyph; the position says which it is.
        assert_eq!(of("Shift"), of("LeftShift"));
        assert_eq!(of("LeftShift"), of("RightShift"));
        // Punctuation shows its character.
        assert_eq!(of("Semicolon"), Some(";"));
        assert_eq!(of("Comma"), Some(","));
        assert_eq!(of("Backslash"), Some("\\"));
        // Unknown names fall back to themselves.
        assert_eq!(legend("A"), "A");
        assert_eq!(legend("LeftShift"), "\u{21e7}");
    }
}
