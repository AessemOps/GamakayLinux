// SPDX-License-Identifier: GPL-3.0-or-later
//! GamaKay GUI (iced 0.14). All device I/O runs on the [`worker`] thread; the
//! UI only sends requests and reacts to events, so it never blocks and does no
//! work while idle.

/// How often the lighting preview advances. The preview runs on its own clock
/// (`phase`), so the step is derived from this rather than hard-coded.
const TICK: std::time::Duration = std::time::Duration::from_millis(40);

mod canvas;
mod color;
mod colorwheel;
mod glyph;
mod keypicker;
mod patterns;
mod physical;
mod recorder;
mod worker;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;

use iced::futures::channel::mpsc::{UnboundedReceiver, unbounded};
use iced::widget::{
    button, column, container, pick_list, row, scrollable, slider, space, text, text_input, toggler,
};
use iced::{Background, Border, Color, Element, Length, Subscription, Theme};

use gamakay_core::keycode;
use gamakay_core::keymap::{Slot, SlotValue};
use gamakay_core::layout::{LK67_HEIGHT, LK67_LAYOUT, LK67_WIDTH, LayoutKey};
use gamakay_core::light::{self, EdgeMode, LK67_FLAGS, LedParam, LedState, Mode};
use gamakay_core::macros::{MACRO_SLOTS, MACRO_STREAM_MAX, MacroEvent};
use worker::{Event, Request, Summary};

/// The worker's event receiver, handed to the subscription once.
static EVENTS: Mutex<Option<UnboundedReceiver<Event>>> = Mutex::new(None);

fn main() -> iced::Result {
    // 0.14 takes a boot function as the first argument; the title is a builder
    // method now.
    iced::application(App::new, App::update, App::view)
        .title("GamaKay")
        .subscription(App::subscription)
        .theme(App::theme)
        // Monochrome Noto Emoji, bundled: it is the only font here with the
        // emoji the previews use (⏸ 🔒 🍎 🤖). See CREDITS.md.
        .font(include_bytes!("../assets/NotoEmoji.ttf").as_slice())
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 820.0),
            // The shell associates a *window* with its .desktop entry by this
            // id (winit's WM_CLASS on X11, the app id on Wayland). Left unset —
            // as it was — the window advertises an empty WM_CLASS, so the
            // running app gets a generic taskbar icon and does not group under
            // its launcher entry, however well the package installs the icons.
            // The value matches the .desktop file's basename (`gamakay`).
            platform_specific: iced::window::settings::PlatformSpecific {
                application_id: "gamakay".to_owned(),
                ..Default::default()
            },
            ..Default::default()
        })
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
    // Underglow (the board's edge light: bottom mesh + side LEDs). Only mode
    // and colour are offered, because on this board brightness, speed and the
    // flags nibble are stored without changing anything visible. The write
    // therefore preserves the fields it read instead of inventing values, which
    // is why the last read is kept.
    /// The stored underglow parameter, once read.
    edge_store: Option<LedState>,
    edge_mode: String,
    edge_color: String,
    /// Whether the underglow has been read this session. Read when the Lighting
    /// tab is opened, not on connect: the connect path has a feature-transfer
    /// budget (see `Request::ReadLayer`).
    edge_loaded: bool,
    /// Seconds of preview animation time; advanced by [`TICK`].
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
    /// Which layer `bindings`/`locked` belong to. Kept in step with them, so a
    /// toggle never shows one layer's symbols with the other's locked set.
    shown_fn: bool,
    // Macro editor.
    /// The stored macros, as `(slot, description)`.
    macros: Vec<(u8, String)>,
    /// The slot the recorder writes to.
    macro_slot: u8,
    macro_repeat: String,
    /// The take being recorded.
    recorder: recorder::Recorder,
    /// Shown under the recorder when saving is refused.
    macro_note: String,
    /// The slot picker's entries, rebuilt when the stored macros change.
    slot_choices: Vec<SlotChoice>,
    /// Whether the macro slots have been read this session (done on demand).
    macros_loaded: bool,
}

