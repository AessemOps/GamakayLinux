// SPDX-License-Identifier: GPL-3.0-or-later
//! The device worker: all blocking hidraw I/O runs on this thread, so the UI
//! never blocks. The UI sends [`Request`]s and receives [`Event`]s.

use std::sync::mpsc::Receiver;

use iced::futures::channel::mpsc::UnboundedSender;

use gamakay_core::error::Error as CoreError;
use gamakay_core::guard::Guard;
use gamakay_core::keymap::{Keymap, Slot, SlotValue};
use gamakay_core::light::{LK67_FLAGS, LedParam, LedState, Mode};
use gamakay_core::macros::{MACRO_SLOTS, Macro, describe, parse_steps};
use gamakay_core::transport::Session;
use gamakay_hidraw::{HidrawDevice, HidrawInfo};

/// A request from the UI to the worker.
pub enum Request {
    Connect,
    Refresh,
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
        steps: String,
    },
    DeleteMacro(u8),
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
}

/// An event from the worker to the UI.
#[derive(Debug, Clone)]
pub enum Event {
    Connected(Summary),
    Disconnected,
    Backlight(LedState),
    /// The decoded non-empty slots of a layer.
    Layer {
        fn_layer: bool,
        slots: Vec<(u8, String)>,
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
}

impl Worker {
    fn new(events: UnboundedSender<Event>) -> Self {
        Self {
            events,
            device: None,
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
                Request::Refresh => self.connect_and_read(),
                Request::SetBacklight(p) => self.set_backlight(&p),
                Request::SetBacklightOff(off) => self.set_backlight_off(off),
                Request::SetSlot {
                    fn_layer,
                    slot,
                    value,
                } => self.set_slot(fn_layer, slot, value),
                Request::SetMacro {
                    slot,
                    repeat,
                    steps,
                } => self.set_macro(slot, repeat, &steps),
                Request::DeleteMacro(slot) => self.delete_macro(slot),
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
        self.read_macros();
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
        let Some(info) = gamakay_hidraw::find_supported() else {
            self.emit(Event::Disconnected);
            return false;
        };
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
                // handle so the next request re-enumerates.
                if matches!(e, CoreError::Transport(_) | CoreError::DeviceNotFound) {
                    self.device = None;
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
        let (id, firmware, profile, model) = match self.with_session(|s| {
            let id = s.device_id()?;
            let rev = s.revision().ok();
            let profile = s.profile().unwrap_or(0);
            Ok((id, rev, profile))
        }) {
            Some((id, rev, profile)) => {
                let model = gamakay_core::device::by_id(id)
                    .map(|r| r.display_name.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let fw = rev.filter(|r| *r != 0).map(|r| format!("{r:X}"));
                (id, fw, profile, model)
            }
            None => (0, None, 0, "unknown".to_string()),
        };
        self.emit(Event::Connected(Summary {
            name,
            usb,
            link,
            id,
            model,
            firmware,
            profile,
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
            self.emit(Event::Layer {
                fn_layer,
                slots: layer_slots(&km),
            });
        }
    }

    fn read_macros(&mut self) {
        let mut out = Vec::new();
        for slot in 0..MACRO_SLOTS {
            if let Some(m) = self.with_session(|s| s.read_macro(slot)) {
                if !m.is_empty() {
                    out.push((slot, describe(&m)));
                }
            }
        }
        self.emit(Event::Macros(out));
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

    fn set_slot(&mut self, fn_layer: bool, slot: u8, value: SlotValue) {
        // A transport failure usually means the node moved under us: the handle
        // is dropped, so re-enumerate and try once more rather than making the
        // user click again.
        for attempt in 0..2 {
            let profile = self.with_session(|s| s.profile()).unwrap_or(0);
            let result = self.with_session(|s| {
                let mut km = s.read_layer(profile, fn_layer)?;
                km.set(slot as usize, Slot::from_value(value));
                s.write_layer(profile, &km, fn_layer)?;
                s.read_layer(profile, fn_layer)
            });
            match result {
                Some(km) => {
                    self.emit(Event::Layer {
                        fn_layer,
                        slots: layer_slots(&km),
                    });
                    self.emit(Event::Status(format!("slot {slot} updated")));
                    return;
                }
                None if attempt == 0 && self.device.is_none() => continue,
                None => return,
            }
        }
    }

    fn set_macro(&mut self, slot: u8, repeat: u16, steps: &str) {
        let events = match parse_steps(steps) {
            Ok(e) => e,
            Err(e) => {
                self.emit(Event::Error(format!("steps: {e}")));
                return;
            }
        };
        let m = Macro { repeat, events };
        let ok = self.with_session(|s| s.write_macro(slot, &m)).is_some();
        if ok {
            self.emit(Event::Status(format!("macro {slot} written")));
            self.read_macros();
        }
    }

    fn delete_macro(&mut self, slot: u8) {
        let ok = self.with_session(|s| s.delete_macro(slot)).is_some();
        if ok {
            self.emit(Event::Status(format!("macro {slot} cleared")));
            self.read_macros();
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
