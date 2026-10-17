// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: JR Lanteigne <root@dnim.dev>
//! Per-mode backlight animations for the GUI preview.
//!
//! Ported from sharkfin's `app/src/lib/effects.ts` (GPL-3.0-or-later, © JR
//! Lanteigne) — a sketch of each vendor effect whose timing comes from the
//! firmware (see `docs/protocol.md`, "Timing"). Layers and Sine wave follow
//! the yc500 firmware scripts; the other shapes are the reference's
//! approximations, not the board's own routines.

use std::collections::HashMap;
use std::sync::LazyLock;

use iced::Color;

/// The firmware renders on a 5 ms tick.
pub const FRAME_MS: f32 = 5.0;

/// The settings a preview draws from.
#[derive(Clone, Copy)]
pub struct Params {
    pub mode: u8,
    /// Host speed 0..=4.
    pub speed: u8,
    pub brightness: u8,
    pub brightness_max: u8,
    /// Direction / variant index.
    pub option: u8,
    pub rainbow: bool,
    pub rgb: [u8; 3],
}

/// The firmware's preset palette; rainbow modes walk it.
const PRESETS: [[f32; 3]; 8] = [
    [255.0, 0.0, 0.0],
    [255.0, 128.0, 0.0],
    [255.0, 255.0, 0.0],
    [0.0, 255.0, 0.0],
    [0.0, 255.0, 255.0],
    [0.0, 0.0, 255.0],
    [255.0, 0.0, 255.0],
    [255.0, 255.0, 255.0],
];

// yc500 speed tables from the RT100 firmware, indexed by wire speed 0..=5.
const BREATH: [f32; 6] = [10.0, 40.0, 60.0, 80.0, 100.0, 200.0];
const BREATH_HOLD: f32 = 20.0;
const SPECTRUM: [f32; 6] = [5.0, 4.0, 3.0, 2.0, 1.0, 0.0];
const HUE: [f32; 6] = [5.0, 4.0, 3.0, 2.0, 1.0, 0.0];
const RIPLE: [f32; 6] = [5.0, 7.0, 9.0, 11.0, 13.0, 12.0];
const RADIANT: [f32; 6] = [5.0, 7.0, 9.0, 11.0, 13.0, 12.0];
const STARS: [f32; 6] = [5.0, 10.0, 15.0, 20.0, 25.0, 12.0];
const FLOW: [f32; 6] = [5.0, 10.0, 15.0, 20.0, 25.0, 12.0];
const LAYERS_T: [f32; 6] = [2.0, 3.0, 5.0, 7.0, 9.0, 11.0];
const SINE: [f32; 6] = [5.0, 7.0, 10.0, 14.0, 17.0, 20.0];

/// One full turn of the hue walker: six ramps of 255.
const HUE_CYCLE: f32 = 6.0 * 255.0;

const GRID_ROWS: usize = 6;
const GRID_COLS: usize = 18;

