// SPDX-License-Identifier: GPL-3.0-or-later
//! Best-effort reproductions of the LK67's backlight modes for the GUI
//! preview, based on the reference's timing tables (`docs/protocol.md`).
//!
//! These are approximations of the firmware's renderers, not byte-exact
//! copies. Keys are placed on a 6 x 18 grid from the layout's units.

use iced::Color;

use crate::color;

/// The lighting settings a preview draws from.
#[derive(Clone, Copy)]
pub struct Params {
    pub mode: u8,
    pub speed: u8,
    pub color: Color,
    pub rainbow: bool,
}

const FRAME: f32 = 0.005; // the firmware's 5 ms tick

/// Colour for a key at grid (row, col), `t` seconds since the tab opened.
pub fn color_for(p: &Params, row: u8, col: u8, t: f32) -> Color {
    let sp = p.speed.min(5) as usize;
    match p.mode {
        1 => p.color,                                 // static
        2 => breathing(p, sp, t),                     // breathing
        3 => spectrum(p, sp, t),                      // spectrum
        4 | 11 | 12 | 15 => wave(p, row, col, sp, t), // wave, spring, neon, loop
        5 | 14 => ripple(p, row, col, sp, t),         // ripple, radiant
        6 => star_dots(p, row, col, sp, t),           // star dots
        7 => flow(p, row, col, sp, t),                // flow
        8 => key_shadow(p, row, col),                 // key shadow (reactive; base dim)
        9 => layers(p, row, col, sp, t),              // layers
        10 => sine(p, row, col, sp, t),               // sine
        16 => colour_grid(p, row, col),               // colour grid
        17 => snow(p, row, col, sp, t, 1.0),          // snowfall
        18 => snow(p, row, col, sp, t, 3.0),          // meteor (fast streaks)
        19 => snow(p, row, col, sp, t, 0.5),          // silent snow (slow)
        _ => p.color,
    }
}

/// A 0..1 triangle wave.
fn tri(x: f32) -> f32 {
    let x = x.rem_euclid(1.0);
    if x < 0.5 { x * 2.0 } else { (1.0 - x) * 2.0 }
}

