// SPDX-License-Identifier: GPL-3.0-or-later
//! The macro recorder: turns a burst of keystrokes into a macro take.
//!
//! Kept out of the UI because the rules are easy to get wrong and matter:
//! a take must never end with a key still down (the board would then hold that
//! key every time the macro runs), auto-repeat must not duplicate a press, and
//! the take is stopped before it outgrows the 256-byte slot rather than failing
//! at save. Time is passed in, so all of this is testable without a board.

use std::time::{Duration, Instant};

use gamakay_core::macros::{MACRO_STREAM_MAX, MacroEvent};

/// A key held while recording.
#[derive(Debug, Clone)]
struct Held {
    /// The logical key the press reported. A release carries no physical key,
    /// so this plus the location is how the release finds its press again.
    key: iced::keyboard::Key,
    location: iced::keyboard::Location,
    /// The usage the press's physical position maps to.
    usage: u8,
}

/// A take being recorded.
#[derive(Debug, Default)]
pub struct Recorder {
    recording: bool,
    events: Vec<MacroEvent>,
    /// When the last event was captured, so each event carries the pause that
    /// preceded it.
    last: Option<Instant>,
    held: Vec<Held>,
    /// Why the take stopped, or a key that was skipped.
    note: String,
}

impl Recorder {
    /// Arm the recorder with a fresh take, timed from `now`.
    pub fn start(&mut self, now: Instant) {
        self.recording = true;
        self.events.clear();
        self.held.clear();
        self.note.clear();
        self.last = Some(now);
    }

    /// A key went down.
    ///
    /// `usage` is `None` when the board cannot send that key. Auto-repeat
    /// re-sends the press with no release in between, so a key already held is
    /// ignored and stays a single event — and, deliberately, does not advance
    /// the clock, so the release carries the whole time the key was held.
    pub fn down(
        &mut self,
        usage: Option<u8>,
        key: &iced::keyboard::Key,
        location: iced::keyboard::Location,
        now: Instant,
    ) {
        if !self.recording {
            return;
        }
        let Some(usage) = usage else {
            self.note = "that key cannot be sent by the board".into();
            return;
        };
        if self
            .held
            .iter()
            .any(|h| h.key == *key && h.location == location)
        {
            return;
        }
        self.held.push(Held {
            key: key.clone(),
            location,
            usage,
        });
        self.push(usage, true, now);
    }

    /// A key came up.
    pub fn up(
        &mut self,
        key: &iced::keyboard::Key,
        location: iced::keyboard::Location,
        now: Instant,
    ) {
        if !self.recording {
            return;
        }
        let Some(pos) = self
            .held
            .iter()
            .rposition(|h| h.key == *key && h.location == location)
        else {
            return;
        };
        let held = self.held.remove(pos);
        self.push(held.usage, false, now);
    }

    /// Stop, closing out any key still down.
    pub fn stop(&mut self, now: Instant) {
        if !self.recording {
            return;
        }
        self.recording = false;
        for held in std::mem::take(&mut self.held) {
            self.push(held.usage, false, now);
        }
        if self.events.is_empty() && self.note.is_empty() {
            self.note = "nothing was recorded".into();
        }
    }

    /// Discard the take.
    pub fn clear(&mut self) {
        self.recording = false;
        self.events.clear();
        self.held.clear();
        self.note.clear();
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn events(&self) -> &[MacroEvent] {
        &self.events
    }

    pub fn note(&self) -> &str {
        &self.note
    }

    /// Bytes of the slot the take occupies.
    pub fn used_bytes(&self) -> usize {
        self.events.iter().map(MacroEvent::encoded_len).sum()
    }

    /// Append one event, carrying the pause since the previous one.
    fn push(&mut self, usage: u8, pressed: bool, now: Instant) {
        // The pause before the first keystroke is the user reacting to having
        // pressed Record, not part of the macro: a macro that waited it out
        // would stall for seconds every time it ran.
        let paused = if self.events.is_empty() {
            Duration::ZERO
        } else {
            now.duration_since(self.last.unwrap_or(now))
        };
        self.last = Some(now);
        // A zero delay is encoded in the four-byte long form, so round it up:
        // back-to-back events are the common case and 1 ms is not observable.
        let delay_ms = (paused.as_millis().min(u128::from(u16::MAX)) as u16).max(1);
        let event = MacroEvent::Key {
            usage,
            pressed,
            delay_ms,
        };
        if self.used_bytes() + event.encoded_len() > MACRO_STREAM_MAX {
            // Stop now rather than let the user record a take the slot will
            // refuse to store.
            self.recording = false;
            self.held.clear();
            self.note = format!("macro full ({MACRO_STREAM_MAX} bytes)");
            return;
        }
        self.events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamakay_core::macros::Macro;

    fn key(name: &str) -> iced::keyboard::Key {
        iced::keyboard::Key::Character(name.into())
    }

    fn at(base: Instant, ms: u64) -> Instant {
        base + std::time::Duration::from_millis(ms)
    }

    fn take(r: &Recorder) -> Vec<MacroEvent> {
        r.events().to_vec()
    }

    #[test]
    fn a_take_pairs_a_press_with_its_release() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        r.down(
            Some(0x14),
            &key("q"),
            iced::keyboard::Location::Standard,
            at(t0, 5000),
        );
        r.up(&key("q"), iced::keyboard::Location::Standard, at(t0, 5050));
        assert_eq!(
            take(&r),
            vec![
                MacroEvent::Key {
                    usage: 0x14,
                    pressed: true,
                    delay_ms: 1,
                },
                MacroEvent::Key {
                    usage: 0x14,
                    pressed: false,
                    delay_ms: 50,
                },
            ]
        );
        assert_eq!(r.used_bytes(), 4);
    }

    /// Thinking time after pressing Record must not become a delay the macro
    /// waits out every time it runs.
    #[test]
    fn the_pause_before_the_first_key_is_not_recorded() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        r.down(
            Some(0x04),
            &key("a"),
            iced::keyboard::Location::Standard,
            at(t0, 30_000),
        );
        assert_eq!(
            take(&r),
            vec![MacroEvent::Key {
                usage: 0x04,
                pressed: true,
                delay_ms: 1,
            }]
        );
    }

