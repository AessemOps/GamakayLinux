// SPDX-License-Identifier: GPL-3.0-or-later
//! A hue/saturation colour wheel: click or drag to pick a colour. The wheel
//! itself is drawn once into a [`Cache`]; only the marker moves.

use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Program, Stroke, Style};
use iced::{Color, Length, Point, Rectangle, Renderer, Theme, mouse};

use crate::Message;
use crate::color;

const HUE_STEPS: usize = 60;
const SAT_STEPS: usize = 14;
const MARGIN: f32 = 6.0;

/// The wheel, positioned at the current hue/saturation.
pub struct ColorWheel {
    pub hue: f32,
    pub sat: f32,
}

impl ColorWheel {
    pub fn new(hue: f32, sat: f32) -> Self {
        Self { hue, sat }
    }

    fn radius(bounds: Rectangle) -> f32 {
        (bounds.width.min(bounds.height) / 2.0 - MARGIN).max(1.0)
    }

    /// The wheel's centre in canvas-local coordinates (the widget's origin is
    /// `(0, 0)` inside the frame, while `bounds` is absolute).
    fn center(bounds: Rectangle) -> Point {
        Point::new(bounds.width / 2.0, bounds.height / 2.0)
    }

    fn point(&self, bounds: Rectangle, hue: f32, sat: f32) -> Point {
        let c = Self::center(bounds);
        let r = Self::radius(bounds) * sat;
        let a = hue.to_radians();
        Point::new(c.x + r * a.cos(), c.y + r * a.sin())
    }

    /// Hue/saturation under a canvas-local position.
    fn hit(&self, pos: Point, bounds: Rectangle) -> (f32, f32) {
        let c = Self::center(bounds);
        let dx = pos.x - c.x;
        let dy = pos.y - c.y;
        let hue = dy.atan2(dx).to_degrees().rem_euclid(360.0);
        let sat = ((dx * dx + dy * dy).sqrt() / Self::radius(bounds)).clamp(0.0, 1.0);
        (hue, sat)
    }
}

/// A `Canvas` over the wheel, `size` pixels square.
pub fn wheel(w: ColorWheel, size: f32) -> Canvas<ColorWheel, Message> {
    Canvas::new(w)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
}

impl Program<Message> for ColorWheel {
    type State = bool; // is the button held down?

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor.position_in(bounds) {
                    *state = true;
                    let (h, s) = self.hit(pos, bounds);
                    return Some(canvas::Action::publish(Message::PickColor(h, s)).and_capture());
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                *state = false;
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) if *state => {
                if let Some(pos) = cursor.position_in(bounds) {
                    let (h, s) = self.hit(pos, bounds);
                    return Some(canvas::Action::publish(Message::PickColor(h, s)).and_capture());
                }
            }
            _ => {}
        }
        None
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
        let c = frame.center();
        let r = Self::radius(bounds);
        for hi in 0..HUE_STEPS {
            let h0 = hi as f32 * 360.0 / HUE_STEPS as f32;
            let h1 = (hi + 1) as f32 * 360.0 / HUE_STEPS as f32;
            for si in 0..SAT_STEPS {
                let s0 = si as f32 / SAT_STEPS as f32;
                let s1 = (si + 1) as f32 / SAT_STEPS as f32;
                let a0 = h0.to_radians();
                let a1 = h1.to_radians();
                let p = |a: f32, s: f32| Point::new(c.x + r * s * a.cos(), c.y + r * s * a.sin());
                let path = Path::new(|b| {
                    b.move_to(p(a0, s0));
                    b.line_to(p(a0, s1));
                    b.line_to(p(a1, s1));
                    b.line_to(p(a1, s0));
                    b.close();
                });
                frame.fill(
                    &path,
                    color::hsv_to_rgb((h0 + h1) / 2.0, (s0 + s1) / 2.0, 1.0),
                );
            }
        }

        let pos = self.point(bounds, self.hue, self.sat);
        frame.fill(&Path::circle(pos, 7.0), Color::WHITE);
        frame.stroke(
            &Path::circle(pos, 7.0),
            Stroke {
                width: 2.0,
                style: Style::Solid(Color::BLACK),
                ..Stroke::default()
            },
        );

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.position_in(bounds).is_some() {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;

    /// A canvas placed away from the origin: `bounds` is absolute while the
    /// cursor position is local, so `point` and `hit` must both work in the
    /// widget's own frame.
    #[test]
    fn hit_inverts_point_inside_the_widget() {
        let bounds = Rectangle::new(Point::new(630.0, 560.0), Size::new(150.0, 150.0));
        let w = ColorWheel::new(0.0, 0.0);
        for &(h, s) in &[(0.0f32, 1.0f32), (90.0, 0.5), (200.0, 0.8), (330.0, 0.3)] {
            let p = w.point(bounds, h, s);
            assert!(
                (0.0..=150.0).contains(&p.x) && (0.0..=150.0).contains(&p.y),
                "marker {p:?} falls outside the widget for hue {h}, sat {s}"
            );
            let (h2, s2) = w.hit(p, bounds);
            assert!((h - h2).abs() < 1.0, "hue {h} -> {h2}");
            assert!((s - s2).abs() < 0.02, "sat {s} -> {s2}");
        }
    }
}
