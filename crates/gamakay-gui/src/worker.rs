// SPDX-License-Identifier: GPL-3.0-or-later
//! The device worker: all blocking hidraw I/O runs on this thread, so the UI
//! never blocks. The UI sends [`Request`]s and receives [`Event`]s.

use std::sync::mpsc::Receiver;

use iced::futures::channel::mpsc::UnboundedSender;

use gamakay_core::error::Error as CoreError;
use gamakay_core::guard::Guard;
use gamakay_core::keymap::{Keymap, Slot, SlotValue};
use gamakay_core::light::{LK67_FLAGS, LedParam, LedState, Mode};
use gamakay_core::macros::{MACRO_SLOTS, Macro, MacroEvent, describe};
use gamakay_core::transport::Session;
use gamakay_hidraw::{HidrawDevice, HidrawInfo};

/// A request from the UI to the worker.
pub enum Request {
    Connect,
    /// Make `profile` the active one (the keymap follows it).
    SetLayer(u8),
    /// Re-read just one keymap layer. Switching layers only needs this: the
    /// full connect path also reads the summary, the backlight and 50 macro
    /// slots, and that burst is enough feature traffic to stall the board's
    /// control endpoint (which then needs a replug).
    ReadLayer(bool),
    SetBacklight(LedParam),
    SetBacklightOff(bool),
    SetSlot {
        fn_layer: bool,
        slot: u8,
        value: SlotValue,
    },
    SetMacro {
        slot: u8,
        repeat: u16,
        events: Vec<MacroEvent>,
    },
    DeleteMacro(u8),
    /// Read the 50 macro slots. Sent when the Macros tab is opened rather than
    /// on connect: it is 200 feature transfers, and the connect path must stay
    /// small or the board's control endpoint stalls and needs a replug.
    ReadMacros,
}

/// A summary of the attached board.
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub name: String,
    pub usb: String,
    pub link: String,
    pub id: u32,
    pub model: String,
    pub firmware: Option<String>,
    pub profile: u8,
    /// Keymap layers the board exposes (its profile count).
    pub layers: u8,
}

/// An event from the worker to the UI.
#[derive(Debug, Clone)]
pub enum Event {
    Connected(Summary),
    Disconnected,
    Backlight(LedState),
    /// The decoded non-empty slots of a layer, plus the slots the firmware
    /// runs itself (which therefore cannot be repurposed).
    Layer {
        fn_layer: bool,
        slots: Vec<(u8, String)>,
        locked: Vec<u8>,
    },
    Macros(Vec<(u8, String)>),
    Status(String),
    Error(String),
}

/// Start the worker thread. Returns the sender the UI uses for requests.
pub fn spawn(events: UnboundedSender<Event>) -> std::sync::mpsc::Sender<Request> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("gamakay-io".into())
        .spawn(move || Worker::new(events).run(rx))
        .expect("spawn worker");
    tx
}

struct Worker {
    events: UnboundedSender<Event>,
    device: Option<(HidrawDevice, HidrawInfo)>,
    /// Fn-layer slots the firmware drives (from the identified record).
    fn_locked: &'static [u8],
    /// The macros read from the board, kept so a write can update the list
    /// without re-reading all 50 slots (200 feature transfers).
    macros: Vec<(u8, String)>,
    /// Whether the identified board is the one this build targets. Writes are
    /// refused until it is: the USB ids only pick a candidate, and the GUI must
    /// not write backlight, keymap or macro packets to a board it has not
    /// identified as an LK67 (`docs/protocol.md`, the device table).
    writes_allowed: bool,
    /// The last layer read from the board: `(profile, fn_layer, keymap)`. Binding
    /// a key is a read-modify-write of the whole layer, and a layer read is two
    /// 8-page passes, so keeping what was just read saves that on every bind.
    layer: Option<(u8, bool, Keymap)>,
}

impl Worker {
    fn new(events: UnboundedSender<Event>) -> Self {
        Self {
            events,
            device: None,
            fn_locked: &[],
            macros: Vec::new(),
            writes_allowed: false,
            layer: None,
        }
    }

    fn emit(&self, e: Event) {
        let _ = self.events.unbounded_send(e);
    }

