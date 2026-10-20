// SPDX-License-Identifier: GPL-3.0-or-later
//! `#rrggbb` parsing and formatting, shared by both front-ends.
//!
//! There is one implementation on purpose: this parses a text field as the user
//! types and an argument straight off a command line, and a second copy is a
//! second place to get the non-ASCII case wrong.

/// Parse `#rrggbb` / `rrggbb`.
///
/// The slices below are taken at byte offsets, so anything that is not six
/// ASCII digits is rejected first: a six-*byte* non-ASCII string (say "€€")
/// would otherwise panic on a char boundary.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let t = s.trim().trim_start_matches('#');
    if t.len() != 6 || !t.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// Format as `#rrggbb`.
pub fn to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        assert_eq!(parse_hex("#ff0000"), Some([0xff, 0x00, 0x00]));
        assert_eq!(parse_hex(" 00ff80 "), Some([0x00, 0xff, 0x80]));
        assert_eq!(to_hex([0x00, 0xff, 0x80]), "#00ff80");
        assert_eq!(parse_hex("#ff00"), None);
        assert_eq!(parse_hex("#gggggg"), None);
    }

    /// Six *bytes* of non-ASCII must be refused, not panic: this runs on a text
    /// field as the user types, and a panic here aborts the whole application.
    #[test]
    fn non_ascii_is_refused() {
        assert_eq!(parse_hex("€€"), None);
        assert_eq!(parse_hex("ééé"), None);
        assert_eq!(parse_hex("#ééa"), None);
    }
}