/// The Layers script (mode 9) as the RT100 firmware stores it.
const LAYERS_COUNTS: [u32; 33] = [
    4, 4, 6, 6, 5, 5, 5, 6, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 255, 13, 11, 11, 10, 8, 11,
    12, 8, 10, 0,
];
const LAYERS_CELLS: &[[u32; 2]] = &[
    [0, 0],
    [1, 0],
    [5, 17],
    [5, 16],
    [2, 0],
    [1, 1],
    [4, 16],
    [3, 16],
    [0, 2],
    [1, 2],
    [2, 1],
    [5, 15],
    [4, 15],
    [3, 15],
    [0, 3],
    [1, 3],
    [2, 2],
    [5, 14],
    [4, 14],
    [3, 14],
    [0, 4],
    [1, 4],
    [2, 3],
    [5, 13],
    [4, 13],
    [0, 5],
    [1, 5],
    [2, 4],
    [5, 12],
    [3, 13],
    [0, 6],
    [1, 6],
    [2, 5],
    [5, 11],
    [4, 12],
    [0, 7],
    [1, 7],
    [2, 6],
    [5, 10],
    [4, 11],
    [3, 11],
    [1, 10],
    [2, 10],
    [3, 10],
    [4, 10],
    [4, 9],
    [4, 8],
    [3, 7],
    [2, 7],
    [1, 8],
    [1, 9],
    [2, 9],
    [3, 9],
    [3, 8],
    [2, 8],
    [1, 10],
    [2, 10],
    [3, 10],
    [4, 10],
    [4, 9],
    [4, 8],
    [3, 7],
    [2, 7],
    [1, 8],
    [1, 9],
    [2, 9],
    [3, 9],
    [3, 8],
    [2, 8],
    [255, 255],
    [1, 10],
    [2, 10],
    [3, 10],
    [4, 10],
    [4, 9],
    [4, 8],
    [3, 7],
    [2, 7],
    [1, 8],
    [1, 9],
    [0, 8],
    [0, 9],
    [5, 9],
    [0, 10],
    [1, 11],
    [2, 11],
    [3, 11],
    [4, 11],
    [5, 10],
    [0, 7],
    [1, 7],
    [2, 6],
    [3, 6],
    [4, 7],
    [0, 11],
    [1, 12],
    [2, 12],
    [4, 12],
    [5, 11],
    [0, 6],
    [1, 6],
    [2, 5],
    [3, 5],
    [4, 6],
    [5, 6],
    [0, 12],
    [1, 13],
    [2, 13],
    [3, 13],
    [5, 12],
    [0, 5],
    [1, 5],
    [2, 4],
    [3, 4],
    [4, 5],
    [0, 13],
    [4, 13],
    [5, 13],
    [0, 4],
    [1, 4],
    [2, 3],
    [3, 3],
    [4, 4],
    [0, 14],
    [1, 14],
    [2, 14],
    [3, 14],
    [4, 14],
    [5, 14],
    [0, 3],
    [1, 3],
    [2, 2],
    [3, 2],
    [4, 3],
    [0, 15],
    [1, 15],
    [2, 15],
    [3, 15],
    [4, 15],
    [5, 15],
    [0, 2],
    [1, 2],
    [2, 1],
    [3, 1],
    [4, 2],
    [5, 3],
    [0, 16],
    [1, 16],
    [2, 16],
    [3, 16],
    [4, 16],
    [5, 16],
    [1, 1],
    [5, 2],
    [0, 17],
    [1, 17],
    [3, 17],
    [5, 17],
    [0, 0],
    [1, 0],
    [2, 0],
    [3, 0],
    [4, 0],
    [5, 0],
];
const LAYERS_PAUSE: u32 = 5;
const LAYERS_REST: u32 = 25;

struct LayerScript {
    /// cell (row*18+col) -> the steps that light it, with their colour phase.
    cells: HashMap<usize, Vec<(u32, u32)>>,
    total: u32,
}

/// Per grid cell, the steps at which the script lights it and in which colour
/// phase, plus the cycle length in steps.
static LAYERS: LazyLock<LayerScript> = LazyLock::new(|| {
    let mut cells: HashMap<usize, Vec<(u32, u32)>> = HashMap::new();
    let mut step = 0u32;
    let mut phase = 0u32;
    let mut at = 0usize;
    for &n in &LAYERS_COUNTS {
        if n == 0 {
            break;
        }
        if n == 255 {
            step += LAYERS_PAUSE;
            phase += 1;
            at += 1; // the (255, 255) placeholder pair
            continue;
        }
        let mut i = 0;
        while i < n && at < LAYERS_CELLS.len() {
            let [row, col] = LAYERS_CELLS[at];
            cells
                .entry(row as usize * GRID_COLS + col as usize)
                .or_default()
                .push((step, phase));
            at += 1;
            i += 1;
        }
        step += 1;
    }
    LayerScript {
        cells,
        total: step + LAYERS_REST,
    }
});