    fn run(mut self, rx: Receiver<Request>) {
        while let Ok(req) = rx.recv() {
            match req {
                Request::Connect => {
                    // Reconnect must actually re-enumerate: the node name
                    // changes across replugs, so a cached handle can be stale.
                    self.device = None;
                    self.connect_and_read();
                }
                Request::SetLayer(p) => {
                    if self.writable() {
                        self.set_layer(p);
                    }
                }
                Request::ReadLayer(fn_layer) => self.read_layer(fn_layer),
                Request::SetBacklight(p) => {
                    if self.writable() {
                        self.set_backlight(&p);
                    }
                }
                Request::SetBacklightOff(off) => {
                    if self.writable() {
                        self.set_backlight_off(off);
                    }
                }
                Request::SetSlot {
                    fn_layer,
                    slot,
                    value,
                } => {
                    if self.writable() {
                        self.set_slot(fn_layer, slot, value);
                    }
                }
                Request::SetMacro {
                    slot,
                    repeat,
                    events,
                } => {
                    if self.writable() {
                        self.set_macro(slot, repeat, events);
                    }
                }
                Request::DeleteMacro(slot) => {
                    if self.writable() {
                        self.delete_macro(slot);
                    }
                }
                Request::ReadMacros => self.read_macros(),
            }
        }
    }

    /// Open the board if needed, then read the summary and current state.
    fn connect_and_read(&mut self) {
        if !self.ensure_device() {
            return;
        }
        self.read_summary();
        self.read_backlight();
        self.read_layer(false);
        self.read_layer(true);
    }

    /// Make sure the board is open, re-enumerating when the cached node has
    /// gone. Unplugging and replugging renumbers `/dev/hidrawN`, and a number
    /// freed by another device gets reused, so a cached handle can point at a
    /// device that no longer exists while its path looks fine — every access
    /// then fails with ioctl `ENODEV` and reusing the fd never recovers.
    fn ensure_device(&mut self) -> bool {
        let gone = match &self.device {
            Some((_, info)) => node_missing(&info.path),
            None => true,
        };
        if !gone {
            return true;
        }
        if self.device.take().is_some() {
            self.emit(Event::Disconnected);
        }
        self.layer = None;
        self.writes_allowed = false;
        let Some(info) = gamakay_hidraw::find_supported() else {
            self.emit(Event::Disconnected);
            return false;
        };
        // Never write to a device that is not ours: `find_supported` falls back
        // to *any* hidraw node with a 64-byte feature report (on this machine,
        // the Corsair mouse), and this worker writes backlight, keymap and macro
        // packets. Only a board this project drives may be written to.
        if !info.is_gamakay() {
            self.emit(Event::Error(format!(
                "{} ({:04x}:{:04x}, {}) is not a recognised GamaKay keyboard",
                info.path.display(),
                info.vendor_id,
                info.product_id,
                info.connection.label()
            )));
            self.emit(Event::Disconnected);
            return false;
        }
        match HidrawDevice::open(&info.path) {
            Ok(dev) => {
                self.device = Some((dev, info));
                // Announce it so the sidebar stops showing the old error.
                self.read_summary();
                true
            }
            Err(e) => {
                self.emit(Event::Error(format!("open: {e}")));
                self.emit(Event::Disconnected);
                false
            }
        }
    }

    fn with_session<T>(
        &mut self,
        f: impl FnOnce(&mut Session) -> gamakay_core::Result<T>,
    ) -> Option<T> {
        if !self.ensure_device() {
            return None;
        }
        let outcome = match self.device.as_mut() {
            Some((dev, _)) => {
                let mut s = Session::new(dev, Guard::new(false));
                f(&mut s)
            }
            None => return None,
        };
        match outcome {
            Ok(v) => Some(v),
            Err(e) => {
                // A device that has gone away never recovers in place: drop the
                // handle so the next request re-enumerates. Whatever was cached
                // for it is not known to be current any more.
                if matches!(e, CoreError::Transport(_)) {
                    self.device = None;
                    self.layer = None;
                    // The board's identity is not known for whatever comes
                    // next, so writes stay refused until it identifies again.
                    self.writes_allowed = false;
                    self.emit(Event::Disconnected);
                }
                self.emit(Event::Error(e.to_string()));
                None
            }
        }
    }

