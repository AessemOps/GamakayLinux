// SPDX-License-Identifier: GPL-3.0-or-later
//! Colour helpers shared by the wheel and the keyboard preview.

use iced::Color;

/// Parse `#rrggbb` / `rrggbb`.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let t = s.trim().trim_start_matches('#');
    // The slices below are taken at byte offsets, so reject anything that is
    // not six ASCII digits first: a six-*byte* non-ASCII string (say "€€")
    // would otherwise panic on a char boundary, and this parses a text field
    // as the user types.
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
/// `h` in degrees, `s` and `v` in 0..=1.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Color::from_rgb(r + m, g + m, b + m)
}

/// Hue (degrees) and saturation (0..=1) of an RGB colour.
pub fn rgb_to_hs(rgb: [u8; 3]) -> (f32, f32) {
    let r = f32::from(rgb[0]) / 255.0;
    let g = f32::from(rgb[1]) / 255.0;
    let b = f32::from(rgb[2]) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h.rem_euclid(360.0), s)
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

    /// Six *bytes* of non-ASCII must be refused, not panic: the parser slices
    /// at byte offsets and it runs on a text field as the user types, so a
    /// panic here aborts the whole application.
    #[test]
    fn non_ascii_is_refused() {
        assert_eq!(parse_hex("€€"), None); // 3 + 3 bytes
        assert_eq!(parse_hex("ééé"), None); // 2 + 2 + 2 bytes
        assert_eq!(parse_hex("#ééa"), None); // 5 bytes
    }
}