/// Whether this mode's preview can change from one frame to the next.
///
/// The modes whose arm in [`color_at`] ignores the frame are the static ones.
/// This is deliberately conservative — a mode whose arm reads the frame but
/// happens not to change at a given frame is still reported as animating — and
/// the reverse is what matters: reporting a moving mode as static would freeze
/// its preview, which `a_moving_preview_always_keeps_its_timer` forbids. Used
/// to stop the animation timer for a static mode, which would otherwise rebuild
/// the whole preview 25 times a second for a picture that never changes.
pub fn animates(p: &Params) -> bool {
    !matches!(p.mode, 1 | 19)
}

/// The colour of one key at a frame.
pub fn color_at(p: &Params, x: f32, y: f32, index: usize, frame: f32) -> Color {
    let wire = wire_speed(p.speed) as usize;
    let gain = if p.brightness_max > 0 {
        f32::from(p.brightness) / f32::from(p.brightness_max)
    } else {
        1.0
    };
    let step_len = step_frames(p.mode, wire);
    let step = (frame / step_len).floor();
    let step_phase = (frame % step_len) / step_len;
    let turns = hue_turns(p.mode, wire, frame);
    let row = ((y * GRID_ROWS as f32).floor() as usize).min(GRID_ROWS - 1);
    let col = ((x * GRID_COLS as f32).floor() as usize).min(GRID_COLS - 1);

    let c: [f32; 3] = match p.mode {
        1 | 19 => {
            if p.rainbow {
                hsv(x, 1.0, 1.0)
            } else {
                [
                    f32::from(p.rgb[0]),
                    f32::from(p.rgb[1]),
                    f32::from(p.rgb[2]),
                ]
            }
        }
        2 => {
            let (level, cycle) = breath(wire, frame);
            scale(base(p, cycle), glow(level))
        }
        3 => hsv(turns, 1.0, 1.0),
        4 => {
            let along = match p.option % 4 {
                0 => x,
                1 => 1.0 - x,
                2 => y,
                _ => 1.0 - y,
            };
            if p.rainbow {
                hsv(along + turns, 1.0, 1.0)
            } else {
                scale(
                    [
                        f32::from(p.rgb[0]),
                        f32::from(p.rgb[1]),
                        f32::from(p.rgb[2]),
                    ],
                    glow(0.5 + 0.5 * (cos_deg(along + turns))),
                )
            }
        }
        5 => {
            // A ring grows one cell per step from a random key, then another.
            let life = 24.0;
            let burst = (step / life).floor();
            let ox = noise(1, burst as i32);
            let oy = noise(2, burst as i32);
            let d = (((x - ox) * GRID_COLS as f32).powi(2) + ((y - oy) * GRID_ROWS as f32).powi(2))
                .sqrt();
            let radius = (step % life) + step_phase;
            scale(
                base(p, burst),
                glow((1.0 - (d - radius).abs() * 0.8).max(0.0)),
            )
        }
        6 => {
            // A new star every step; each burns for a while and fades.
            let life = 40.0;
            let born = step - ((noise(index as i32, 0) * life) % life).floor();
            let age = ((step - born) % life + life) % life;
            let level = if age < 6.0 {
                age / 6.0
            } else {
                (1.0 - (age - 6.0) / (life - 6.0)).max(0.0)
            };
            let col = if p.rainbow {
                hsv(noise(index as i32, (step / life).floor() as i32), 1.0, 1.0)
            } else {
                [
                    f32::from(p.rgb[0]),
                    f32::from(p.rgb[1]),
                    f32::from(p.rgb[2]),
                ]
            };
            scale(col, glow(level))
        }
        7 => {
            // A head runs the rows, zigzag or spiral, with a tail.
            let cells = (GRID_ROWS * GRID_COLS) as f32;
            let spiral = p.option == 1;
            let pos = if spiral {
                let ring = row
                    .min(GRID_ROWS - 1 - row)
                    .min(col)
                    .min(GRID_COLS - 1 - col);
                ring as f32 * 40.0
                    + if row <= GRID_ROWS / 2 {
                        col as f32
                    } else {
                        (GRID_COLS * 2) as f32 - col as f32
                    }
            } else if row % 2 == 1 {
                (row * GRID_COLS) as f32 + (GRID_COLS - 1 - col) as f32
            } else {
                (row * GRID_COLS) as f32 + col as f32
            };
            let head = (step % cells) + step_phase;
            let dist = ((pos - head) % cells + cells) % cells;
            scale(
                base(p, (step / cells).floor()),
                glow(if dist > cells - 6.0 {
                    (dist - (cells - 6.0)) / 6.0
                } else {
                    0.0
                }),
            )
        }
        8 => {
            // Reactive: the preview presses one random key per step.
            let hit = (noise(3, step as i32) * 100.0).floor();
            if index as f32 % 100.0 == hit {
                scale(base(p, step), glow(1.0 - step_phase))
            } else {
                [0.0, 0.0, 0.0]
            }
        }
        9 => {
            // The firmware's script: cells stay lit until the cycle rests.
            let cycle = (step / LAYERS.total as f32).floor();
            let now = (step as u32) % LAYERS.total;
            let mut lit: Option<(u32, u32)> = None;
            if let Some(items) = LAYERS.cells.get(&(row * GRID_COLS + col)) {
                for &(s, ph) in items {
                    if s <= now {
                        lit = Some((s, ph));
                    }
                }
            }
            match lit {
                Some((_, ph)) => {
                    if p.rainbow {
                        PRESETS[((cycle as u32 * 2 + ph) % 6) as usize]
                    } else {
                        [
                            f32::from(p.rgb[0]),
                            f32::from(p.rgb[1]),
                            f32::from(p.rgb[2]),
                        ]
                    }
                }
                None => [0.0, 0.0, 0.0],
            }
        }
        10 => {
            // A dot is born at column 6 on a row that follows a sine and runs
            // out both ways one column per step.
            let mut level = 0.0;
            for age in 0..20i32 {
                let born = step as i32 - age;
                if born < 0 {
                    break;
                }
                let r = (2.5 + 2.5 * ((born as f32 / 30.0) * std::f32::consts::TAU).sin()).round();
                if r as usize != row {
                    continue;
                }
                if col as i32 == 6 - age || col as i32 == 6 + age {
                    level = 1.0;
                }
            }
            scale(base(p, (step / 30.0).floor()), level)
        }
        11 => {
            // Rings breathing out of, or into, the centre.
            let d = (((x - 0.5).powi(2) + ((y - 0.5) * 0.4).powi(2)).sqrt()) / 0.6;
            let dir = if p.option == 1 { -1.0 } else { 1.0 };
            let phase = d * 2.0 - dir * turns;
            let col = if p.rainbow {
                hsv(phase, 1.0, 1.0)
            } else {
                [
                    f32::from(p.rgb[0]),
                    f32::from(p.rgb[1]),
                    f32::from(p.rgb[2]),
                ]
            };
            scale(col, 0.45 + 0.55 * glow(0.5 + 0.5 * cos_deg(phase)))
        }
        12 => {
            let dir = if p.option == 1 { -1.0 } else { 1.0 };
            hsv(x * dir - turns, 1.0, 1.0)
        }
        14 => {
            let a = ((y - 0.5) * 0.4).atan2(x - 0.5) / std::f32::consts::TAU;
            hsv(a + (step + step_phase) / 24.0, 1.0, 1.0)
        }
        15 => {
            let dir = if p.option == 1 { -1.0 } else { 1.0 };
            let a = (((y - 0.5) * 0.4).atan2(x - 0.5) / std::f32::consts::TAU + 1.0) % 1.0;
            let head = ((dir * turns) % 1.0 + 1.0) % 1.0;
            let dist = ((a - head) % 1.0 + 1.0) % 1.0;
            let edge = if ((x - 0.5).abs() * 2.0).max((y - 0.5).abs() * 2.0) > 0.7 {
                1.0
            } else {
                0.2
            };
            scale(
                base(p, turns.floor()),
                glow((1.0 - dist * 4.0).max(0.0) * edge),
            )
        }
        16 => {
            let cell = (x * 6.0).floor() as i32 + 6 * (y * 3.0).floor() as i32;
            let epoch = (frame / 120.0).floor() as i32;
            PRESETS[(noise(cell, epoch) * 7.0).floor().min(6.0) as usize]
        }
        17 => {
            let fall_frames = 12.0;
            let drop = frame / (fall_frames * GRID_ROWS as f32) + noise(col as i32, 0);
            let fall = ((drop % 1.0) + 1.0) % 1.0;
            let d = (y - fall).abs();
            let flake = if p.rainbow {
                PRESETS[(noise(col as i32, drop.floor() as i32 + 1) * 7.0)
                    .floor()
                    .min(6.0) as usize]
            } else {
                [
                    f32::from(p.rgb[0]),
                    f32::from(p.rgb[1]),
                    f32::from(p.rgb[2]),
                ]
            };
            let factor = if noise(col as i32, 1) > 0.4 { 1.0 } else { 0.3 };
            scale(flake, glow((1.0 - d * 5.0).max(0.0) * factor))
        }
        18 => {
            let fall_frames = 8.0;
            let drop = frame / (fall_frames * GRID_ROWS as f32) + noise(col as i32, 0);
            let head = ((drop % 1.5) + 1.5) % 1.5;
            let d = head - y;
            let active = noise(col as i32, (drop / 1.5).floor() as i32) > 0.6;
            scale(
                base(p, col as f32),
                glow(if active && (0.0..0.5).contains(&d) {
                    1.0 - d * 2.0
                } else {
                    0.0
                }),
            )
        }
        _ => [
            f32::from(p.rgb[0]),
            f32::from(p.rgb[1]),
            f32::from(p.rgb[2]),
        ],
    };

    let c = scale(c, gain);
    Color::from_rgb8(
        c[0].round().clamp(0.0, 255.0) as u8,
        c[1].round().clamp(0.0, 255.0) as u8,
        c[2].round().clamp(0.0, 255.0) as u8,
    )
}