fn hash(a: u32, b: u32, c: u32) -> u32 {
    let mut h =
        a.wrapping_mul(374_761_393) ^ b.wrapping_mul(668_265_263) ^ c.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

fn rnd(a: u32, b: u32, c: u32) -> f32 {
    (hash(a, b, c) % 1000) as f32 / 1000.0
}

/// A saturated hue.
fn hue(h: f32) -> Color {
    color::hsv_to_rgb(h, 0.9, 1.0)
}

/// Breathing: the whole board pulses.
fn breathing(p: &Params, sp: usize, t: f32) -> Color {
    // Frames per ramp, from the yc500 table.
    const RAMP: [f32; 6] = [10.0, 40.0, 60.0, 80.0, 100.0, 200.0];
    let ramp = RAMP[sp] * FRAME;
    let pct = tri(t / (2.0 * ramp));
    color::scale(p.color, 0.2 + 0.8 * pct)
}

/// Spectrum: one hue for the whole board, cycling.
fn spectrum(p: &Params, sp: usize, t: f32) -> Color {
    const STEP: [f32; 6] = [5.0, 4.0, 3.0, 2.0, 1.0, 0.0];
    let step = STEP[sp];
    let h = if step == 0.0 {
        0.0
    } else {
        t / (2.0 * FRAME * step) * 360.0
    };
    if p.rainbow { hue(h) } else { p.color }
}

/// Wave: the hue steps one per column, moving across the board.
fn wave(p: &Params, _row: u8, col: u8, sp: usize, t: f32) -> Color {
    const STEP: [f32; 6] = [5.0, 4.0, 3.0, 2.0, 1.0, 0.0];
    let dir = if p.mode == 11 || p.mode == 15 {
        -1.0
    } else {
        1.0
    };
    let step = STEP[sp].max(1.0);
    let c = if dir > 0.0 {
        col as f32
    } else {
        17.0 - col as f32
    };
    let h = (c * 20.0 + t / (FRAME * step) * 6.0 * dir).rem_euclid(360.0);
    if p.rainbow {
        hue(h)
    } else {
        color::scale(p.color, 0.35 + 0.65 * (0.5 + 0.5 * (h.to_radians()).sin()))
    }
}

/// Ripple / radiant: a ring expanding from the centre.
fn ripple(p: &Params, row: u8, col: u8, sp: usize, t: f32) -> Color {
    const STEP: [f32; 6] = [5.0, 7.0, 9.0, 11.0, 13.0, 12.0];
    let period = STEP[sp] * FRAME * 6.0;
    let cx = 8.5;
    let cy = 2.5;
    let d = ((col as f32 - cx).powi(2) + ((row as f32 - cy) * 1.8).powi(2)).sqrt();
    let phase = (t / period - d / 10.0).rem_euclid(1.0);
    let level = (1.0 - phase).powf(2.5);
    let base = if p.rainbow {
        hue((d * 24.0 + t * 60.0).rem_euclid(360.0))
    } else {
        p.color
    };
    color::scale(base, 0.15 + 0.85 * level)
}

/// Star dots: sparse dots light up and expand.
fn star_dots(p: &Params, row: u8, col: u8, sp: usize, t: f32) -> Color {
    const STEP: [f32; 6] = [5.0, 10.0, 15.0, 20.0, 25.0, 12.0];
    let period = STEP[sp] * FRAME * 8.0;
    let epoch = (t / period).floor();
    // Each cell gets a staggered birth time.
    let off = rnd(col as u32, row as u32, 7) * period;
    let local = (t + off).rem_euclid(period) / period;
    let level = (1.0 - local).powf(3.0);
    let base = if p.rainbow {
        hue((rnd(col as u32, row as u32, 3) * 360.0 + epoch * 40.0).rem_euclid(360.0))
    } else {
        p.color
    };
    color::scale(base, 0.12 + 0.88 * level)
}

/// Flow: dots run along each row.
fn flow(p: &Params, _row: u8, col: u8, sp: usize, t: f32) -> Color {
    const STEP: [f32; 6] = [5.0, 10.0, 15.0, 20.0, 25.0, 12.0];
    let period = STEP[sp] * FRAME * 6.0;
    let head = (t / period).fract() * 20.0 - 1.0;
    let d = (col as f32 - head).abs();
    let level = (1.0 - d / 3.0).max(0.0);
    color::scale(p.color, 0.15 + 0.85 * level)
}

/// Key shadow: dark until a key is pressed (the preview shows the base).
fn key_shadow(p: &Params, _row: u8, _col: u8) -> Color {
    color::scale(p.color, 0.12)
}

/// Layers: a scripted diagonal sweep across the grid.
fn layers(p: &Params, row: u8, col: u8, sp: usize, t: f32) -> Color {
    const STEP: [f32; 6] = [2.0, 3.0, 5.0, 7.0, 9.0, 11.0];
    let period = STEP[sp] * FRAME * 4.0;
    let phase = (t / period).fract();
    let cell = ((col as f32 * 0.35 + row as f32) * 0.25).fract();
    let level = (1.0 - ((cell - phase).abs())).max(0.0);
    color::scale(p.color, 0.15 + 0.85 * level)
}

/// Sine: a dot born on a row runs out both ways, one column per step.
fn sine(p: &Params, row: u8, col: u8, sp: usize, t: f32) -> Color {
    const STEP: [f32; 6] = [5.0, 7.0, 10.0, 14.0, 17.0, 20.0];
    let period = STEP[sp] * FRAME * 6.0;
    let epoch = (t / period).floor();
    let src_row = (hash(epoch as u32, 11, 5) % 5) as f32;
    let mid = 8.5;
    let reach = (t / period).fract() * 9.0;
    let level = if (row as f32 - src_row).abs() < 0.6 && (col as f32 - mid).abs() <= reach {
        1.0
    } else {
        0.0
    };
    color::scale(p.color, 0.12 + 0.88 * level)
}

/// Colour grid: a fixed grid of preset colours.
fn colour_grid(p: &Params, row: u8, col: u8) -> Color {
    const PRESET: [[u8; 3]; 7] = [
        [255, 0, 0],
        [255, 127, 0],
        [255, 255, 0],
        [0, 255, 0],
        [0, 255, 255],
        [0, 0, 255],
        [255, 0, 255],
    ];
    if p.rainbow {
        let c = PRESET[(hash(col as u32, row as u32, 9) % 7) as usize];
        Color::from_rgb8(c[0], c[1], c[2])
    } else {
        p.color
    }
}

/// Snowfall / meteor / silent snow: dots falling down the columns.
fn snow(p: &Params, row: u8, col: u8, sp: usize, t: f32, rate: f32) -> Color {
    const STEP: [f32; 6] = [5.0, 10.0, 15.0, 20.0, 25.0, 12.0];
    let period = STEP[sp] * FRAME * 8.0 / rate;
    let off = rnd(col as u32, 1, 7) * period;
    let y = ((t + off) / period).fract() * 6.0;
    let d = (row as f32 - y).abs();
    let level = (1.0 - d / 1.2).max(0.0);
    color::scale(p.color, 0.12 + 0.88 * level)
}
