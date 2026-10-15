// SPDX-License-Identifier: GPL-3.0-or-later
//! The keyboard canvas: draws the LK67 layout with each key's current binding,
//! highlights the selected key and shows a pending change as a live preview.
//! On the Lighting tab the same canvas is tinted with the chosen colour.

use std::collections::HashMap;

use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Program, Stroke, Style, Text};
use iced::{Color, Length, Point, Rectangle, Renderer, Size, Theme, alignment, mouse};

use gamakay_core::layout::{LK67_HEIGHT, LK67_LAYOUT, LK67_WIDTH, LayoutKey};

use crate::Message;

const GAP: f32 = 4.0;
const RADIUS: f32 = 5.0;

const KEY_BG: Color = Color::from_rgb(0.16, 0.18, 0.22);
const KEY_SELECTED: Color = Color::from_rgb(0.30, 0.40, 0.55);
const TEXT: Color = Color::from_rgb(0.85, 0.88, 0.92);
const BINDING: Color = Color::from_rgb(0.53, 0.75, 0.82);
const PREVIEW: Color = Color::from_rgb(0.92, 0.80, 0.55);
const OUTLINE: Color = Color::from_rgb(0.53, 0.75, 0.82);

/// The canvas program.
pub struct Keyboard<'a> {
    /// slot -> current binding description (from the loaded layer).
    pub bindings: &'a HashMap<u8, String>,
    pub selected: Option<u8>,
    /// A pending change to preview: (slot, new binding).
    pub preview: Option<(u8, String)>,
    /// Tint every key (lighting preview).
    pub tint: Option<Color>,
}

/// A `Canvas` widget over the layout.
pub fn keyboard<'a>(kb: Keyboard<'a>) -> Canvas<Keyboard<'a>, Message> {
    Canvas::new(kb)
        .width(Length::Fill)
        .height(Length::Fixed(260.0))
}

impl Program<Message> for Keyboard<'_> {
    type State = Option<u8>;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let Some(pos) = cursor.position_in(bounds)
            && let Some(k) = hit(pos, bounds)
        {
            *state = Some(k.slot);
            return (
                canvas::event::Status::Captured,
                Some(Message::SelectSlot(k.slot)),
            );
        }
        (canvas::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let scale = ((bounds.width - GAP) / LK67_WIDTH)
            .min((bounds.height - GAP) / LK67_HEIGHT)
            .max(1.0);
        let ox = (bounds.width - LK67_WIDTH * scale) / 2.0;
        let oy = (bounds.height - LK67_HEIGHT * scale) / 2.0;

        for k in LK67_LAYOUT {
            let rect = Rectangle::new(
                Point::new(ox + k.x * scale + GAP / 2.0, oy + k.y * scale + GAP / 2.0),
                Size::new((k.w * scale - GAP).max(2.0), (k.h * scale - GAP).max(2.0)),
            );

            if k.knob {
                frame.fill(
                    &Path::circle(rect.center(), rect.width.min(rect.height) / 2.0),
                    Color::from_rgb(0.30, 0.33, 0.38),
                );
                continue;
            }

            let selected = self.selected == Some(k.slot);
            let fill = if selected {
                KEY_SELECTED
            } else {
                self.tint.unwrap_or(KEY_BG)
            };
            let path = Path::rounded_rectangle(rect.position(), rect.size(), RADIUS.into());
            frame.fill(&path, fill);
            if selected {
                frame.stroke(
                    &path,
                    Stroke {
                        width: 2.0,
                        style: Style::Solid(OUTLINE),
                        ..Stroke::default()
                    },
                );
            }

            // The key legend.
            frame.fill_text(Text {
                content: k.label.to_string(),
                position: Point::new(rect.center_x(), rect.y + rect.height * 0.18),
                color: TEXT,
                size: iced::Pixels((scale * 0.26).clamp(8.0, 18.0)),
                horizontal_alignment: alignment::Horizontal::Center,
                vertical_alignment: alignment::Vertical::Top,
                ..Text::default()
            });

            // The current binding, or the pending preview.
            let preview = self
                .preview
                .as_ref()
                .filter(|(slot, _)| *slot == k.slot)
                .map(|(_, v)| (v.as_str(), PREVIEW));
            let current = self.bindings.get(&k.slot).map(|v| (v.as_str(), BINDING));
            if let Some((text, color)) = preview.or(current)
                && text != k.label
            {
                frame.fill_text(Text {
                    content: text.to_string(),
                    position: Point::new(rect.center_x(), rect.y + rect.height * 0.62),
                    color,
                    size: iced::Pixels((scale * 0.2).clamp(7.0, 13.0)),
                    horizontal_alignment: alignment::Horizontal::Center,
                    vertical_alignment: alignment::Vertical::Top,
                    ..Text::default()
                });
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor.position_in(bounds).and_then(|p| hit(p, bounds)) {
            Some(_) => mouse::Interaction::Pointer,
            None => mouse::Interaction::default(),
        }
    }
}

/// The key under a canvas-local position.
fn hit(pos: Point, bounds: Rectangle) -> Option<&'static LayoutKey> {
    let scale = ((bounds.width - GAP) / LK67_WIDTH)
        .min((bounds.height - GAP) / LK67_HEIGHT)
        .max(1.0);
    let ox = (bounds.width - LK67_WIDTH * scale) / 2.0;
    let oy = (bounds.height - LK67_HEIGHT * scale) / 2.0;
    LK67_LAYOUT.iter().find(|k| {
        let x0 = ox + k.x * scale + GAP / 2.0;
        let y0 = oy + k.y * scale + GAP / 2.0;
        pos.x >= x0
            && pos.x <= x0 + (k.w * scale - GAP)
            && pos.y >= y0
            && pos.y <= y0 + (k.h * scale - GAP)
    })
}