/// yc500 host speed 0..=4 to the wire byte (1..=5, inverted).
fn wire_speed(host: u8) -> u8 {
    5 - host.min(4)
}

fn tab(t: &[f32; 6], wire: usize) -> f32 {
    t[wire.min(t.len() - 1)]
}

/// Frames between steps of a stepped effect.
fn step_frames(mode: u8, wire: usize) -> f32 {
    match mode {
        5 => tab(&RIPLE, wire),
        14 => tab(&RADIANT, wire),
        6 => tab(&STARS, wire),
        7 => tab(&FLOW, wire),
        9 => tab(&LAYERS_T, wire),
        10 => tab(&SINE, wire),
        _ => 8.0,
    }
}

/// Hue turns completed by a frame, for the modes that walk the palette.
fn hue_turns(mode: u8, wire: usize, frame: f32) -> f32 {
    if mode == 3 {
        (frame / 2.0).floor() * tab(&SPECTRUM, wire) / HUE_CYCLE
    } else {
        frame * (tab(&HUE, wire) + 1.0) / HUE_CYCLE
    }
}

/// How long a full breath takes, in frames, both directions.
fn breath_frames(wire: usize) -> f32 {
    2.0 * (tab(&BREATH, wire) + BREATH_HOLD)
}

fn cos_deg(turns: f32) -> f32 {
    (turns * std::f32::consts::TAU).cos()
}

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let hh = ((h % 1.0) + 1.0) % 1.0;
    let i = (hh * 6.0).floor();
    let f = hh * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let u = v * (1.0 - (1.0 - f) * s);
    let pick: [[f32; 3]; 6] = [
        [v, u, p],
        [q, v, p],
        [p, v, u],
        [p, q, v],
        [u, p, v],
        [v, p, q],
    ];
    let c = pick[(i as usize) % 6];
    [c[0] * 255.0, c[1] * 255.0, c[2] * 255.0]
}

