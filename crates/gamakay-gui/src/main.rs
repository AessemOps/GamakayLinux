// SPDX-License-Identifier: GPL-3.0-or-later
//! GamaKay GUI (iced 0.13). All device I/O runs on the [`worker`] thread; the
//! UI only sends requests and reacts to events, so it never blocks and does no
//! work while idle.

mod canvas;
mod color;
mod colorwheel;
mod patterns;
mod worker;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Mutex;

use iced::futures::channel::mpsc::{UnboundedReceiver, unbounded};
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, slider, text,
    text_input, toggler,
};
use iced::{Background, Border, Color, Element, Length, Subscription, Theme};

use gamakay_core::keymap::SlotValue;
use gamakay_core::layout::{LK67_HEIGHT, LK67_LAYOUT, LK67_WIDTH, LayoutKey};
use gamakay_core::light::{LK67_FLAGS, LedParam, LedState, Mode};
use worker::{Event, Request, Summary};

/// The worker's event receiver, handed to the subscription once.
static EVENTS: Mutex<Option<UnboundedReceiver<Event>>> = Mutex::new(None);

fn main() -> iced::Result {
    iced::application("GamaKay", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size(iced::Size::new(1280.0, 820.0))
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
    /// Animation phase for the lighting preview.
    phase: f32,
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
    PickColor(f32, f32),
    Tick,
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
            phase: 0.0,
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
            Message::PickColor(h, s) => {
                self.color = color::to_hex(rgb8(color::hsv_to_rgb(h, s, 1.0)));
            }
            Message::Tick => self.phase += 0.04,
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
        self.brightness = s.brightness.min(4);
        self.speed = s.speed.min(4);
        self.direction = s.option;
        self.rainbow = s.flags == LK67_FLAGS.rainbow;
    }

    fn apply_light(&mut self) {
        let mode = Mode::from_name(&self.mode).map(Mode::code).unwrap_or(1);
        let rgb = color::parse_hex(&self.color).unwrap_or([0xff, 0x00, 0x00]);
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

    fn subscription(&self) -> Subscription<Message> {
        let events = Subscription::run(event_stream).map(Message::Worker);
        if self.tab == Tab::Lighting {
            Subscription::batch([
                events,
                iced::time::every(std::time::Duration::from_millis(40)).map(|_| Message::Tick),
            ])
        } else {
            events
        }
    }

    fn theme(_: &App) -> Theme {
        Theme::Dark
    }

    fn view(&self) -> Element<'_, Message> {
        let body: Element<Message> = match self.tab {
            Tab::Info => self.view_info(),
            Tab::Lighting => self.view_lighting(),
            Tab::Keymap => self.view_keymap(),
            Tab::Macros => self.view_macros(),
        };

        let main =
            container(scrollable(container(body).height(Length::Shrink)).height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(16);

        row![self.sidebar(), main].into()
    }

    /// The left sidebar: the device, the section menu and the connection.
    fn sidebar(&self) -> Element<'_, Message> {
        let model = self
            .summary
            .as_ref()
            .map(|s| s.model.clone())
            .unwrap_or_else(|| "no keyboard".into());
        let menu: Vec<Element<Message>> = Tab::ALL
            .iter()
            .map(|&t| {
                let active = t == self.tab;
                button(text(t.label()).size(14))
                    .on_press(Message::Tab(t))
                    .width(Length::Fill)
                    .style(if active { button::primary } else { menu_button })
                    .into()
            })
            .collect();
        container(
            column![
                text("GamaKay").size(20),
                text(model).size(12),
                horizontal_space().height(Length::Fixed(6.0)),
                column(menu).spacing(4),
                horizontal_space(),
                self.status_pill(),
                button(text("Reconnect"))
                    .on_press(Message::Connect)
                    .width(Length::Fill)
                    .style(menu_button),
            ]
            .spacing(8),
        )
        .width(Length::Fixed(200.0))
        .height(Length::Fill)
        .padding(14)
        .style(sidebar_style)
        .into()
    }

    fn status_pill(&self) -> Element<'_, Message> {
        let color = if self.connected {
            Color::from_rgb8(0x4c, 0xaf, 0x50)
        } else {
            Color::from_rgb8(0xcf, 0x66, 0x79)
        };
        container(text(self.status.clone()).size(13))
            .padding([4, 10])
            .style(move |_| container::Style {
                background: Some(Background::Color(color.scale_alpha(0.16))),
                text_color: Some(color),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color,
                },
                ..container::Style::default()
            })
            .into()
    }

    fn view_info(&self) -> Element<'_, Message> {
        let s = self.summary.clone().unwrap_or_default();
        let grid = column![
            kv("Name", s.name.clone()),
            kv("Model", s.model.clone()),
            kv("USB id", s.usb.clone()),
            kv("Link", s.link.clone()),
            kv("Device id", s.id.to_string()),
            kv("Firmware", s.firmware.clone().unwrap_or_else(|| "-".into())),
            kv("Profile", s.profile.to_string()),
        ]
        .spacing(10);
        section("Device".to_string(), grid.into())
    }

    /// The lighting preview as a grid of coloured key widgets (a canvas would
    /// be cached by the renderer and would not animate). Horizontal gaps are
    /// reproduced so the layout matches the board.
    fn preview_grid(&self, mode_code: u8, rgb: [u8; 3]) -> Element<'_, Message> {
        let p = patterns::Params {
            mode: mode_code,
            speed: self.speed,
            brightness: self.brightness,
            brightness_max: 4,
            option: self.direction,
            rainbow: self.rainbow,
            rgb,
        };
        let frame = self.phase / (patterns::FRAME_MS / 1000.0);
        let unit = 52.0f32;
        let pad = 4.0f32;

        let mut rows: BTreeMap<i32, Vec<&LayoutKey>> = BTreeMap::new();
        for k in LK67_LAYOUT.iter() {
            rows.entry(k.y as i32).or_default().push(k);
        }

        let rows_el: Vec<Element<Message>> = rows
            .into_values()
            .map(|mut keys| {
                keys.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
                let mut cells: Vec<Element<Message>> = Vec::new();
                let mut prev_end = 0.0f32;
                for k in keys {
                    let gap = k.x - prev_end;
                    if gap > 0.05 {
                        cells.push(container(text(" ")).width(Length::Fixed(gap * unit)).into());
                    }
                    let x = (k.x + k.w / 2.0) / LK67_WIDTH;
                    let y = (k.y + k.h / 2.0) / LK67_HEIGHT;
                    let c = patterns::color_at(&p, x, y, k.slot as usize, frame);
                    let radius = if k.knob { unit / 2.0 } else { 5.0 };
                    cells.push(
                        container(text(k.label).size(11))
                            .width(Length::Fixed(k.w * unit - pad))
                            .height(Length::Fixed(unit - pad))
                            .align_x(iced::Alignment::Center)
                            .align_y(iced::Alignment::Center)
                            .style(move |_: &Theme| container::Style {
                                background: Some(Background::Color(c)),
                                border: Border {
                                    radius: radius.into(),
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                },
                                ..container::Style::default()
                            })
                            .into(),
                    );
                    prev_end = k.x + k.w;
                }
                Element::from(row(cells).spacing(2.0))
            })
            .collect();
        column(rows_el).spacing(pad).into()
    }

    fn view_lighting(&self) -> Element<'_, Message> {
        let rgb = color::parse_hex(&self.color).unwrap_or([0xff, 0x00, 0x00]);
        let tint = Color::from_rgb8(rgb[0], rgb[1], rgb[2]);
        let (hue, sat) = color::rgb_to_hs(rgb);
        let mode_code = Mode::from_name(&self.mode).map(Mode::code).unwrap_or(1);
        let swatch: Element<Message> = container(text(" "))
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(22.0))
            .style(move |_: &Theme| container::Style {
                background: Some(Background::Color(tint)),
                border: Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: Color::from_rgb(0.4, 0.4, 0.45),
                },
                ..container::Style::default()
            })
            .into();
        let preview = self.preview_grid(mode_code, rgb);
        let colour_field: Element<Message> = row![
            Element::from(colorwheel::wheel(colorwheel::ColorWheel::new(hue, sat))),
            Element::from(
                column![
                    swatch,
                    Element::from(text_input("#rrggbb", &self.color).on_input(Message::Color)),
                    Element::from(text(format!("hue {hue:.0}°  sat {:.0}%", sat * 100.0)).size(12)),
                ]
                .spacing(10),
            ),
        ]
        .spacing(16)
        .into();
        let controls = column![
            labeled(
                "Mode",
                pick_list(self.modes.clone(), Some(self.mode.clone()), Message::Mode).into()
            ),
            labeled("Colour", colour_field),
            labeled(
                "Brightness",
                row![
                    Element::from(slider(0..=4, self.brightness, Message::Brightness)),
                    text(format!("{} / 4", self.brightness)),
                ]
                .spacing(8)
                .into()
            ),
            labeled(
                "Speed",
                row![
                    Element::from(slider(0..=4, self.speed, Message::Speed)),
                    text(format!("{} / 4", self.speed)),
                ]
                .spacing(8)
                .into()
            ),
            labeled(
                "Direction",
                row![
                    Element::from(slider(0..=3, self.direction, Message::Direction)),
                    text(self.direction.to_string()),
                ]
                .spacing(8)
                .into()
            ),
            rainbow_row(self.rainbow),
            light_buttons(),
        ]
        .spacing(12);
        column![
            section("Preview".to_string(), preview),
            section("Backlight".to_string(), controls.into()),
        ]
        .spacing(12)
        .into()
    }

    fn view_keymap(&self) -> Element<'_, Message> {
        let kb = canvas::keyboard(canvas::Keyboard {
            bindings: &self.bindings,
            selected: self.selected,
            preview: self.preview.clone(),
            lighting: None,
        });
        let editing = match self.selected {
            Some(slot) => format!("Editing slot {slot}"),
            None => "Click a key on the board to edit it".to_string(),
        };
        let editor = row![
            text_input("slot:key (e.g. 3:Esc)", &self.slot_edit).on_input(Message::SlotEdit),
            button(text("Apply"))
                .on_press(Message::SlotApply)
                .style(button::primary),
        ]
        .spacing(8);
        let list = column(self.layer.iter().map(|(slot, desc)| {
            let mark = if self.selected == Some(*slot) {
                "●"
            } else {
                " "
            };
            Element::from(text(format!("{mark} slot {slot:>3}   {desc}")))
        }))
        .spacing(2);
        column![
            section(
                "Layer".to_string(),
                row![
                    text("Fn layer"),
                    toggler(self.fn_layer).on_toggle(Message::ToggleFn),
                    horizontal_space(),
                    text(editing).size(13),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center)
                .into()
            ),
            section("Board".to_string(), kb.into()),
            section("Rebind".to_string(), editor.into()),
            section("Slots".to_string(), list.into()),
        ]
        .spacing(12)
        .into()
    }

    fn view_macros(&self) -> Element<'_, Message> {
        let list: Element<Message> = if self.macros.is_empty() {
            text("No macros stored.").into()
        } else {
            column(
                self.macros
                    .iter()
                    .map(|(slot, desc)| Element::from(text(format!("macro {slot:>2}   {desc}")))),
            )
            .spacing(2)
            .into()
        };
        let editor = column![
            row![
                labeled(
                    "Slot",
                    text_input("0", &self.macro_slot)
                        .on_input(Message::MacroSlot)
                        .into()
                ),
                labeled(
                    "Repeat",
                    text_input("1", &self.macro_repeat)
                        .on_input(Message::MacroRepeat)
                        .into()
                ),
            ]
            .spacing(12),
            labeled(
                "Steps",
                text_input("A+@20, A-@20", &self.macro_steps)
                    .on_input(Message::MacroSteps)
                    .into()
            ),
            macro_buttons(),
        ]
        .spacing(12);
        column![
            section("Macros".to_string(), list),
            section("Edit".to_string(), editor.into()),
        ]
        .spacing(12)
        .into()
    }
}

