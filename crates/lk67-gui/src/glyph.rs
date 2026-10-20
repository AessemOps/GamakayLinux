// SPDX-License-Identifier: GPL-3.0-or-later
//! Key legends as symbols where a conventional one exists, so the keyboard
//! previews read like real keycaps instead of spelling everything out.
//!
//! Each symbol carries the font family that actually has it: iced does not
//! fall back to the system fonts, so a glyph missing from the chosen family
//! renders as a tofu box. Everything here was checked against the installed
//! fonts (`fc-list :charset=…`).
//!

/// DejaVu Sans carries most of the symbols.
const DV: &str = "DejaVu Sans";
/// Noto Emoji (monochrome, bundled in `assets/`) carries the emoji: ⏸, 🔒, 🍎
/// and 🤖 — none of which is in DejaVu, and no packaged font here has them.
const NE: &str = "Noto Emoji";

/// The symbol and the font family that has it.
fn entry(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "Esc" => return None,                         // no glyph, stays text
        "Tab" => ("\u{21e5}", DV),                    // ⇥
        "Caps Lock" | "CapsLock" => ("\u{21ea}", DV), // ⇪
        "Shift" | "LeftShift" | "RightShift" => ("\u{21e7}", DV), // ⇧
        "Ctrl" | "LeftCtrl" | "RightCtrl" => ("\u{2303}", DV), // ⌃
        "Alt" | "LeftAlt" | "RightAlt" => ("\u{2387}", DV), // ⎇
        "Win" | "GUI" | "LeftGUI" | "RightGUI" => ("\u{1f427}", NE), // 🐧
        "Enter" => ("\u{23ce}", DV),                  // ⏎
        "Backspace" => ("\u{232b}", DV),              // ⌫
        "Space" => ("\u{2423}", DV),                  // ␣
        "Ins" | "Insert" => return None,              // no glyph in these fonts
        "Del" | "Delete" => ("\u{2326}", DV),         // ⌦
        "PgUp" | "PageUp" => ("\u{21de}", DV),        // ⇞
        "PgDn" | "PageDown" => ("\u{21df}", DV),      // ⇟
        "Home" => ("\u{21f1}", DV),                   // ⇱
        "End" => ("\u{21f2}", DV),                    // ⇲
        "Up" => ("\u{2191}", DV),                     // ↑
        "Down" => ("\u{2193}", DV),                   // ↓
        "Left" => ("\u{2190}", DV),                   // ←
        "Right" => ("\u{2192}", DV),                  // →
        "Menu" => ("\u{2630}", DV),                   // ☰
        "Pause" => ("\u{23f8}", NE),                  // ⏸ (not in DejaVu)
        "ScrollLock" => ("\u{21f3}", DV),             // ⇳
        "Eject" => ("\u{23cf}", DV),                  // ⏏
        "Play/Pause" => ("\u{25b6}", DV),             // ▶
        // The firmware's Fn functions.
        "Lock Win Key" => ("\u{1f512}", NE),     // 🔒
        "Backlight on/off" => ("\u{1f4a1}", NE), // 💡
        "Windows mode" => ("\u{229e}", DV),      // ⊞
        "macOS mode" => ("\u{2318}", DV),        // ⌘
        "Android mode" => ("\u{1f916}", NE),     // 🤖
        "iOS mode" => ("\u{1f34e}", NE),         // 🍎
        // Punctuation keeps its character, which is also what the cap shows.
        "Minus" => ("-", DV),
        "Equal" => ("=", DV),
        "LeftBracket" => ("[", DV),
        "RightBracket" => ("]", DV),
        "Backslash" => ("\\", DV),
        "NonUSHash" => ("#", DV),
        "Semicolon" => (";", DV),
        "Quote" => ("'", DV),
        "Grave" => ("`", DV),
        "Comma" => (",", DV),
        "Dot" | "Period" => (".", DV),
        "Slash" => ("/", DV),
        _ => return None,
    })
}

/// The symbol for a key legend, if there is a conventional one.
pub fn of(name: &str) -> Option<&'static str> {
    entry(name).map(|e| e.0)
}

/// The legend to draw: the symbol when there is one, else the name.
pub fn legend(name: &str) -> &str {
    of(name).unwrap_or(name)
}

/// The font to draw a legend in.
pub fn font(name: &str) -> iced::Font {
    let family = match entry(name) {
        Some((_, family)) => family,
        None => return iced::Font::DEFAULT,
    };
    iced::Font {
        family: iced::font::Family::Name(family),
        ..iced::Font::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meta_keys_share_the_penguin() {
        assert_eq!(of("Win"), Some("\u{1f427}"));
        // A meta cap and a meta binding compare equal, so only one is drawn.
        assert_eq!(legend("Win"), legend("LeftGUI"));
    }

    /// The bundled font must really have every glyph we point at it, or the
    /// key renders a tofu box — exactly how the small O key broke (`Pause`).
    #[test]
    fn the_bundled_font_has_every_glyph_we_ask_it_for() {
        const FONT: &[u8] = include_bytes!("../assets/NotoEmoji.ttf");
        let face = ttf_parser::Face::parse(FONT, 0).expect("the bundled font parses");
        let asked: Vec<u32> = [
            "Pause",
            "Lock Win Key",
            "Backlight on/off",
            "Android mode",
            "iOS mode",
            "Win",
        ]
        .iter()
        .filter_map(|n| entry(n))
        .filter(|(_, family)| *family == NE)
        .filter_map(|(sym, _)| sym.chars().next())
        .map(u32::from)
        .collect();
        assert!(
            asked.len() >= 4,
            "expected several emoji legends, got {asked:?}"
        );
        for cp in asked {
            let c = char::from_u32(cp).expect("bmp");
            assert!(
                face.glyph_index(c).is_some(),
                "U+{cp:04X} is missing from the bundled font"
            );
        }
    }

    #[test]
    fn symbols_come_from_a_font_that_has_them() {
        // ⏸ is not in DejaVu Sans; the small O key used to render a tofu box.
        assert_eq!(entry("Pause"), Some(("\u{23f8}", NE)));
        assert_eq!(entry("Lock Win Key"), Some(("\u{1f512}", NE)));
        assert_eq!(entry("Android mode"), Some(("\u{1f916}", NE)));
        assert_eq!(entry("iOS mode"), Some(("\u{1f34e}", NE)));
        assert_eq!(entry("Backspace"), Some(("\u{232b}", DV)));
        assert_eq!(entry("Windows mode"), Some(("\u{229e}", DV)));
        assert_eq!(entry("macOS mode"), Some(("\u{2318}", DV)));
    }

    #[test]
    fn names_without_a_glyph_in_the_fonts_stay_text() {
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
