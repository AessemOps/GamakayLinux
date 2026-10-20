// SPDX-License-Identifier: GPL-3.0-or-later
//! `lk67ctl light`: read and set the backlight and edge light.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use lk67_core::guard::Guard;
use lk67_core::light::{EdgeMode, LK67_FLAGS, LedParam, LedState, Mode};
use lk67_core::transport::Session;
use lk67_hidraw::HidrawDevice;

use crate::{Cli, check_writable, select_device};

#[derive(Subcommand)]
pub enum LightCmd {
    /// Show the current backlight and edge light.
    Get,
    /// Set the backlight.
    Set(LightArgs),
    /// The edge (side) light — the board's underglow.
    #[command(visible_alias = "underglow")]
    Edge {
        #[command(subcommand)]
        action: EdgeCmd,
    },
    /// Turn the backlight on (clear the options bit).
    On,
    /// Turn the backlight off (set the options bit; not a black write).
    Off,
}

#[derive(Subcommand)]
pub enum EdgeCmd {
    /// Show the current edge light.
    Get,
    /// Set the edge light.
    Set(LightArgs),
}

#[derive(Args, Clone, Debug, Default)]
pub struct LightArgs {
    /// Mode name (e.g. static, wave, spectrum) or wire number.
    #[arg(long)]
    pub mode: Option<String>,
    /// Colour as `#RRGGBB` or `RRGGBB`.
    #[arg(long, value_parser = parse_color)]
    pub color: Option<[u8; 3]>,
    /// Brightness, 0..4.
    #[arg(long)]
    pub brightness: Option<u8>,
    /// Speed, 0..4 (0 slowest). Inverted on the wire for the backlight.
    #[arg(long)]
    pub speed: Option<u8>,
    /// Direction / effect option, 0..3 for modes that use one.
    #[arg(long)]
    pub direction: Option<u8>,
    /// Use the rainbow flag instead of a fixed colour.
    #[arg(long, conflicts_with = "fixed")]
    pub rainbow: bool,
    /// Use a fixed colour (the default).
    #[arg(long)]
    pub fixed: bool,
}

pub fn parse_color(s: &str) -> Result<[u8; 3], String> {
    lk67_core::color::parse_hex(s).ok_or_else(|| format!("colour must be 6 hex digits, got {s:?}"))
}

pub fn run(cli: &Cli, action: Option<&LightCmd>) -> Result<()> {
    match action {
        None | Some(LightCmd::Get) => show(cli),
        Some(LightCmd::Set(args)) => set_backlight(cli, args),
        Some(LightCmd::Edge { action }) => match action {
            EdgeCmd::Get => show(cli),
            EdgeCmd::Set(args) => set_edge(cli, args),
        },
        Some(LightCmd::On) => toggle_backlight(cli, false),
        Some(LightCmd::Off) => toggle_backlight(cli, true),
    }
}

fn show(cli: &Cli) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    let led = s.led_param().context("reading backlight (0x87)")?;
    print_state("backlight", &led, false);
    if board_has_edge(&mut s) {
        match s.edge_param() {
            Ok(e) => print_state("edge", &e, true),
            Err(_) => println!("edge:       (unreadable)"),
        }
    } else {
        println!("edge:       (none on this board)");
    }
    Ok(())
}

/// Presence of an edge light is a property of the board, not of a GET reply:
/// the firmware answers `0x88` for parts it lacks, and the LK67's firmware
/// answers with real stored data (`[mode, speed, brightness, flags, R, G, B]`).
/// The record follows the hardware — see the field's note and
/// docs/protocol.md -> "Underglow (edge light)".
pub(crate) fn board_has_edge(s: &mut Session<'_>) -> bool {
    s.device_id()
        .ok()
        .and_then(lk67_core::device::by_id)
        .map(|r| r.side_light)
        .unwrap_or(false)
}

fn set_backlight(cli: &Cli, args: &LightArgs) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    check_writable(cli, &mut s)?;
    let cur = s.led_param()?;
    let p = merge(&cur, args, false)?;
    s.set_backlight(&p, LK67_FLAGS)?;
    if cli.dry_run {
        for pkt in s.withheld() {
            print_plan(pkt);
        }
    } else {
        print_param("backlight", &p, false);
    }
    Ok(())
}

fn set_edge(cli: &Cli, args: &LightArgs) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    check_writable(cli, &mut s)?;
    if !board_has_edge(&mut s) && !cli.force {
        bail!("this board has no edge light (pass --force to try anyway)");
    }
    let cur = s.edge_param()?;
    let p = merge(&cur, args, true)?;
    s.set_edge(&p, LK67_FLAGS)?;
    if cli.dry_run {
        for pkt in s.withheld() {
            print_plan(pkt);
        }
    } else {
        print_param("edge", &p, true);
    }
    Ok(())
}

fn toggle_backlight(cli: &Cli, off: bool) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    check_writable(cli, &mut s)?;
    let o = s.set_backlight_off(off)?;
    if cli.dry_run {
        for pkt in s.withheld() {
            print_plan(pkt);
        }
    } else {
        println!("backlight:  {}", if o.backlight_off { "off" } else { "on" });
    }
    Ok(())
}