/// The sidebar background.
fn sidebar_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.08, 0.08, 0.11))),
        ..container::Style::default()
    }
}

/// A flat, full-width sidebar button.
fn menu_button(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            Color::from_rgb(0.16, 0.16, 0.21)
        } else {
            Color::from_rgb(0.11, 0.11, 0.15)
        })),
        text_color: Color::from_rgb(0.85, 0.86, 0.9),
        border: Border {
            radius: 6.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..button::Style::default()
    }
}

/// A rounded card used to group a section.
fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.12, 0.14, 0.17))),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: Color::from_rgb(0.20, 0.23, 0.28),
        },
        ..container::Style::default()
    }
}

/// A titled card.
fn section<'a>(title: String, body: Element<'a, Message>) -> Element<'a, Message> {
    container(column![text(title).size(15), body].spacing(10))
        .padding(14)
        .style(card)
        .into()
}

/// The Rainbow toggle row.
fn rainbow_row(value: bool) -> Element<'static, Message> {
    row![text("Rainbow"), toggler(value).on_toggle(Message::Rainbow)]
        .spacing(8)
        .into()
}

/// The lighting action buttons.
fn light_buttons() -> Element<'static, Message> {
    row![
        button(text("Apply"))
            .on_press(Message::ApplyLight)
            .style(button::primary),
        button(text("Backlight on")).on_press(Message::LightOn),
        button(text("Backlight off")).on_press(Message::LightOff),
    ]
    .spacing(8)
    .into()
}

/// The macro action buttons.
fn macro_buttons() -> Element<'static, Message> {
    row![
        button(text("Set"))
            .on_press(Message::MacroSet)
            .style(button::primary),
        button(text("Delete")).on_press(Message::MacroDelete),
    ]
    .spacing(8)
    .into()
}

/// A small labelled field.
fn labeled<'a>(label: &'a str, field: Element<'a, Message>) -> Element<'a, Message> {
    column![text(label).size(12), field].spacing(4).into()
}

/// A key/value line for the Info tab.
fn kv<'a>(key: &'a str, value: String) -> Element<'a, Message> {
    row![
        container(text(key).size(13)).width(Length::Fixed(90.0)),
        text(value).size(14),
    ]
    .spacing(8)
    .into()
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

/// Round an `iced::Color` (0..=1) to 8-bit RGB.
fn rgb8(c: Color) -> [u8; 3] {
    [
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
    ]
}
