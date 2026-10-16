// SPDX-License-Identifier: GPL-3.0-or-later
//! GamaKay GUI (iced 0.13). All device I/O runs on the [`worker`] thread; the
//! UI only sends requests and reacts to events, so it never blocks and does no
//! work while idle.

mod canvas;
mod color;
mod colorwheel;
mod glyph;
mod keypicker;
mod patterns;
mod physical;
mod worker;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;

use iced::futures::channel::mpsc::{UnboundedReceiver, unbounded};
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, slider, text,
    text_input, toggler,
};
use iced::{Background, Border, Color, Element, Length, Subscription, Theme};

use gamakay_core::keymap::{Slot, SlotValue};
use gamakay_core::layout::{LK67_HEIGHT, LK67_LAYOUT, LK67_WIDTH, LayoutKey};
use gamakay_core::light::{LK67_FLAGS, LedParam, LedState, Mode};
use worker::{Event, Request, Summary};

/// The worker's event receiver, handed to the subscription once.
static EVENTS: Mutex<Option<UnboundedReceiver<Event>>> = Mutex::new(None);

fn main() -> iced::Result {
    iced::application("GamaKay", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        // Monochrome Noto Emoji, bundled: it is the only font here with the
        // emoji the previews use (⏸ 🔒 🍎 🤖). See CREDITS.md.
        .font(include_bytes!("../assets/NotoEmoji.ttf").as_slice())
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
    show_slots: bool,
    /// Keymap layers the board exposes, and the active one.
    layers: u8,
    profile: u8,
    /// Slots the firmware runs itself, which the Fn layer shows in red.
    locked: HashSet<u8>,
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
    /// Edit the Fn layer rather than the base keymap.
    FnLayer(bool),
    /// Make a keymap layer (onboard profile) active.
    SwitchLayer(u8),
    SelectSlot(u8),
    /// Bind the selected slot to a value (from the picker or a captured key).
    AssignSlot(SlotValue),
    /// Expand/collapse the slot list.
    ToggleSlots,
    /// A physical key was pressed while a slot is selected.
    Capture(iced::keyboard::key::Physical),
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
            show_slots: false,
            layers: 1,
            profile: 0,
            locked: HashSet::new(),
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
            Message::FnLayer(fn_layer) => {
                self.fn_layer = fn_layer;
                // The other layer has different bindings, so drop anything
                // pending for the one being left: a stale preview would draw
                // its symbol on the wrong layer.
                self.preview = None;
                self.selected = None;
                self.locked.clear();
                self.send(Request::Refresh);
            }
            Message::SwitchLayer(profile) => {
                self.profile = profile;
                self.status = format!("switching to layer {}…", profile + 1);
                self.send(Request::SetLayer(profile));
            }
            Message::AssignSlot(v) => self.assign_slot(v),
            Message::ToggleSlots => self.show_slots = !self.show_slots,
            Message::Capture(physical) => match physical::usage(&physical) {
                Some(u) => self.assign_slot(SlotValue::Hid(u)),
                None => self.status = "that key can't be sent by the board".into(),
            },
            Message::PickColor(h, s) => {
                self.color = color::to_hex(rgb8(color::hsv_to_rgb(h, s, 1.0)));
            }
            Message::Tick => self.phase += 0.04,
            Message::SelectSlot(slot) => {
                self.selected = Some(slot);
                self.slot_edit = self
                    .layer
                    .iter()
                    .find(|(s, _)| *s == slot)
                    .map(|(_, d)| d.clone())
                    .unwrap_or_else(|| "-".into());
                self.preview = None;
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
                self.layers = s.layers;
                self.profile = s.profile;
                self.status = format!("connected: {} ({})", s.model, s.usb);
                self.summary = Some(s);
            }
            Event::Disconnected => {
                self.connected = false;
                self.summary = None;
                self.status = "no keyboard found".into();
            }
            Event::Backlight(s) => self.load_state(&s),
            Event::Layer {
                fn_layer,
                slots,
                locked,
            } => {
                if fn_layer == self.fn_layer {
                    self.bindings = slots.iter().cloned().collect();
                    self.layer = slots;
                    self.locked = locked.into_iter().collect();
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

    /// Bind the selected slot. The write is fire-and-forget, so the new value
    /// is shown at once as a preview on the board.
    fn assign_slot(&mut self, value: SlotValue) {
        let Some(slot) = self.selected else {
            self.status = "select a key on the board first".into();
            return;
        };
        let desc = Slot::from_value(value).describe();
        self.preview = Some((slot, desc.clone()));
        self.slot_edit = desc;
        self.status = "writing…".into();
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
        let mut subs = vec![Subscription::run(event_stream).map(Message::Worker)];
        if self.tab == Tab::Lighting {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(40)).map(|_| Message::Tick),
            );
        }
        // While a key is selected on the Keymap tab, the next key press binds
        // it — the VIA "press a key" flow.
        if self.tab == Tab::Keymap && self.selected.is_some() {
            subs.push(iced::event::listen_with(capture_key));
        }
        Subscription::batch(subs)
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
    /// be cached by the renderer and would not animate).
    ///
    /// Each key sits in a cell exactly `w * unit` wide, with the coloured key
    /// inset by `pad`; positions are therefore exact and every row's right
    /// column lines up. A `row`-level spacing would accumulate per cell and
    /// drift the right column of rows that hold more keys.
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
                        cells.push(
                            container(text(" "))
                                .width(Length::Fixed(gap * unit))
                                .height(Length::Fixed(unit))
                                .into(),
                        );
                    }
                    let x = (k.x + k.w / 2.0) / LK67_WIDTH;
                    let y = (k.y + k.h / 2.0) / LK67_HEIGHT;
                    let c = patterns::color_at(&p, x, y, k.slot as usize, frame);
                    let radius = if k.knob { (unit - pad) / 2.0 } else { 5.0 };
                    let key = container(
                        text(glyph::legend(k.label))
                            .size(11)
                            .font(glyph::font(k.label)),
                    )
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
                    });
                    cells.push(
                        container(key)
                            .width(Length::Fixed(k.w * unit))
                            .height(Length::Fixed(unit))
                            .align_x(iced::Alignment::Center)
                            .align_y(iced::Alignment::Center)
                            .into(),
                    );
                    prev_end = k.x + k.w;
                }
                Element::from(row(cells))
            })
            .collect();
        column(rows_el).into()
    }

    fn view_lighting(&self) -> Element<'_, Message> {
        let rgb = color::parse_hex(&self.color).unwrap_or([0xff, 0x00, 0x00]);
        let tint = Color::from_rgb8(rgb[0], rgb[1], rgb[2]);
        let (hue, sat) = color::rgb_to_hs(rgb);
        let mode_code = Mode::from_name(&self.mode).map(Mode::code).unwrap_or(1);

        let preview = container(self.preview_grid(mode_code, rgb))
            .width(Length::Fill)
            .align_x(iced::Alignment::Center);

        let swatch: Element<Message> = container(text(" "))
            .width(Length::Fixed(20.0))
            .height(Length::Fixed(20.0))
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
        let colour: Element<Message> = column![
            text("Colour").size(12),
            row![
                Element::from(colorwheel::wheel(
                    colorwheel::ColorWheel::new(hue, sat),
                    150.0
                )),
                column![
                    swatch,
                    Element::from(
                        text_input("#rrggbb", &self.color)
                            .on_input(Message::Color)
                            .width(Length::Fixed(120.0))
                    ),
                    Element::from(text(format!("hue {hue:.0}°  sat {:.0}%", sat * 100.0)).size(11)),
                ]
                .spacing(8),
            ]
            .spacing(14)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(8)
        .into();

        let controls: Element<Message> = row![
            column![
                field_row(
                    "Mode",
                    pick_list(self.modes.clone(), Some(self.mode.clone()), Message::Mode)
                        .width(Length::Fixed(150.0))
                        .into()
                ),
                slider_row(
                    "Brightness",
                    0..=4,
                    self.brightness,
                    Message::Brightness,
                    format!("{} / 4", self.brightness),
                ),
                slider_row(
                    "Speed",
                    0..=4,
                    self.speed,
                    Message::Speed,
                    format!("{} / 4", self.speed),
                ),
                slider_row(
                    "Direction",
                    0..=3,
                    self.direction,
                    Message::Direction,
                    self.direction.to_string(),
                ),
                field_row(
                    "Rainbow",
                    toggler(self.rainbow).on_toggle(Message::Rainbow).into()
                ),
            ]
            .spacing(10)
            .width(Length::Fill),
            colour,
        ]
        .spacing(30)
        .width(Length::Fill)
        .into();

        let backlight = column![controls, light_buttons()]
            .spacing(16)
            .width(Length::Fill);

        column![
            section("Preview".to_string(), preview.into()),
            section("Backlight".to_string(), backlight.into()),
        ]
        .spacing(12)
        .into()
    }

    fn view_keymap(&self) -> Element<'_, Message> {
        let kb = canvas::keyboard(canvas::Keyboard {
            bindings: &self.bindings,
            selected: self.selected,
            preview: self.preview.clone(),
            locked: &self.locked,
            fn_layer: self.fn_layer,
            lighting: None,
        });
        let hint = match self.selected {
            Some(slot) => format!("Slot {slot} — press a key on the board, or pick one below"),
            None => "Click a key above to select it".to_string(),
        };
        let layer_buttons: Vec<Element<Message>> = (0..self.layers.max(1))
            .map(|i| {
                let active = i == self.profile;
                button(text(format!("{}", i + 1)).size(12))
                    .on_press(Message::SwitchLayer(i))
                    .width(Length::Fixed(30.0))
                    .style(if active {
                        button::primary
                    } else {
                        picker_button
                    })
                    .into()
            })
            .collect();
        let board = column![
            row![
                text("Layer").size(13),
                row(layer_buttons).spacing(3),
                row![
                    text("Fn").size(12),
                    toggler(self.fn_layer).on_toggle(Message::FnLayer),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
                horizontal_space(),
                text(hint).size(13),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            Element::from(kb),
        ]
        .spacing(10)
        .width(Length::Fill);

        let list = column(self.layer.iter().map(|(slot, desc)| {
            let mark = if self.selected == Some(*slot) {
                "●"
            } else {
                " "
            };
            Element::from(text(format!("{mark} slot {slot:>3}   {desc}")))
        }))
        .spacing(2);

        let slots_body: Element<Message> = if self.show_slots {
            list.into()
        } else {
            text(format!("{} keys bound on this layer", self.layer.len()))
                .size(12)
                .into()
        };

        column![
            section("Board".to_string(), board.into()),
            section("Pick a key".to_string(), self.key_picker()),
            section_toggle("Slots", self.show_slots, slots_body, Message::ToggleSlots),
        ]
        .spacing(12)
        .into()
    }

    /// The VIA-style picker: every key the board can send, in categories.
    fn key_picker(&self) -> Element<'_, Message> {
        let groups: Vec<Element<Message>> = keypicker::CATEGORIES
            .iter()
            .map(|(name, entries)| {
                column![text(*name).size(12), key_grid(entries)]
                    .spacing(6)
                    .into()
            })
            .collect();
        column(groups).spacing(16).into()
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
    container(
        column![text(title).size(14), body]
            .spacing(8)
            .width(Length::Fill),
    )
    .padding(12)
    .width(Length::Fill)
    .style(card)
    .into()
}