    fn read_summary(&mut self) {
        let Some((_, info)) = &self.device else {
            return;
        };
        let name = info.name.clone();
        let usb = format!("{:04x}:{:04x}", info.vendor_id, info.product_id);
        let link = info.connection.label().to_string();
        // A board that does not answer is *not* connected: never report a bogus
        // "unknown" model, which is what a failed identify used to look like.
        let Some((id, rev, profile)) = self.with_session(|s| {
            let id = s.device_id()?;
            let rev = s.revision().ok();
            let profile = s.profile().unwrap_or(0);
            Ok((id, rev, profile))
        }) else {
            self.writes_allowed = false;
            self.emit(Event::Disconnected);
            return;
        };
        let record = gamakay_core::device::by_id(id);
        self.writes_allowed = record.is_some_and(|r| r.id == gamakay_core::device::LK67.id);
        self.fn_locked = record.map(|r| r.fn_locked).unwrap_or(&[]);
        let model = record
            .map(|r| r.display_name.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let layers = record.map(|r| r.layer).unwrap_or(1);
        let firmware = rev.filter(|r| *r != 0).map(|r| format!("{r:X}"));
        self.emit(Event::Connected(Summary {
            name,
            usb,
            link,
            id,
            model,
            firmware,
            profile,
            layers,
        }));
    }

    fn read_backlight(&mut self) {
        if let Some(led) = self.with_session(|s| s.led_param()) {
            self.emit(Event::Backlight(led));
        }
    }

    fn read_layer(&mut self, fn_layer: bool) {
        let profile = self.with_session(|s| s.profile()).unwrap_or(0);
        if let Some(km) = self.with_session(|s| s.read_layer(profile, fn_layer)) {
            self.layer = Some((profile, fn_layer, km.clone()));
            self.emit(Event::Layer {
                fn_layer,
                slots: layer_slots(&km),
                locked: if fn_layer {
                    self.fn_locked.to_vec()
                } else {
                    Vec::new()
                },
            });
        }
    }

    fn read_macros(&mut self) {
        let mut out = Vec::new();
        for slot in 0..MACRO_SLOTS {
            if let Some(m) = self.with_session(|s| s.read_macro(slot))
                && !m.is_empty()
            {
                out.push((slot, describe(&m)));
            }
        }
        self.macros = out;
        self.emit(Event::Macros(self.macros.clone()));
    }

    /// Record `slot`'s new contents in the cached list and hand it to the UI.
    /// The write is fire-and-forget, so the list is updated from what was sent
    /// rather than by re-reading every slot.
    fn set_macro_in_list(&mut self, slot: u8, value: Option<String>) {
        self.macros.retain(|(s, _)| *s != slot);
        if let Some(desc) = value {
            self.macros.push((slot, desc));
            self.macros.sort_by_key(|(s, _)| *s);
        }
        self.emit(Event::Macros(self.macros.clone()));
    }

    fn set_backlight(&mut self, p: &LedParam) {
        if let Some(state) = self.with_session(|s| {
            s.set_backlight(p, LK67_FLAGS)?;
            s.led_param()
        }) {
            self.emit(Event::Backlight(state));
            self.emit(Event::Status("lighting applied".into()));
        }
    }

    fn set_backlight_off(&mut self, off: bool) {
        if let Some(o) = self.with_session(|s| s.set_backlight_off(off)) {
            self.emit(Event::Status(format!(
                "backlight {}",
                if o.backlight_off { "off" } else { "on" }
            )));
        }
    }

    /// Select the active profile. The keymap commands address the active
    /// profile, so this is how a different keymap layer is opened.
    /// Whether a write may go out. Emits the reason and answers `false` when
    /// the board is not the one this build targets.
    fn writable(&mut self) -> bool {
        if self.writes_allowed {
            return true;
        }
        self.emit(Event::Error(
            "the attached device did not identify as an LK67; not writing to it".into(),
        ));
        false
    }

    fn set_layer(&mut self, profile: u8) {
        if self.with_session(|s| s.set_profile(profile)).is_none() {
            return;
        }
        // The active profile changed, so nothing cached belongs to it.
        self.layer = None;
        self.read_summary();
        self.read_layer(false);
        self.read_layer(true);
        // Only now claim the switch: if the layers could not be read, the UI
        // would otherwise highlight the new profile while showing the old
        // keymap, and the next key the user assigns would come from the wrong
        // picture.
        if self.layer.as_ref().is_some_and(|(p, ..)| *p == profile) {
            self.emit(Event::Status(format!("layer {} active", profile + 1)));
        } else {
            self.emit(Event::Error(format!(
                "layer {profile} was selected on the board but its keymap could not be read; \
                 press Reconnect to try again"
            )));
        }
    }

    fn set_slot(&mut self, fn_layer: bool, slot: u8, value: SlotValue) {
        // No read-back: a feature transfer that lands in the board's flash
        // commit window stalls the settings endpoint until it is re-enumerated
        // (see "Writes (SET)" in docs/protocol.md). The write is applied either
        // way, so the UI is updated from the copy we just sent.
        for attempt in 0..2 {
            let Some(profile) = self.with_session(|s| s.profile()) else {
                return;
            };
            // Reuse the layer this worker last read for this profile: a bind is
            // a read-modify-write of the whole layer and a layer read is two
            // 8-page passes, so binding several keys in a row would otherwise
            // pay that before every one. The live profile is re-read each time,
            // so a profile swapped on the keyboard itself is still noticed.
            let mut km = match &self.layer {
                Some((p, f, km)) if *p == profile && *f == fn_layer => km.clone(),
                _ => match self.with_session(|s| s.read_layer(profile, fn_layer)) {
                    Some(km) => km,
                    None => return,
                },
            };
            if usize::from(slot) >= gamakay_core::keymap::TX_SLOTS {
                self.emit(Event::Error(format!(
                    "slot {slot} cannot be written: a bulk write reaches slots 0..{}",
                    gamakay_core::keymap::TX_SLOTS - 1
                )));
                return;
            }
            if !km.set(slot as usize, Slot::from_value(value)) {
                self.emit(Event::Error(format!("slot {slot} is not in the layer")));
                return;
            }
            match self.with_session(|s| s.write_layer(profile, &km, fn_layer)) {
                Some(()) => {
                    self.layer = Some((profile, fn_layer, km.clone()));
                    self.emit(Event::Layer {
                        fn_layer,
                        slots: layer_slots(&km),
                        locked: if fn_layer {
                            self.fn_locked.to_vec()
                        } else {
                            Vec::new()
                        },
                    });
                    self.emit(Event::Status(format!("slot {slot} updated")));
                    return;
                }
                None => {
                    // The handle may have been dropped, in which case the board
                    // state is unknown: retry once against a fresh read.
                    self.layer = None;
                    if attempt == 0 && self.device.is_none() {
                        continue;
                    }
                    return;
                }
            }
        }
    }

    fn set_macro(&mut self, slot: u8, repeat: u16, events: Vec<MacroEvent>) {
        let m = Macro { repeat, events };
        // Refuse an oversize take before touching the board: a macro that does
        // not fit must never be half-written into the slot.
        if let Err(e) = m.to_blob() {
            self.emit(Event::Error(format!("macro: {e}")));
            return;
        }
        let desc = describe(&m);
        if self.with_session(|s| s.write_macro(slot, &m)).is_some() {
            self.set_macro_in_list(slot, Some(desc));
            self.emit(Event::Status(format!("macro {} written", slot + 1)));
        }
    }

    fn delete_macro(&mut self, slot: u8) {
        if self.with_session(|s| s.delete_macro(slot)).is_some() {
            self.set_macro_in_list(slot, None);
            self.emit(Event::Status(format!("macro {} cleared", slot + 1)));
        }
    }
}

/// Decode the non-empty slots of a layer into `(slot, description)`.
fn layer_slots(km: &Keymap) -> Vec<(u8, String)> {
    km.slots
        .iter()
        .enumerate()
        .filter(|(_, s)| **s != Slot::EMPTY)
        .map(|(i, s)| (i as u8, s.describe()))
        // An entry with no name (the Fn layer's `Special(8)`) is not listed.
        .filter(|(_, d)| !d.is_empty())
        .collect()
}

/// Whether a cached hidraw path has gone. Replugging renumbers `/dev/hidrawN`,
/// so a remembered path can stop existing (or be reused by another device).
fn node_missing(path: &std::path::Path) -> bool {
    !path.exists()
}

/// The mode names for the lighting picker.
pub fn mode_names() -> Vec<String> {
    Mode::ALL.iter().map(|m| m.name().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vanished_node_is_detected() {
        assert!(node_missing(std::path::Path::new(
            "/dev/hidraw-does-not-exist"
        )));
        assert!(!node_missing(std::path::Path::new("/dev/null")));
    }
}
