// SPDX-License-Identifier: GPL-3.0-or-later
//! `gamakay macro`: list, set and delete onboard macros.
//!
//! A macro is bound to a key with `gamakay keymap set --slot N --to
//! macro:INDEX` (see `keymap`). Events are given with `--steps`, a comma
//! separated list:
//!
//! ```text
//! A+@20, A-@20          press A, wait 20 ms, release A, wait 20 ms
//! mouse:0+@10           left button down
//! move:5:-3@10          move the pointer (dx, dy)
//! ```

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use gamakay_core::guard::Guard;
use gamakay_core::keycode;
use gamakay_core::macros::{MACRO_SLOTS, Macro, MacroEvent};
use gamakay_core::transport::Session;
use gamakay_hidraw::HidrawDevice;

use crate::{Cli, select_device};

#[derive(Subcommand)]
pub enum MacroCmd {
    /// List the onboard macros.
    List(ListArgs),
    /// Write a macro into a slot.
    Set(SetArgs),
    /// Clear a macro slot.
    Delete(DeleteArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Only this slot; default is all 50.
    #[arg(long)]
    pub slot: Option<u8>,
}

#[derive(Args)]
pub struct SetArgs {
    /// Macro slot, 0..49.
    pub slot: u8,
    /// Repeat count.
    #[arg(long, default_value_t = 1)]
    pub repeat: u16,
    /// The event sequence (see the command help).
    #[arg(long)]
    pub steps: String,
}

#[derive(Args)]
pub struct DeleteArgs {
    /// Macro slot, 0..49.
    pub slot: u8,
}

pub fn run(cli: &Cli, action: &MacroCmd) -> Result<()> {
    match action {
        MacroCmd::List(a) => list(cli, a),
        MacroCmd::Set(a) => set(cli, a),
        MacroCmd::Delete(a) => delete(cli, a),
    }
}

fn list(cli: &Cli, args: &ListArgs) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    let slots: Vec<u8> = match args.slot {
        Some(n) => vec![n],
        None => (0..MACRO_SLOTS).collect(),
    };
    let mut found = 0;
    for slot in slots {
        let m = s
            .read_macro(slot)
            .with_context(|| format!("reading macro {slot}"))?;
        if m.is_empty() && args.slot.is_none() {
            continue;
        }
        println!("macro {slot:>2}: repeat {} — {}", m.repeat, describe(&m));
        found += 1;
    }
    if found == 0 {
        println!("(no macros set)");
    }
    Ok(())
}

fn set(cli: &Cli, args: &SetArgs) -> Result<()> {
    require_verified(cli)?;
    let events = parse_steps(&args.steps)?;
    if events.is_empty() {
        bail!("--steps is empty");
    }
    let m = Macro {
        repeat: args.repeat,
        events,
    };
    // Validate the encoding before opening the device.
    m.to_blob()?;

    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    s.write_macro(args.slot, &m)?;
    if cli.dry_run {
        for pkt in s.withheld() {
            let hex: Vec<String> = pkt.iter().take(16).map(|b| format!("{b:02x}")).collect();
            println!("dry run: {}", hex.join(" "));
        }
    } else {
        println!(
            "macro {}: repeat {} — {}",
            args.slot,
            m.repeat,
            describe(&m)
        );
        println!(
            "bind it with: gamakay keymap set --slot N --to macro:{}",
            args.slot
        );
    }
    Ok(())
}

fn delete(cli: &Cli, args: &DeleteArgs) -> Result<()> {
    require_verified(cli)?;
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    s.delete_macro(args.slot)?;
    if cli.dry_run {
        for pkt in s.withheld() {
            let hex: Vec<String> = pkt.iter().take(16).map(|b| format!("{b:02x}")).collect();
            println!("dry run: {}", hex.join(" "));
        }
    } else {
        println!("macro {} cleared", args.slot);
    }
    Ok(())
}

/// The macro write is not yet verified on the LK67: the slot does not read
/// back (0x8B returns zeros), so either the opcode or the flow differs. Refuse
/// by default rather than guess at opcodes that could hit a live register.
fn require_verified(cli: &Cli) -> Result<()> {
    if cli.force {
        return Ok(());
    }
    bail!(
        "macro writes are not yet verified on the LK67 (the slot does not read back); \
         re-run with --force to send anyway — see docs/protocol.md"
    )
}

fn describe(m: &Macro) -> String {
    if m.events.is_empty() {
        return "(empty)".to_string();
    }
    m.events
        .iter()
        .map(describe_event)
        .collect::<Vec<_>>()
        .join(", ")
}

fn describe_event(e: &MacroEvent) -> String {
    match *e {
        MacroEvent::Key {
            usage,
            pressed,
            delay_ms,
        } => {
            let name = keycode::name(usage)
                .map(str::to_string)
                .unwrap_or_else(|| format!("0x{usage:02X}"));
            let sign = if pressed { "+" } else { "-" };
            format!("{name}{sign}@{delay_ms}")
        }
        MacroEvent::MouseButton {
            button,
            pressed,
            delay_ms,
        } => {
            let sign = if pressed { "+" } else { "-" };
            format!("mouse:{button}{sign}@{delay_ms}")
        }
        MacroEvent::MouseMove { dx, dy, delay_ms } => {
            format!("move:{dx}:{dy}@{delay_ms}")
        }
    }
}

/// Parse the `--steps` DSL.
fn parse_steps(s: &str) -> Result<Vec<MacroEvent>> {
    let mut events = Vec::new();
    for raw in s.split(',') {
        let tok = raw.trim();
        if tok.is_empty() {
            continue;
        }
        let (body, delay) = match tok.rsplit_once('@') {
            Some((b, d)) => (
                b.trim(),
                d.trim()
                    .parse::<u16>()
                    .with_context(|| format!("bad delay in {tok:?}"))?,
            ),
            None => (tok, 0u16),
        };
        if let Some(rest) = body.strip_prefix("move:") {
            let (dx, dy) = rest
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("move needs dx:dy, got {rest:?}"))?;
            let dx: i8 = dx.trim().parse().context("move dx")?;
            let dy: i8 = dy.trim().parse().context("move dy")?;
            if delay > 255 {
                bail!("move delay max is 255 ms");
            }
            events.push(MacroEvent::MouseMove {
                dx,
                dy,
                delay_ms: delay as u8,
            });
            continue;
        }
        if let Some(rest) = body.strip_prefix("mouse:") {
            let (n, pressed) = split_press(rest)?;
            let button: u8 = n.parse().context("mouse button number (0..4)")?;
            events.push(MacroEvent::MouseButton {
                button,
                pressed,
                delay_ms: delay,
            });
            continue;
        }
        let (name, pressed) = split_press(body)?;
        let usage = keycode::usage(name).ok_or_else(|| anyhow::anyhow!("unknown key {name:?}"))?;
        events.push(MacroEvent::Key {
            usage,
            pressed,
            delay_ms: delay,
        });
    }
    Ok(events)
}

fn split_press(s: &str) -> Result<(&str, bool)> {
    if let Some(n) = s.strip_suffix('+') {
        Ok((n.trim(), true))
    } else if let Some(n) = s.strip_suffix('-') {
        Ok((n.trim(), false))
    } else {
        bail!("event {s:?} needs + (down) or - (up)")
    }
}
