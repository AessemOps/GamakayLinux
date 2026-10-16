// SPDX-License-Identifier: GPL-3.0-or-later
//! Key legends as symbols where a conventional one exists, so the keyboard
//! previews read like real keycaps instead of spelling everything out.
//!
//! Only unambiguous names are mapped. `LeftShift`/`RightCtrl` and friends stay
//! as text, because the left/right distinction matters and there is no
//! conventional symbol for it.

/// The symbol for a key legend, if there is a conventional one.
pub fn of(name: &str) -> Option<&'static str> {
    Some(match name {
        "Tab" => "\u{21e5}",                    // ⇥
        "Caps Lock" | "CapsLock" => "\u{21ea}", // ⇪
        "Shift" => "\u{21e7}",                  // ⇧
        "Ctrl" => "\u{2303}",                   // ⌃
        "Alt" => "\u{2387}",                    // ⎇
        "Win" | "GUI" => "\u{229e}",            // ⊞
        "Enter" => "\u{23ce}",                  // ⏎
        "Backspace" => "\u{232b}",              // ⌫
        "Space" => "\u{2423}",                  // ␣
        "Del" | "Delete" => "\u{2326}",         // ⌦
        "PgUp" | "PageUp" => "\u{21de}",        // ⇞
        "PgDn" | "PageDown" => "\u{21df}",      // ⇟
        "Home" => "\u{21f1}",                   // ⇱
        "End" => "\u{21f2}",                    // ⇲
        "Up" => "\u{2191}",                     // ↑
        "Down" => "\u{2193}",                   // ↓
        "Left" => "\u{2190}",                   // ←
        "Right" => "\u{2192}",                  // →
        "Menu" => "\u{2630}",                   // ☰
        "Pause" => "\u{23f8}",                  // ⏸
        "ScrollLock" => "\u{21f3}",             // ⇳
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
    fn unambiguous_names_are_mapped_and_lr_are_not() {
        assert_eq!(of("Backspace"), Some("\u{232b}"));
        assert_eq!(of("Up"), Some("\u{2191}"));
        assert_eq!(of("Shift"), Some("\u{21e7}"));
        // The left/right variants keep their text.
        assert_eq!(of("LeftShift"), None);
        assert_eq!(of("RightCtrl"), None);
        // Unknown names fall back to themselves.
        assert_eq!(legend("A"), "A");
        assert_eq!(legend("LeftShift"), "LeftShift");
    }
}
