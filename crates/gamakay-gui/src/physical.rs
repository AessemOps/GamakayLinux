// SPDX-License-Identifier: GPL-3.0-or-later
//! Map a *physical* key to the HID usage the board would send.
//!
//! The LK67 is a standard HID keyboard: a slot holds the usage for the key's
//! *position*, and the OS layout turns that into a character. Binding the
//! physical code therefore binds whatever key the user pressed, independent of
//! their layout — the board here has AZERTY caps but QWERTY firmware, so its
//! `A` cap sits at the US `Q` position and sends `0x14`.
//!
//! Kept separate from `gamakay_core::keycode` (which is HID <-> name) because
//! this depends on iced's key codes.

use iced::keyboard::key::{Code, Physical};

/// The HID keyboard usage for a physical key, if the board can send it.
pub fn usage(physical: &Physical) -> Option<u8> {
    let Physical::Code(code) = physical else {
        return None;
    };
    Some(match code {
        // Letters.
        Code::KeyA => 0x04,
        Code::KeyB => 0x05,
        Code::KeyC => 0x06,
        Code::KeyD => 0x07,
        Code::KeyE => 0x08,
        Code::KeyF => 0x09,
        Code::KeyG => 0x0A,
        Code::KeyH => 0x0B,
        Code::KeyI => 0x0C,
        Code::KeyJ => 0x0D,
        Code::KeyK => 0x0E,
        Code::KeyL => 0x0F,
        Code::KeyM => 0x10,
        Code::KeyN => 0x11,
        Code::KeyO => 0x12,
        Code::KeyP => 0x13,
        Code::KeyQ => 0x14,
        Code::KeyR => 0x15,
        Code::KeyS => 0x16,
        Code::KeyT => 0x17,
        Code::KeyU => 0x18,
        Code::KeyV => 0x19,
        Code::KeyW => 0x1A,
        Code::KeyX => 0x1B,
        Code::KeyY => 0x1C,
        Code::KeyZ => 0x1D,
        // Digits.
        Code::Digit1 => 0x1E,
        Code::Digit2 => 0x1F,
        Code::Digit3 => 0x20,
        Code::Digit4 => 0x21,
        Code::Digit5 => 0x22,
        Code::Digit6 => 0x23,
        Code::Digit7 => 0x24,
        Code::Digit8 => 0x25,
        Code::Digit9 => 0x26,
        Code::Digit0 => 0x27,
        // Editing / whitespace.
        Code::Enter => 0x28,
        Code::Escape => 0x29,
        Code::Backspace => 0x2A,
        Code::Tab => 0x2B,
        Code::Space => 0x2C,
        Code::Minus => 0x2D,
        Code::Equal => 0x2E,
        Code::BracketLeft => 0x2F,
        Code::BracketRight => 0x30,
        Code::Backslash => 0x31,
        Code::Semicolon => 0x33,
        Code::Quote => 0x34,
        Code::Backquote => 0x35,
        Code::Comma => 0x36,
        Code::Period => 0x37,
        Code::Slash => 0x38,
        Code::CapsLock => 0x39,
        // Function keys.
        Code::F1 => 0x3A,
        Code::F2 => 0x3B,
        Code::F3 => 0x3C,
        Code::F4 => 0x3D,
        Code::F5 => 0x3E,
        Code::F6 => 0x3F,
        Code::F7 => 0x40,
        Code::F8 => 0x41,
        Code::F9 => 0x42,
        Code::F10 => 0x43,
        Code::F11 => 0x44,
        Code::F12 => 0x45,
        // System / navigation.
        Code::PrintScreen => 0x46,
        Code::ScrollLock => 0x47,
        Code::Pause => 0x48,
        Code::Insert => 0x49,
        Code::Home => 0x4A,
        Code::PageUp => 0x4B,
        Code::Delete => 0x4C,
        Code::End => 0x4D,
        Code::PageDown => 0x4E,
        Code::ArrowRight => 0x4F,
        Code::ArrowLeft => 0x50,
        Code::ArrowDown => 0x51,
        Code::ArrowUp => 0x52,
        // Keypad.
        Code::NumLock => 0x53,
        Code::NumpadDivide => 0x54,
        Code::NumpadMultiply | Code::NumpadStar => 0x55,
        Code::NumpadSubtract => 0x56,
        Code::NumpadAdd => 0x57,
        Code::NumpadEnter => 0x58,
        Code::Numpad1 => 0x59,
        Code::Numpad2 => 0x5A,
        Code::Numpad3 => 0x5B,
        Code::Numpad4 => 0x5C,
        Code::Numpad5 => 0x5D,
        Code::Numpad6 => 0x5E,
        Code::Numpad7 => 0x5F,
        Code::Numpad8 => 0x60,
        Code::Numpad9 => 0x61,
        Code::Numpad0 => 0x62,
        Code::NumpadDecimal => 0x63,
        Code::IntlBackslash => 0x64,
        Code::ContextMenu => 0x65,
        Code::NumpadEqual => 0x67,
        // Modifiers.
        Code::ControlLeft => 0xE0,
        Code::ShiftLeft => 0xE1,
        Code::AltLeft => 0xE2,
        Code::SuperLeft | Code::Meta => 0xE3,
        Code::ControlRight => 0xE4,
        Code::ShiftRight => 0xE5,
        Code::AltRight => 0xE6,
        Code::SuperRight => 0xE7,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamakay_core::keycode;

    /// Every usage this module can produce must be a nameable key, so the
    /// canvas and the slot list show something sensible.
    #[test]
    fn mapped_usages_are_known_names() {
        for code in [
            Code::KeyA,
            Code::KeyQ,
            Code::Digit1,
            Code::Escape,
            Code::Enter,
            Code::Space,
            Code::Slash,
            Code::F5,
            Code::ArrowUp,
            Code::Numpad7,
            Code::ShiftRight,
            Code::SuperLeft,
        ] {
            let u = usage(&Physical::Code(code)).expect("mapped");
            assert!(keycode::name(u).is_some(), "0x{u:02X} has no name");
        }
    }

    /// The AZERTY `A` cap sits at the US `Q` position: pressing it must bind
    /// `Q` (0x14), which the OS layout then renders as `a`.
    #[test]
    fn physical_position_is_used_not_the_character() {
        assert_eq!(usage(&Physical::Code(Code::KeyQ)), Some(0x14));
        assert_eq!(keycode::name(0x14), Some("Q"));
    }
}
