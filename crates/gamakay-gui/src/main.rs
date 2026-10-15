// SPDX-License-Identifier: GPL-3.0-or-later
//! GamaKay GUI (iced 0.13). All device I/O runs on the [`worker`] thread; the
//! UI only sends requests and reacts to events, so it never blocks and does no
//! work while idle.

mod canvas;
mod worker;

use std::collections::HashMap;
use std::sync::Mutex;

use iced::futures::channel::mpsc::{UnboundedReceiver, unbounded};
use iced::widget::{
    button, column, container, pick_list, row, scrollable, slider, text, text_input, toggler,
};
use iced::{Element, Length, Subscription, Theme};

use gamakay_core::keymap::SlotValue;
use gamakay_core::light::{LK67_FLAGS, LedParam, LedState, Mode};
use worker::{Event, Request, Summary};

/// The worker's event receiver, handed to the subscription once.
static EVENTS: Mutex<Option<UnboundedReceiver<Event>>> = Mutex::new(None);

fn main() -> iced::Result {
    iced::application("GamaKay", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size(iced::Size::new(1100.0, 760.0))
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Info,
    Lighting,
    Keymap,
    Macros,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Info, Tab::Lighting, Tab::Keymap, Tab::Macros];
    fn label(self) -> &'static str {
        match self {
            Tab::Info => "Info",
            Tab::Lighting => "Lighting",
            Tab::Keymap => "Keymap",
            Tab::Macros => "Macros",
        }
    }
    /// The tab named on the command line, if any.
    fn from_arg(s: &str) -> Option<Tab> {
        match s.to_ascii_lowercase().as_str() {
            "info" => Some(Tab::Info),
            "lighting" | "light" => Some(Tab::Lighting),
            "keymap" | "keys" => Some(Tab::Keymap),
            "macros" | "macro" => Some(Tab::Macros),
            _ => None,
        }
    }
}

struct App {
    req: std::sync::mpsc::Sender<Request>,
    tab: Tab,
    connected: bool,
    summary: Option<Summary>,
    status: String,
    modes: Vec<String>,
    // Lighting editor.
    mode: String,
    color: String,
    brightness: u8,
    speed: u8,
    direction: u8,
    rainbow: bool,
    // Keymap editor.
    fn_layer: bool,
    layer: Vec<(u8, String)>,
    bindings: HashMap<u8, String>,
    selected: Option<u8>,
    preview: Option<(u8, String)>,
    slot_edit: String,
    // Macro editor.
    macros: Vec<(u8, String)>,
    macro_slot: String,
    macro_steps: String,
    macro_repeat: String,
}

#[derive(Debug, Clone)]
enum Message {
    Tab(Tab),
    Connect,
    Worker(Event),
    Mode(String),
    Color(String),
    Brightness(u8),
    Speed(u8),
    Direction(u8),
    Rainbow(bool),
    ApplyLight,
    LightOn,
    LightOff,
    ToggleFn(bool),
    SlotEdit(String),
    SlotApply,
    SelectSlot(u8),
    MacroSlot(String),
    MacroSteps(String),
    MacroRepeat(String),
    MacroSet,
    MacroDelete,
}