fn scale(c: [f32; 3], k: f32) -> [f32; 3] {
    let m = k.clamp(0.0, 1.0);
    [c[0] * m, c[1] * m, c[2] * m]
}

/// An animated level, lifted so half-lit LEDs read as lit on a screen.
fn glow(level: f32) -> f32 {
    level.clamp(0.0, 1.0).powf(0.6)
}

/// Deterministic noise in 0..1 from a key index and a step number.
fn noise(i: i32, n: i32) -> f32 {
    let mut h = i
        .wrapping_mul(374_761_393)
        .wrapping_add(n.wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) as u32) as f32 / 4_294_967_296.0
}

/// The colour a fixed-colour mode paints with, or a preset when the firmware
/// walks its palette.
fn base(p: &Params, step: f32) -> [f32; 3] {
    if !p.rainbow {
        [
            f32::from(p.rgb[0]),
            f32::from(p.rgb[1]),
            f32::from(p.rgb[2]),
        ]
    } else {
        PRESETS[((step as i32 % 7) + 7) as usize % 7]
    }
}

/// Triangle 0..1..0 with a flat top and bottom, as the yc500 fader.
fn breath(wire: usize, frame: f32) -> (f32, f32) {
    let total = breath_frames(wire);
    let cycle = (frame / total).floor();
    let f = frame % total;
    let ramp = tab(&BREATH, wire);
    let hold = BREATH_HOLD;
    if f < ramp {
        (f / ramp, cycle)
    } else if f < ramp + hold {
        (1.0, cycle)
    } else if f < 2.0 * ramp + hold {
        (1.0 - (f - ramp - hold) / ramp, cycle)
    } else {
        (0.0, cycle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded() {
        for i in 0..50 {
            let n = noise(i, 7);
            assert!((0.0..1.0).contains(&n), "{n}");
        }
    }

    #[test]
    fn static_returns_the_colour() {
        let p = Params {
            mode: 1,
            speed: 2,
            brightness: 4,
            brightness_max: 4,
            option: 0,
            rainbow: false,
            rgb: [255, 0, 0],
        };
        let c = color_at(&p, 0.5, 0.5, 0, 0.0);
        assert_eq!((c.r, c.g, c.b), (1.0, 0.0, 0.0));
    }

    #[test]
    fn layers_script_is_built() {
        assert!(!LAYERS.cells.is_empty());
        assert!(LAYERS.total > LAYERS_REST);
    }

    /// The animation timer is gated on this. Reporting a moving mode as static
    /// would freeze its preview, so that direction is asserted here against
    /// `color_at` on a dense grid; the other direction only costs a redraw of a
    /// picture that happens not to change.
    #[test]
    fn a_moving_preview_always_keeps_its_timer() {
        let params = |mode, rainbow| Params {
            mode,
            speed: 2,
            brightness: 4,
            brightness_max: 4,
            option: 0,
            rainbow,
            rgb: [255, 0, 0],
        };
        // A dense grid and several frames spanning many steps, so a sparse or
        // slow mode (star dots lights only a few keys at a time) is still seen
        // moving.
        let moves = |mode, rainbow| {
            let p = params(mode, rainbow);
            (0..GRID_ROWS * 2)
                .flat_map(|r| (0..GRID_COLS * 2).map(move |c| (r, c)))
                .any(|(r, c)| {
                    let x = (r as f32 + 0.5) / (GRID_ROWS * 2) as f32;
                    let y = (c as f32 + 0.5) / (GRID_COLS * 2) as f32;
                    [0.0f32, 400.0, 1600.0].windows(2).any(|f| {
                        (0..64).any(|index| {
                            color_at(&p, x, y, index, f[0]) != color_at(&p, x, y, index, f[1])
                        })
                    })
                })
        };
        for mode in 1..=19u8 {
            for rainbow in [false, true] {
                if moves(mode, rainbow) {
                    assert!(
                        animates(&params(mode, rainbow)),
                        "mode {mode} (rainbow {rainbow}) moves, so it must keep a timer"
                    );
                }
            }
        }
        // And the reason the gate exists: a static mode does not animate.
        assert!(!animates(&params(1, false)));
        assert!(!animates(&params(1, true)));
    }
}
