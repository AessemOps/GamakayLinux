// SPDX-License-Identifier: GPL-3.0-or-later
//! A tiny Tux for the Meta key. Drawn with canvas paths rather than a glyph:
//! the only font with a penguin (`Noto Color Emoji`) cannot be rendered as
//! text here, and a drawn icon scales cleanly to every preview size.

use iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke, Style};
use iced::{Color, Length, Point, Rectangle, Renderer, Size, Theme};

use crate::Message;

/// The penguin's body colour. The details are painted in `paper`, the colour
/// behind it, so the same drawing works on the dark board and on a lit key.
const INK: Color = Color::from_rgb(0.90, 0.92, 0.96);

/// Draw Tux centred in `rect`, with `paper` as the colour behind it.
pub fn draw(frame: &mut Frame, rect: Rectangle, paper: Color) {
    let s = rect.width.min(rect.height);
    let c = rect.center();
    let oval = |w: f32, h: f32, dx: f32, dy: f32| {
        Path::rounded_rectangle(
            Point::new(c.x - w / 2.0 + dx * s, c.y - h / 2.0 + dy * s),
            Size::new(w * s, h * s),
            (w.min(h) * s / 2.0).into(),
        )
    };

    // Body: dark, outlined in ink so the silhouette reads on a dark cap.
    let body = oval(0.70, 1.00, 0.0, 0.0);
    frame.fill(&body, paper);
    frame.stroke(
        &body,
        Stroke {
            width: (s * 0.07).max(1.0),
            style: Style::Solid(INK),
            ..Stroke::default()
        },
    );

    // Belly, beak and eyes in ink.
    frame.fill(&oval(0.38, 0.46, 0.0, 0.26), INK);
    if s >= 11.0 {
        for dx in [-0.13_f32, 0.13] {
            frame.fill(
                &Path::circle(Point::new(c.x + dx * s, c.y - 0.21 * s), 0.085 * s),
                INK,
            );
        }
        frame.fill(
            &Path::new(|b| {
                b.move_to(Point::new(c.x - 0.10 * s, c.y - 0.07 * s));
                b.line_to(Point::new(c.x + 0.10 * s, c.y - 0.07 * s));
                b.line_to(Point::new(c.x, c.y + 0.06 * s));
                b.close();
            }),
            INK,
        );
    }
}

/// Tux as a widget, for the preview grids.
pub struct Tux {
    pub paper: Color,
}

impl Program<Message> for Tux {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        // `bounds` is absolute; the frame is local, so draw at the origin.
        draw(
            &mut frame,
            Rectangle::new(Point::ORIGIN, bounds.size()),
            self.paper,
        );
        vec![frame.into_geometry()]
    }
}

/// A `size`-pixel Tux widget on a `paper` background.
pub fn icon(paper: Color, size: f32) -> iced::widget::canvas::Canvas<Tux, Message> {
    iced::widget::canvas::Canvas::new(Tux { paper })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
}