/// A card whose header collapses the body.
fn section_toggle<'a>(
    title: &'a str,
    open: bool,
    body: Element<'a, Message>,
    on_toggle: Message,
) -> Element<'a, Message> {
    let header = button(
        row![
            text(title).size(14),
            horizontal_space(),
            text(if open { "▾" } else { "▸" }).size(14),
        ]
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
    )
    .on_press(on_toggle)
    .width(Length::Fill)
    .style(header_button);
    container(column![header, body].spacing(8).width(Length::Fill))
        .padding(12)
        .width(Length::Fill)
        .style(card)
        .into()
}

/// A flat, borderless button for card headers.
fn header_button(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: None,
        text_color: if hovered {
            Color::from_rgb(0.96, 0.97, 1.0)
        } else {
            Color::from_rgb(0.80, 0.83, 0.90)
        },
        ..button::Style::default()
    }
}

/// A label on the left and its control on the right, on a single line.
fn field_row<'a>(label: &'a str, control: Element<'a, Message>) -> Element<'a, Message> {
    row![
        container(text(label).size(12)).width(Length::Fixed(84.0)),
        control,
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .into()
}

/// A labelled 0..=max slider with its value shown on the right.
fn slider_row<'a>(
    label: &'a str,
    range: std::ops::RangeInclusive<u8>,
    value: u8,
    on_change: fn(u8) -> Message,
    shown: String,
) -> Element<'a, Message> {
    let control: Element<Message> = row![
        Element::from(slider(range, value, on_change).width(Length::Fixed(200.0))),
        container(text(shown).size(12)).width(Length::Fixed(56.0)),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .into();
    field_row(label, control)
}