    /// A modifier and a letter share a logical key only when left and right are
    /// distinguished, which is what the location is for.
    #[test]
    fn left_and_right_are_different_keys() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        let shift = iced::keyboard::Key::Named(iced::keyboard::key::Named::Shift);
        r.down(
            Some(0xE1),
            &shift,
            iced::keyboard::Location::Left,
            at(t0, 0),
        );
        r.down(
            Some(0xE5),
            &shift,
            iced::keyboard::Location::Right,
            at(t0, 0),
        );
        r.up(&shift, iced::keyboard::Location::Right, at(t0, 0));
        let usages: Vec<u8> = r
            .events()
            .iter()
            .map(|e| match e {
                MacroEvent::Key { usage, .. } => *usage,
                _ => 0,
            })
            .collect();
        assert_eq!(usages, vec![0xE1, 0xE5, 0xE5]);
    }

    /// A held key repeats as further presses; each must not add an event, and
    /// must not move the clock either — the release then carries the whole
    /// time the key was held, which is what a macro wants.
    #[test]
    fn auto_repeat_does_not_duplicate_a_press() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        for ms in [0, 30, 60, 90] {
            r.down(
                Some(0x04),
                &key("a"),
                iced::keyboard::Location::Standard,
                at(t0, ms),
            );
        }
        r.up(&key("a"), iced::keyboard::Location::Standard, at(t0, 120));
        assert_eq!(r.events().len(), 2);
        assert_eq!(
            take(&r)[1],
            MacroEvent::Key {
                usage: 0x04,
                pressed: false,
                delay_ms: 120,
            }
        );
    }

    /// The take must not end with a key down: the board would hold it every
    /// time the macro runs.
    #[test]
    fn stopping_closes_the_keys_still_held() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        r.down(
            Some(0xE1),
            &key("lshift"),
            iced::keyboard::Location::Left,
            at(t0, 0),
        );
        r.down(
            Some(0x04),
            &key("a"),
            iced::keyboard::Location::Standard,
            at(t0, 10),
        );
        r.stop(at(t0, 20));
        let presses: Vec<bool> = r
            .events()
            .iter()
            .map(|e| match e {
                MacroEvent::Key { pressed, .. } => *pressed,
                _ => false,
            })
            .collect();
        assert_eq!(presses, vec![true, true, false, false]);
        assert!(!r.is_recording());
    }

    #[test]
    fn a_release_without_a_press_is_ignored() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        r.up(&key("a"), iced::keyboard::Location::Standard, at(t0, 10));
        assert!(r.events().is_empty());
        assert!(r.note().is_empty());
    }

    #[test]
    fn a_key_the_board_cannot_send_is_refused() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        r.down(
            None,
            &key("f13"),
            iced::keyboard::Location::Standard,
            at(t0, 0),
        );
        assert!(r.events().is_empty());
        assert_eq!(r.note(), "that key cannot be sent by the board");
        // Recording carries on: one key being unsendable is not fatal.
        assert!(r.is_recording());
    }

    #[test]
    fn nothing_recorded_is_reported() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        r.stop(at(t0, 500));
        assert_eq!(r.note(), "nothing was recorded");
    }

    /// The recorder stops itself at the slot boundary, and what it kept is
    /// something the encoder accepts.
    #[test]
    fn the_take_stops_before_it_outgrows_the_slot() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        for i in 0..200u64 {
            let k = key(&format!("k{i}"));
            r.down(
                Some(0x04),
                &k,
                iced::keyboard::Location::Standard,
                at(t0, i * 10),
            );
            r.up(&k, iced::keyboard::Location::Standard, at(t0, i * 10));
        }
        assert!(!r.is_recording());
        assert_eq!(r.note(), format!("macro full ({MACRO_STREAM_MAX} bytes)"));
        assert!(r.used_bytes() <= MACRO_STREAM_MAX);
        let m = Macro {
            repeat: 1,
            events: take(&r),
        };
        assert!(m.to_blob().is_ok());
        assert_eq!(m.stream_len(), r.used_bytes());
    }

    #[test]
    fn clearing_drops_everything() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        r.start(t0);
        r.down(
            Some(0x04),
            &key("a"),
            iced::keyboard::Location::Standard,
            at(t0, 0),
        );
        r.stop(at(t0, 10));
        r.clear();
        assert!(r.events().is_empty());
        assert!(!r.is_recording());
        assert!(r.note().is_empty());
    }
}
