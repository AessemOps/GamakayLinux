// SPDX-License-Identifier: GPL-3.0-or-later
//! `lk67ctl macro`: list, set and delete onboard macros.
//!
//! A macro is bound to a key with `lk67ctl keymap set --slot N --to
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

use lk67_core::guard::Guard;

use lk67_core::macros::{MACRO_SLOTS, Macro, describe, parse_steps};
use lk67_core::transport::Session;
use lk67_hidraw::HidrawDevice;

use crate::{Cli, check_writable, select_device};

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
    check_writable(cli, &mut s)?;
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
            "bind it with: lk67ctl keymap set --slot N --to macro:{}",
            args.slot
        );
    }
    Ok(())
}

fn delete(cli: &Cli, args: &DeleteArgs) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    check_writable(cli, &mut s)?;
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