/// Merge the requested fields over the current state; unspecified fields keep
/// their current value.
///
/// A colour is a request for *that colour*, so it clears the rainbow flag even
/// when `--fixed` is not given: with the flag left set the board shows its own
/// rainbow palette and ignores the colour. `--rainbow` sets it explicitly, and
/// with neither flag nor colour the stored value is preserved.
pub(crate) fn merge(cur: &LedState, args: &LightArgs, edge: bool) -> Result<LedParam> {
    let mode = match &args.mode {
        Some(m) => resolve_mode(m, edge)?,
        None => cur.mode,
    };
    if let Some(b) = args.brightness.filter(|b| *b > 4) {
        bail!("brightness {b} is out of range (0..4)");
    }
    if let Some(sp) = args.speed.filter(|sp| *sp > 4) {
        bail!("speed {sp} is out of range (0..4)");
    }
    if let Some(d) = args.direction.filter(|d| *d > 3) {
        bail!("direction {d} is out of range (0..3)");
    }
    Ok(LedParam {
        mode,
        speed: args.speed.unwrap_or(cur.speed),
        brightness: args.brightness.unwrap_or(cur.brightness),
        option: args.direction.unwrap_or(cur.option),
        rainbow: if args.rainbow {
            true
        } else if args.fixed || args.color.is_some() {
            false
        } else {
            cur.flags == LK67_FLAGS.rainbow
        },
        rgb: args.color.unwrap_or(cur.rgb),
    })
}

fn resolve_mode(s: &str, edge: bool) -> Result<u8> {
    if edge {
        // The edge light resolves names and codes by its own rule, from core,
        // so the GUI's picker cannot disagree with this. Its off state is code
        // 0, which the backlight's range rejects.
        if let Some(code) = lk67_core::light::edge_mode_code(s) {
            return Ok(code);
        }
        if let Ok(n) = s.trim().parse::<u8>() {
            bail!(
                "unknown edge mode code {n} (this firmware implements 0..={})",
                lk67_core::light::MAX_MODE_CODE
            );
        }
        bail!("unknown edge mode {s:?} (try: off, static, breathing, neon, wave, snake)")
    }
    if let Some(m) = Mode::from_name(s) {
        return Ok(m.code());
    }
    if let Ok(n) = s.trim().parse::<u8>() {
        // A numeric code is only accepted when it names a mode the board
        // implements: writing an arbitrary byte is a silent misconfiguration.
        if lk67_core::light::is_mode_code(n) {
            return Ok(n);
        }
        bail!(
            "unknown backlight mode code {n} (this firmware implements 1..={})",
            lk67_core::light::MAX_MODE_CODE
        );
    }
    bail!("unknown mode {s:?} (try: static, wave, spectrum, breathing, ripple, ...)")
}

/// The requested state, printed because a write cannot be read back in the
/// same exchange (see `docs/protocol.md`).
pub(crate) fn print_param(label: &str, p: &LedParam, edge: bool) {
    let name = if edge {
        EdgeMode::name_for_code(p.mode)
    } else {
        Mode::name_for_code(p.mode)
    };
    let kind = if p.rainbow { "rainbow" } else { "fixed" };
    println!(
        "{label}: {name:<10} speed {} brightness {} {kind} #{:02x}{:02x}{:02x} (sent)",
        p.speed, p.brightness, p.rgb[0], p.rgb[1], p.rgb[2]
    );
}

/// Print a read-back state. `edge` picks the mode table: the two lights share
/// wire codes but name them differently (3 is `spectrum` for the backlight and
/// `neon` for the underglow), so naming an edge state from the backlight table
/// would print a mode the same tool refuses to accept back.
pub(crate) fn print_state(label: &str, s: &LedState, edge: bool) {
    let color = format!("#{:02x}{:02x}{:02x}", s.rgb[0], s.rgb[1], s.rgb[2]);
    let kind = match s.preset {
        Some(p) => format!("preset {p}"),
        None => format!("flags {}", s.flags),
    };
    let name = if edge {
        s.edge_mode_name()
    } else {
        s.mode_name()
    };
    println!(
        "{label}: {name:<10} speed {} brightness {} {} {color}",
        s.speed, s.brightness, kind,
    );
}

pub(crate) fn print_plan(pkt: &[u8; 64]) {
    let hex: Vec<String> = pkt.iter().take(16).map(|b| format!("{b:02x}")).collect();
    println!("dry run: {}", hex.join(" "));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_args_are_six_hex_digits() {
        assert_eq!(parse_color("#ff0000"), Ok([0xff, 0x00, 0x00]));
        assert_eq!(parse_color("ff0000"), Ok([0xff, 0x00, 0x00]));
        assert!(parse_color("#ff00").is_err());
        // Six *bytes* of non-ASCII must be an error, not a char-boundary panic.
        assert!(parse_color("€€").is_err());
        assert!(parse_color("ééé").is_err());
    }

    fn rainbow_state() -> LedState {
        LedState {
            mode: 1,
            speed: 3,
            brightness: 4,
            option: 0,
            flags: LK67_FLAGS.rainbow,
            rgb: [0x12, 0x34, 0x56],
            preset: None,
        }
    }

    /// The board ignores RGB while the rainbow bit is set, so writing a colour
    /// has to clear it — otherwise `--color red` shows a rainbow.
    #[test]
    fn a_colour_clears_the_rainbow_flag() {
        let cur = rainbow_state();
        let args = LightArgs {
            color: Some([0xff, 0x00, 0x00]),
            ..Default::default()
        };
        let p = merge(&cur, &args, true).unwrap();
        assert_eq!(p.rgb, [0xff, 0x00, 0x00]);
        assert!(!p.rainbow, "the colour must be the one the board shows");
        assert_eq!(p.speed, 3, "fields not offered are preserved");

        let untouched = merge(&cur, &LightArgs::default(), true).unwrap();
        assert!(untouched.rainbow, "untouched flags are preserved");

        let forced = merge(
            &cur,
            &LightArgs {
                rainbow: true,
                ..Default::default()
            },
            true,
        )
        .unwrap();
        assert!(forced.rainbow);

        let fixed = merge(
            &cur,
            &LightArgs {
                fixed: true,
                ..Default::default()
            },
            true,
        )
        .unwrap();
        assert!(!fixed.rainbow);
    }
}