/// One entry of the slot picker: its index and a label naming what the slot
/// holds, so an occupied slot is visible in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SlotChoice {
    index: u8,
    label: String,
}

impl std::fmt::Display for SlotChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
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
    /// Underglow (edge light) edits.
    EdgeMode(String),
    EdgeColor(String),
    ApplyEdge,
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
    /// The recorder's slot changed.
    MacroSlotPick(SlotChoice),
    MacroRepeat(String),
    /// Arm the recorder: the next keystrokes become the macro.
    RecordStart,
    /// Stop and close out the take.
    RecordStop,
    /// A key went down while recording, with the usage its physical position
    /// maps to (`None` when the board cannot send that key).
    RecordDown {
        usage: Option<u8>,
        key: iced::keyboard::Key,
        location: iced::keyboard::Location,
    },
    /// A key came up while recording.
    RecordUp {
        key: iced::keyboard::Key,
        location: iced::keyboard::Location,
    },
    /// Discard the take.
    RecordClear,
    /// Write the take into the selected slot.
    MacroSave,
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
        let mut app = Self {
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
            edge_store: None,
            edge_mode: EdgeMode::Static.name().into(),
            edge_color: "#ff0000".into(),
            edge_loaded: false,
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
            shown_fn: false,
            macros: Vec::new(),
            macro_slot: 0,
            macro_repeat: "1".into(),
            recorder: recorder::Recorder::default(),
            macro_note: String::new(),
            slot_choices: Vec::new(),
            macros_loaded: false,
        };
        app.rebuild_slot_choices();
        let _ = app.req.send(Request::Connect);
        app
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Tab(t) => {
                self.tab = t;
                self.read_open_tab();
            }
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
            Message::EdgeMode(m) => self.edge_mode = m,
            Message::EdgeColor(c) => self.edge_color = c,
            Message::ApplyEdge => self.apply_edge(),
            Message::FnLayer(fn_layer) => {
                self.fn_layer = fn_layer;
                // Drop anything pending for the layer being left: a stale
                // preview would draw its symbol on the wrong layer. The
                // bindings and the locked set stay as they are until the read
                // comes back, so the canvas never shows one layer's symbols
                // with the other's red keys.
                self.preview = None;
                self.selected = None;
                // Only the layer is re-read: a full refresh would also pull the
                // summary, the backlight and every macro slot.
                self.send(Request::ReadLayer(fn_layer));
            }
            Message::SwitchLayer(profile) => {
                self.profile = profile;
                // The same slot number means a different key on another
                // profile, so nothing selected on the old one may stay armed:
                // a picker click or a key press would otherwise write a slot of
                // the profile the user is no longer looking at.
                self.selected = None;
                self.preview = None;
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
            Message::Tick => self.phase += TICK.as_secs_f32(),
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
            Message::MacroSlotPick(c) => self.macro_slot = c.index,
            Message::MacroRepeat(s) => self.macro_repeat = s,
            Message::RecordStart => {
                self.macro_note.clear();
                self.recorder.start(Instant::now());
            }
            Message::RecordStop => self.recorder.stop(Instant::now()),
            Message::RecordDown {
                usage,
                key,
                location,
            } => self.recorder.down(usage, &key, location, Instant::now()),
            Message::RecordUp { key, location } => self.recorder.up(&key, location, Instant::now()),
            Message::RecordClear => {
                self.macro_note.clear();
                self.recorder.clear();
            }
            Message::MacroSave => self.macro_save(),
            Message::MacroDelete => self.macro_delete(),
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
                // Whatever the on-demand tabs hold belongs to the board that was
                // attached: a Reconnect re-opens the device without ever
                // emitting `Disconnected`, so it has to be dropped here too or
                // the panel would describe the previous board and its stored
                // fields would be what the next Apply writes.
                self.forget_on_demand();
                // A board that appears while an on-demand tab is already open
                // still needs its data read: the tab-change handler has already
                // run, and a tab named on the command line never runs it at all.
                self.read_open_tab();
            }
            Event::Disconnected => {
                // Showing either one after an unplug would describe a board that
                // is no longer attached.
                self.forget_on_demand();
                self.connected = false;
                self.summary = None;
                self.status = "no keyboard found".into();
            }
            Event::Backlight(s) => self.load_state(&s),
            Event::Edge(s) => self.load_edge(&s),
            Event::Layer {
                fn_layer,
                slots,
                locked,
            } => {
                if fn_layer == self.fn_layer {
                    // The canvas is about to show a different layer: drop any
                    // selection, which would otherwise keep the "press a key"
                    // capture armed on a key the user can no longer see.
                    if fn_layer != self.shown_fn {
                        self.selected = None;
                        self.preview = None;
                    }
                    self.bindings = slots.iter().cloned().collect();
                    self.layer = slots;
                    self.locked = locked.into_iter().collect();
                    self.shown_fn = fn_layer;
                }
            }
            Event::Macros(m) => {
                self.macros_loaded = true;
                self.macros = m;
                self.rebuild_slot_choices();
            }
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
        // Both fields feed a write, so neither may fall back to a default: a
        // mistyped colour silently becoming red, or an unmodelled board mode
        // becoming "static", would change the keyboard to something the user
        // did not ask for. Refuse and say why.
        let Some(mode) = self.current_mode_code() else {
            self.status = format!("unknown mode {:?}; not sent", self.mode);
            return;
        };
        let Some(rgb) = color::parse_hex(&self.color) else {
            self.status = format!("colour {:?} is not #rrggbb; not sent", self.color);
            return;
        };
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

    /// Take the underglow the board reported. An unmodelled mode code keeps its
    /// number rather than being snapped to a name, so a later apply cannot
    /// quietly change the mode.
    fn load_edge(&mut self, s: &LedState) {
        self.edge_mode = EdgeMode::from_code(s.mode)
            .map(|m| m.name().to_string())
            .unwrap_or_else(|| s.mode.to_string());
        self.edge_color = format!("#{:02x}{:02x}{:02x}", s.rgb[0], s.rgb[1], s.rgb[2]);
        self.edge_store = Some(*s);
        self.edge_loaded = true;
    }

    /// The edge mode the picker names, accepting a wire number. The rule is
    /// core's, so this panel and `light edge` resolve a name or a number the
    /// same way — including the underglow's off state, which is code 0.
    fn edge_mode_code(&self) -> Option<u8> {
        light::edge_mode_code(&self.edge_mode)
    }

    fn apply_edge(&mut self) {
        // The underglow write carries the whole parameter, so the fields this
        // panel does not offer (brightness, speed, direction, flags) come from
        // the board's own last read rather than from a default: inventing them
        // would change settings the user never touched. Without that read there
        // is nothing to preserve, so refuse instead.
        let Some(store) = self.edge_store else {
            self.status = "underglow not read yet; not sent".into();
            return;
        };
        let Some(mode) = self.edge_mode_code() else {
            self.status = format!("unknown underglow mode {:?}; not sent", self.edge_mode);
            return;
        };
        let Some(rgb) = color::parse_hex(&self.edge_color) else {
            self.status = format!("colour {:?} is not #rrggbb; not sent", self.edge_color);
            return;
        };
        self.send(Request::SetEdge(LedParam::with_mode_and_colour(
            &store, mode, rgb,
        )));
    }

    /// Forget what the on-demand tabs hold. Both belong to the board they were
    /// read from, which is not necessarily the board attached now.
    fn forget_on_demand(&mut self) {
        self.macros_loaded = false;
        self.macros.clear();
        self.edge_loaded = false;
        self.edge_store = None;
        self.rebuild_slot_choices();
    }

    /// Read what the open tab shows but the connect path does not fetch: the
    /// underglow (one transfer) and the macro list (200). Fetching them on
    /// demand is what keeps the connect burst, and its risk of stalling the
    /// board's control endpoint, small. The flags are not set here: a request
    /// lost to a disconnect would leave the panel empty for the session.
    fn read_open_tab(&mut self) {
        match self.tab {
            Tab::Lighting if !self.edge_loaded => self.send(Request::ReadEdge),
            Tab::Macros if !self.macros_loaded => self.send(Request::ReadMacros),
            _ => {}
        }
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

    /// Write the take into the selected slot.
    fn macro_save(&mut self) {
        if self.recorder.is_recording() {
            self.recorder.stop(Instant::now());
        }
        let events = self.recorder.events().to_vec();
        if events.is_empty() {
            self.macro_note = "record something first".into();
            return;
        }
        let repeat = match self.macro_repeat.trim().parse::<u16>() {
            Ok(n) if n >= 1 => n,
            _ => {
                self.macro_note =
                    format!("repeat must be a number >= 1, not {:?}", self.macro_repeat);
                return;
            }
        };
        self.macro_note.clear();
        self.send(Request::SetMacro {
            slot: self.macro_slot,
            repeat,
            events,
        });
    }

    fn macro_delete(&mut self) {
        self.send(Request::DeleteMacro(self.macro_slot));
    }

    /// Rebuild the slot picker's entries, each labelled with what the slot
    /// holds. Called when the stored macros change rather than from `view`:
    /// formatting 50 labels on every message is wasted work.
    fn rebuild_slot_choices(&mut self) {
        let stored: HashMap<u8, &str> = self
            .macros
            .iter()
            .map(|(slot, desc)| (*slot, desc.as_str()))
            .collect();
        self.slot_choices = (0..MACRO_SLOTS)
            .map(|i| SlotChoice {
                index: i,
                label: format!(
                    "Macro {:>2}  ·  {}",
                    i + 1,
                    shorten(stored.get(&i).copied().unwrap_or("empty"), 64)
                ),
            })
            .collect();
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![Subscription::run(event_stream).map(Message::Worker)];
        // Only animate while the selected mode actually moves: a static mode
        // would otherwise have the whole preview rebuilt 25 times a second.
        if self.tab == Tab::Lighting && patterns::animates(&self.pattern_params()) {
            subs.push(iced::time::every(TICK).map(|_| Message::Tick));
        }
        // While a key is selected on the Keymap tab, the next key press binds
        // it — the VIA "press a key" flow.
        if self.tab == Tab::Keymap && self.selected.is_some() {
            subs.push(iced::event::listen_with(capture_key));
        }
        // While the recorder is armed every keystroke becomes a macro event.
        if self.recorder.is_recording() {
            subs.push(iced::event::listen_with(record_key));
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
                space::horizontal().height(Length::Fixed(6.0)),
                column(menu).spacing(4),
                space::horizontal(),
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

    /// The mode code the mode field means: a known name, or the numeric code
    /// the board reported (which may be a mode this build does not model, and
    /// must round-trip rather than be replaced by a default).
    fn current_mode_code(&self) -> Option<u8> {
        if let Some(code) = Mode::from_name(&self.mode).map(Mode::code) {
            return Some(code);
        }
        match self.mode.trim().parse::<u8>() {
            Ok(n) if light::is_mode_code(n) => Some(n),
            _ => None,
        }
    }

    /// The preview parameters the lighting controls currently describe.
    fn pattern_params(&self) -> patterns::Params {
        patterns::Params {
            mode: self.current_mode_code().unwrap_or(1),
            speed: self.speed,
            brightness: self.brightness,
            brightness_max: 4,
            option: self.direction,
            rainbow: self.rainbow,
            rgb: color::parse_hex(&self.color).unwrap_or([0xff, 0x00, 0x00]),
        }
    }

    /// The lighting preview as a grid of coloured key widgets (a canvas would
    /// be cached by the renderer and would not animate).
    ///
    /// Each key sits in a cell exactly `w * unit` wide, with the coloured key
    /// inset by `pad`; positions are therefore exact and every row's right
    /// column lines up. A `row`-level spacing would accumulate per cell and
    /// drift the right column of rows that hold more keys.
    fn preview_grid(&self, p: &patterns::Params) -> Element<'_, Message> {
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
                    let c = patterns::color_at(p, x, y, k.slot as usize, frame);
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
        let p = self.pattern_params();
        let rgb = p.rgb;
        let tint = Color::from_rgb8(rgb[0], rgb[1], rgb[2]);
        let (hue, sat) = color::rgb_to_hs(rgb);

        let preview = container(self.preview_grid(&p))
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

        // Only what the board honours is offered: mode and colour. Brightness,
        // speed and the rainbow flag are stored for the underglow but change
        // nothing visible on it, so the write keeps whatever the board has
        // rather than pretending they do something (see docs/protocol.md ->
        // "Underglow (edge light)").
        let underglow: Element<Message> = if self.edge_store.is_none() {
            text("reading the underglow…").size(12).into()
        } else {
            let modes: Vec<String> = EdgeMode::ALL.iter().map(|m| m.name().to_string()).collect();
            column![
                row![
                    field_row(
                        "Mode",
                        pick_list(modes, Some(self.edge_mode.clone()), Message::EdgeMode)
                            .width(Length::Fixed(150.0))
                            .into()
                    ),
                    field_row(
                        "Colour",
                        Element::from(
                            text_input("#rrggbb", &self.edge_color)
                                .on_input(Message::EdgeColor)
                                .width(Length::Fixed(120.0))
                        )
                    ),
                    Element::from(button(text("Apply")).on_press(Message::ApplyEdge)),
                ]
                .spacing(14)
                .align_y(iced::Alignment::Center),
                text(
                    "The board's underglow has no brightness or speed: mode and colour \
                     are the only controls it answers to."
                )
                .size(11),
            ]
            .spacing(8)
            .into()
        };

        column![
            section("Preview".to_string(), preview.into()),
            section("Backlight".to_string(), backlight.into()),
            section("Underglow".to_string(), underglow),
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
            fn_layer: self.shown_fn,
        });
        let hint = match self.selected {
            Some(slot) => format!(
                "Slot {slot} — {} — press a key on the board, or pick one below",
                self.slot_edit
            ),
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
                space::horizontal(),
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
        let stored: Element<Message> = if self.macros.is_empty() {
            text("No macros stored.").size(12).into()
        } else {
            column(self.macros.iter().map(|(slot, desc)| {
                Element::from(text(format!("Macro {:>2}   {desc}", slot + 1)).size(12))
            }))
            .spacing(2)
            .into()
        };

        let choices = self.slot_choices.clone();
        let selected = choices.get(self.macro_slot as usize).cloned();
        let meta = row![
            labeled(
                "Slot",
                pick_list(choices, selected, Message::MacroSlotPick)
                    .width(Length::Fixed(360.0))
                    .into()
            ),
            labeled(
                "Repeat",
                text_input("1", &self.macro_repeat)
                    .on_input(Message::MacroRepeat)
                    .width(Length::Fixed(64.0))
                    .into()
            ),
        ]
        .spacing(16)
        .align_y(iced::Alignment::End);

        let record: Element<Message> = if self.recorder.is_recording() {
            button(text("■  Stop"))
                .on_press(Message::RecordStop)
                .style(danger_button)
                .into()
        } else {
            button(text("●  Record"))
                .on_press(Message::RecordStart)
                .style(button::primary)
                .into()
        };
        let buttons = row![
            record,
            button(text("Clear")).on_press(Message::RecordClear),
            space::horizontal(),
            button(text(format!("Save to macro {}", self.macro_slot + 1)))
                .on_press(Message::MacroSave)
                .style(button::primary),
            button(text(format!("Delete macro {}", self.macro_slot + 1)))
                .on_press(Message::MacroDelete)
                .style(danger_button),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let events: Element<Message> = if self.recorder.events().is_empty() {
            text(if self.recorder.is_recording() {
                "Press the keys of the macro, in order…"
            } else {
                "Press Record, then type the macro on the keyboard."
            })
            .size(12)
            .into()
        } else {
            scrollable(
                column(
                    self.recorder
                        .events()
                        .iter()
                        .enumerate()
                        .map(|(i, e)| macro_event_row(i, e)),
                )
                .spacing(2),
            )
            .height(Length::Fixed(200.0))
            .into()
        };

        let recorder = column![meta, buttons, text(self.macro_help()).size(12), events].spacing(10);

        column![
            section("Recorded macros".to_string(), stored),
            section("Recorder".to_string(), recorder.into()),
        ]
        .spacing(12)
        .into()
    }

    /// The line under the recorder: a refusal (a full slot, an unsendable key)
    /// wins over the running count.
    fn macro_help(&self) -> String {
        if !self.macro_note.is_empty() {
            return self.macro_note.clone();
        }
        if !self.recorder.note().is_empty() {
            return self.recorder.note().to_string();
        }
        let used = self.recorder.used_bytes();
        if self.recorder.is_recording() {
            return format!(
                "recording — {} events, {used} bytes",
                self.recorder.events().len()
            );
        }
        if self.recorder.events().is_empty() {
            return String::new();
        }
        format!(
            "{} events, {used} of {MACRO_STREAM_MAX} bytes",
            self.recorder.events().len()
        )
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
            space::horizontal(),
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

/// Map a key press/release into a recorder message. Unlike [`capture_key`]
/// this takes events whatever their status: while recording, every keystroke is
/// an event, and nothing on the tab should be eating them.
fn record_key(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            physical_key,
            key,
            location,
            ..
        }) => Some(Message::RecordDown {
            usage: physical::usage(&physical_key),
            key,
            location,
        }),
        iced::Event::Keyboard(iced::keyboard::Event::KeyReleased { key, location, .. }) => {
            Some(Message::RecordUp { key, location })
        }
        _ => None,
    }
}

/// One line of the recorded take: its place in the sequence, the key, whether
/// it goes down or up, and the pause it carries.
fn macro_event_row<'a>(i: usize, event: &MacroEvent) -> Element<'a, Message> {
    let (name, what, delay_ms) = match *event {
        MacroEvent::Key {
            usage,
            pressed,
            delay_ms,
        } => (
            keycode::name(usage)
                .map(str::to_string)
                .unwrap_or_else(|| format!("0x{usage:02X}")),
            if pressed { "down" } else { "up" },
            delay_ms,
        ),
        MacroEvent::MouseButton {
            button,
            pressed,
            delay_ms,
        } => (
            format!("Mouse {button}"),
            if pressed { "down" } else { "up" },
            delay_ms,
        ),
        MacroEvent::MouseMove { dx, dy, delay_ms } => {
            (format!("Move {dx}, {dy}"), "move", u16::from(delay_ms))
        }
    };
    row![
        container(text(format!("{:>2}", i + 1)).size(11)).width(Length::Fixed(28.0)),
        container(text(name).size(12)).width(Length::Fixed(120.0)),
        container(text(what).size(11)).width(Length::Fixed(46.0)),
        text(format!("{delay_ms} ms")).size(11),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

/// Truncate a label to `max` characters, on a character boundary.
fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A destructive button.
fn danger_button(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            Color::from_rgb(0.66, 0.27, 0.30)
        } else {
            Color::from_rgb(0.52, 0.21, 0.24)
        })),
        text_color: Color::from_rgb(0.98, 0.94, 0.94),
        border: Border {
            radius: 6.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..button::Style::default()
    }
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
            // 0.14 exposes the auto-repeat flag. The recorder still dedupes a
            // repeated press itself (see `recorder::Recorder::down`), so this is
            // passed for shape only — adopting it is a follow-up.
            repeat: false,
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
