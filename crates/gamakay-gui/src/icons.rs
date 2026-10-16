// SPDX-License-Identifier: GPL-3.0-or-later
//! Small marks drawn with canvas paths, for legends that are not glyphs: the
//! Meta key's penguin and the Android/iOS Fn functions (whose only font here is
//! the colour-emoji one, which this renderer cannot draw as text).
//!
//! Each is filled in the key's own colour and drawn in ink, so the same code
//! works on the dark board and on a lit key preview.

use iced::widget::canvas::{Frame, Geometry, Path, Program};
use iced::{Color, Length, Point, Rectangle, Renderer, Size, Theme};

use crate::Message;
use crate::glyph::Icon;

/// The mark's colour; details are painted in `paper`, the colour behind it.
const INK: Color = Color::from_rgb(0.90, 0.92, 0.96);

/// Draw `icon` centred in `rect`, with `paper` as the colour behind it.
pub fn draw(frame: &mut Frame, icon: Icon, rect: Rectangle, paper: Color) {
    match icon {
        Icon::Tux => tux(frame, rect, paper),
        Icon::Android => android(frame, rect, paper),
        Icon::Phone => phone(frame, rect, paper),
    }
}

/// A rounded blob helper, relative to the rect's centre and size.
fn oval(c: Point, s: f32, w: f32, h: f32, dx: f32, dy: f32) -> Path {
    Path::rounded_rectangle(
        Point::new(c.x - w * s / 2.0 + dx * s, c.y - h * s / 2.0 + dy * s),
        Size::new(w * s, h * s),
        (w.min(h) * s / 2.0).into(),
    )
}

fn tux(frame: &mut Frame, rect: Rectangle, paper: Color) {
    let s = rect.width.min(rect.height);
    let c = rect.center();
    // Body outlined in ink so the silhouette reads on a dark cap.
    let body = oval(c, s, 0.70, 1.00, 0.0, 0.0);
    frame.fill(&body, paper);
    frame.stroke(
        &body,
        iced::widget::canvas::Stroke {
            width: (s * 0.07).max(1.0),
            style: iced::widget::canvas::Style::Solid(INK),
            ..Default::default()
        },
    );
    frame.fill(&oval(c, s, 0.38, 0.46, 0.0, 0.26), INK);
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

fn android(frame: &mut Frame, rect: Rectangle, paper: Color) {
    let s = rect.width.min(rect.height);
    let c = rect.center();
    // A dome: an ink disc with its lower half covered.
    frame.fill(
        &Path::circle(Point::new(c.x, c.y + 0.04 * s), 0.40 * s),
        INK,
    );
    frame.fill(
        &Path::rectangle(
            Point::new(c.x - 0.42 * s, c.y + 0.06 * s),
            Size::new(0.84 * s, 0.40 * s),
        ),
        paper,
    );
    if s >= 11.0 {
        for dx in [-0.15_f32, 0.15] {
            frame.fill(
                &Path::circle(Point::new(c.x + dx * s, c.y - 0.06 * s), 0.055 * s),
                paper,
            );
        }
        // Antennae.
        for dx in [-0.26_f32, 0.26] {
            frame.fill(
                &Path::rounded_rectangle(
                    Point::new(c.x + dx * s - 0.035 * s, c.y - 0.50 * s),
                    Size::new(0.07 * s, 0.22 * s),
                    (0.035 * s).into(),
                ),
                INK,
            );
        }
    }
}

fn phone(frame: &mut Frame, rect: Rectangle, paper: Color) {
    let s = rect.width.min(rect.height);
    let c = rect.center();
    let body = Path::rounded_rectangle(
        Point::new(c.x - 0.28 * s, c.y - 0.46 * s),
        Size::new(0.56 * s, 0.92 * s),
        (0.12 * s).into(),
    );
    frame.fill(&body, INK);
    // Screen and home button.
    frame.fill(
        &Path::rectangle(
            Point::new(c.x - 0.19 * s, c.y - 0.33 * s),
            Size::new(0.38 * s, 0.55 * s),
        ),
        paper,
    );
    frame.fill(
        &Path::circle(Point::new(c.x, c.y + 0.33 * s), 0.075 * s),
        paper,
    );
}

/// A mark as a widget, for the preview grids.
pub struct Mark {
    pub icon: Icon,
    pub paper: Color,
}

impl Program<Message> for Mark {
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
            self.icon,
            Rectangle::new(Point::ORIGIN, bounds.size()),
            self.paper,
        );
        vec![frame.into_geometry()]
    }
}

/// A `size`-pixel mark on a `paper` background.
pub fn widget(icon: Icon, paper: Color, size: f32) -> iced::widget::canvas::Canvas<Mark, Message> {
    iced::widget::canvas::Canvas::new(Mark { icon, paper })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
}