/// Map an *ignored* key press into a capture message, so a focused text field
/// still receives its keystrokes.
fn capture_key(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    if status != iced::event::Status::Ignored {
        return None;
    }
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { physical_key, .. }) => {
            Some(Message::Capture(physical_key))
        }
        _ => None,
    }
}

/// A grid of picker buttons, filling the card's width.
fn key_grid(entries: &[keypicker::Entry]) -> Element<'static, Message> {
    const PER_ROW: usize = 12;
    let rows: Vec<Element<Message>> = entries
        .chunks(PER_ROW)
        .map(|chunk| {
            let cells: Vec<Element<Message>> = chunk
                .iter()
                .map(|(label, value)| {
                    button(text(*label).size(11))
                        .on_press(Message::AssignSlot(*value))
                        .width(Length::Fixed(78.0))
                        .style(picker_button)
                        .into()
                })
                .collect();
            row(cells).spacing(4).into()
        })
        .collect();
    column(rows).spacing(4).into()
}

/// A compact key-picker button.
fn picker_button(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            Color::from_rgb(0.24, 0.28, 0.34)
        } else {
            Color::from_rgb(0.16, 0.18, 0.22)
        })),
        text_color: Color::from_rgb(0.85, 0.88, 0.92),
        border: Border {
            radius: 5.0.into(),
            width: 1.0,
            color: Color::from_rgb(0.24, 0.27, 0.32),
        },
        ..button::Style::default()
    }
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

/// Round an `iced::Color` (0..=1) to 8-bit RGB.
fn rgb8(c: Color) -> [u8; 3] {
    [
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::Event as KbEvent;
    use iced::keyboard::key::{Code, Named, Physical};
    use iced::keyboard::{Key, Location, Modifiers};

    fn press(code: Code) -> iced::Event {
        let key = Key::Named(Named::Escape);
        iced::Event::Keyboard(KbEvent::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers: Modifiers::default(),
            text: None,
        })
    }

    /// The capture must only take keys no widget handled, so typing in a text
    /// field is never swallowed.
    #[test]
    fn capture_takes_ignored_key_presses_only() {
        let id = iced::window::Id::unique();
        assert!(matches!(
            capture_key(press(Code::KeyA), iced::event::Status::Ignored, id),
            Some(Message::Capture(_))
        ));
        assert!(capture_key(press(Code::KeyA), iced::event::Status::Captured, id).is_none());
    }
}