impl App {
    fn new() -> Self {
        let (ev_tx, ev_rx) = unbounded();
        *EVENTS.lock().unwrap() = Some(ev_rx);
        let req = worker::spawn(ev_tx);
        let tab = std::env::args()
            .nth(1)
            .and_then(|a| Tab::from_arg(&a))
            .unwrap_or(Tab::Info);
        let app = Self {
            req,
            tab,
            connected: false,
            summary: None,
            status: "connecting…".into(),
            modes: worker::mode_names(),
            mode: Mode::Static.name().into(),
            color: "#ff0000".into(),
            brightness: 3,
            speed: 2,
            direction: 0,
            rainbow: false,
            fn_layer: false,
            layer: Vec::new(),
            bindings: HashMap::new(),
            selected: None,
            preview: None,
            slot_edit: String::new(),
            macros: Vec::new(),
            macro_slot: "0".into(),
            macro_steps: "A+@20, A-@20".into(),
            macro_repeat: "1".into(),
        };
        let _ = app.req.send(Request::Connect);
        app
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Tab(t) => self.tab = t,
            Message::Connect => {
                self.status = "connecting…".into();
                self.send(Request::Connect);
            }
            Message::Worker(event) => self.on_event(event),
            Message::Mode(m) => self.mode = m,
            Message::Color(c) => self.color = c,
            Message::Brightness(v) => self.brightness = v,
            Message::Speed(v) => self.speed = v,
            Message::Direction(v) => self.direction = v,
            Message::Rainbow(v) => self.rainbow = v,
            Message::ApplyLight => self.apply_light(),
            Message::LightOn => self.send(Request::SetBacklightOff(false)),
            Message::LightOff => self.send(Request::SetBacklightOff(true)),
            Message::ToggleFn(v) => {
                self.fn_layer = v;
                self.send(Request::Refresh);
            }
            Message::SlotEdit(s) => {
                self.preview = parse_preview(&s);
                self.slot_edit = s;
            }
            Message::SlotApply => self.apply_slot(),
            Message::SelectSlot(slot) => {
                self.selected = Some(slot);
                let cur = self
                    .layer
                    .iter()
                    .find(|(s, _)| *s == slot)
                    .map(|(_, d)| d.clone())
                    .unwrap_or_default();
                self.slot_edit = format!("{slot}:{cur}");
                self.preview = parse_preview(&self.slot_edit);
            }
            Message::MacroSlot(s) => self.macro_slot = s,
            Message::MacroSteps(s) => self.macro_steps = s,
            Message::MacroRepeat(s) => self.macro_repeat = s,
            Message::MacroSet => self.set_macro(),
            Message::MacroDelete => self.delete_macro(),
        }
    }

    fn send(&self, r: Request) {
        let _ = self.req.send(r);
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Connected(s) => {
                self.connected = true;
                self.status = format!("connected: {} ({})", s.model, s.usb);
                self.summary = Some(s);
            }
            Event::Disconnected => {
                self.connected = false;
                self.summary = None;
                self.status = "no keyboard found".into();
            }
            Event::Backlight(s) => self.load_state(&s),
            Event::Layer { fn_layer, slots } => {
                if fn_layer == self.fn_layer {
                    self.bindings = slots.iter().cloned().collect();
                    self.layer = slots;
                }
            }
            Event::Macros(m) => self.macros = m,
            Event::Status(s) => self.status = s,
            Event::Error(e) => self.status = format!("error: {e}"),
        }
    }

    fn load_state(&mut self, s: &LedState) {
        self.mode = Mode::from_code(s.mode)
            .map(|m| m.name().to_string())
            .unwrap_or_else(|| s.mode.to_string());
        self.color = format!("#{:02x}{:02x}{:02x}", s.rgb[0], s.rgb[1], s.rgb[2]);
        self.brightness = s.brightness.min(5);
        self.speed = s.speed.min(4);
        self.direction = s.option;
        self.rainbow = s.flags == LK67_FLAGS.rainbow;
    }

    fn apply_light(&mut self) {
        let mode = Mode::from_name(&self.mode).map(Mode::code).unwrap_or(1);
        let rgb = parse_color(&self.color).unwrap_or([0xFF, 0x00, 0x00]);
        let p = LedParam {
            mode,
            speed: self.speed,
            brightness: self.brightness,
            option: self.direction,
            rainbow: self.rainbow,
            rgb,
        };
        self.send(Request::SetBacklight(p));
    }

    fn apply_slot(&mut self) {
        let Some((slot, key)) = self.slot_edit.split_once(':') else {
            self.status = "enter `slot:key` (e.g. 3:Esc)".into();
            return;
        };
        let Ok(slot) = slot.trim().parse::<u8>() else {
            self.status = "slot must be a number".into();
            return;
        };
        let value = if key.trim().eq_ignore_ascii_case("none") {
            SlotValue::Empty
        } else {
            match gamakay_core::keycode::usage(key.trim()) {
                Some(u) => SlotValue::Hid(u),
                None => {
                    self.status = format!("unknown key {:?}", key.trim());
                    return;
                }
            }
        };
        self.send(Request::SetSlot {
            fn_layer: self.fn_layer,
            slot,
            value,
        });
    }

    fn set_macro(&mut self) {
        let slot = self.macro_slot.trim().parse::<u8>().unwrap_or(0);
        let repeat = self.macro_repeat.trim().parse::<u16>().unwrap_or(1);
        self.send(Request::SetMacro {
            slot,
            repeat,
            steps: self.macro_steps.clone(),
        });
    }

    fn delete_macro(&mut self) {
        let slot = self.macro_slot.trim().parse::<u8>().unwrap_or(0);
        self.send(Request::DeleteMacro(slot));
    }

    fn subscription(_: &App) -> Subscription<Message> {
        Subscription::run(event_stream).map(Message::Worker)
    }

    fn theme(_: &App) -> Theme {
        Theme::Dark
    }

    fn view(&self) -> Element<'_, Message> {
        let tabs = row(Tab::ALL.map(|t| {
            let b = button(text(t.label())).on_press(Message::Tab(t));
            let b = if t == self.tab {
                b.style(button::primary)
            } else {
                b.style(button::secondary)
            };
            Element::from(b)
        }))
        .spacing(4);

        let body: Element<Message> = match self.tab {
            Tab::Info => self.view_info(),
            Tab::Lighting => self.view_lighting(),
            Tab::Keymap => self.view_keymap(),
            Tab::Macros => self.view_macros(),
        };

        let header = row![
            button(text("Reconnect")).on_press(Message::Connect),
            text(&self.status),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);

        container(column![header, tabs, scrollable(body).height(Length::Fill)])
            .padding(12)
            .into()
    }

    fn view_info(&self) -> Element<'_, Message> {
        let s = self.summary.clone().unwrap_or_default();
        column![
            text(format!("Name:      {}", s.name)),
            text(format!("USB id:    {}", s.usb)),
            text(format!("Link:      {}", s.link)),
            text(format!("Device id: {}", s.id)),
            text(format!("Model:     {}", s.model)),
            text(format!(
                "Firmware:  {}",
                s.firmware.clone().unwrap_or_else(|| "-".into())
            )),
            text(format!("Profile:   {}", s.profile)),
        ]
        .spacing(6)
        .into()
    }

    fn view_lighting(&self) -> Element<'_, Message> {
        let on = self.connected;
        let tint = parse_color(&self.color).map(|[r, g, b]| iced::Color::from_rgb8(r, g, b));
        let preview = canvas::keyboard(canvas::Keyboard {
            bindings: &self.bindings,
            selected: None,
            preview: None,
            tint,
        });
        let mut col = column![
            text("Preview").size(16),
            preview,
            row![
                text("Mode"),
                pick_list(self.modes.clone(), Some(self.mode.clone()), Message::Mode),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            row![
                text("Colour"),
                text_input("#rrggbb", &self.color).on_input(Message::Color),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            row![
                text("Brightness"),
                slider(0..=5, self.brightness, Message::Brightness),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            row![text("Speed"), slider(0..=4, self.speed, Message::Speed),]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            row![
                text("Direction"),
                slider(0..=3, self.direction, Message::Direction),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            row![
                text("Rainbow"),
                toggler(self.rainbow).on_toggle(Message::Rainbow)
            ]
            .spacing(8),
            row![
                button(text("Apply")).on_press(Message::ApplyLight),
                button(text("On")).on_press(Message::LightOn),
                button(text("Off")).on_press(Message::LightOff),
            ]
            .spacing(8),
        ]
        .spacing(10);
        if !on {
            col = col.push(text("No keyboard connected."));
        }
        col.into()
    }

    fn view_keymap(&self) -> Element<'_, Message> {
        let kb = canvas::keyboard(canvas::Keyboard {
            bindings: &self.bindings,
            selected: self.selected,
            preview: self.preview.clone(),
            tint: None,
        });
        let list = self.layer.iter().map(|(slot, desc)| {
            let mark = if self.selected == Some(*slot) {
                ">"
            } else {
                " "
            };
            text(format!("{mark} slot {slot:>3}   {desc}")).into()
        });
        column![
            row![
                text("Layer"),
                toggler(self.fn_layer).on_toggle(Message::ToggleFn)
            ]
            .spacing(8),
            kb,
            text("Click a key to select it, then rebind it below."),
            row![
                text_input("slot:key (e.g. 3:Esc)", &self.slot_edit).on_input(Message::SlotEdit),
                button(text("Apply")).on_press(Message::SlotApply),
            ]
            .spacing(8),
            column(list).spacing(2),
        ]
        .spacing(10)
        .into()
    }

    fn view_macros(&self) -> Element<'_, Message> {
        let rows = self
            .macros
            .iter()
            .map(|(slot, desc)| text(format!("macro {slot:>2}   {desc}")).into());
        column![
            row![
                text("Slot"),
                text_input("0", &self.macro_slot).on_input(Message::MacroSlot),
                text("Repeat"),
                text_input("1", &self.macro_repeat).on_input(Message::MacroRepeat),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            text_input("A+@20, A-@20", &self.macro_steps).on_input(Message::MacroSteps),
            row![
                button(text("Set")).on_press(Message::MacroSet),
                button(text("Delete")).on_press(Message::MacroDelete),
            ]
            .spacing(8),
            column(rows).spacing(2),
        ]
        .spacing(10)
        .into()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn event_stream() -> UnboundedReceiver<Event> {
    EVENTS
        .lock()
        .unwrap()
        .take()
        .unwrap_or_else(|| unbounded().1)
}

fn parse_preview(s: &str) -> Option<(u8, String)> {
    let (slot, key) = s.split_once(':')?;
    let slot = slot.trim().parse::<u8>().ok()?;
    Some((slot, key.trim().to_string()))
}

fn parse_color(s: &str) -> Option<[u8; 3]> {
    let t = s.trim().trim_start_matches('#');
    if t.len() != 6 {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}
