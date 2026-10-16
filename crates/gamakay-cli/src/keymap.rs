// SPDX-License-Identifier: GPL-3.0-or-later
//! `gamakay keymap`: read, remap and reset the onboard layers.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use gamakay_core::guard::Guard;
use gamakay_core::keycode;
use gamakay_core::keymap::{Slot, SlotValue};
use gamakay_core::transport::Session;
use gamakay_hidraw::HidrawDevice;

use crate::{Cli, select_device};

#[derive(Subcommand)]
pub enum KeymapCmd {
    /// Print the layer's slots.
    Get(GetArgs),
    /// Remap one key.
    Set(SetArgs),
    /// Factory-reset the keyboard (resets everything, not just the layer).
    Reset(ResetArgs),
}

#[derive(Args)]
pub struct GetArgs {
    /// Profile to read; defaults to the active one.
    #[arg(long)]
    pub profile: Option<u8>,
    /// Read the Fn layer.
    #[arg(long = "fn")]
    pub fn_layer: bool,
    /// Print every slot, including empty ones.
    #[arg(long)]
    pub all: bool,
}

#[derive(Args)]
pub struct SetArgs {
    /// Profile to write; defaults to the active one.
    #[arg(long)]
    pub profile: Option<u8>,
    /// Write the Fn layer.
    #[arg(long = "fn")]
    pub fn_layer: bool,
    /// Slot index to change.
    #[arg(long)]
    pub slot: Option<u8>,
    /// Key whose slot to change, by name (e.g. CapsLock).
    #[arg(long)]
    pub key: Option<String>,
    /// The key to bind: a name or `0xNN` usage, or `none` to clear.
    #[arg(long)]
    pub to: String,
}

#[derive(Args)]
pub struct ResetArgs {
    /// Confirm the factory reset.
    #[arg(long)]
    pub yes: bool,
}

pub fn run(cli: &Cli, action: &KeymapCmd) -> Result<()> {
    match action {
        KeymapCmd::Get(args) => get(cli, args),
        KeymapCmd::Set(args) => set(cli, args),
        KeymapCmd::Reset(args) => reset(cli, args),
    }
}

fn active_profile(s: &mut Session<'_>, requested: Option<u8>) -> u8 {
    requested.unwrap_or_else(|| s.profile().unwrap_or(0))
}

fn get(cli: &Cli, args: &GetArgs) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    let profile = active_profile(&mut s, args.profile);
    let layer = s
        .read_layer(profile, args.fn_layer)
        .context("reading keymap")?;
    let which = if args.fn_layer { "Fn" } else { "base" };
    println!("profile {profile}, {which} layer:");
    let mut shown = 0;
    for (i, slot) in layer.slots.iter().enumerate() {
        let desc = slot.describe();
        // Skip empty slots, and entries with no name (the Fn layer's
        // `Special(8)`), unless the caller asked for everything.
        if (*slot == Slot::EMPTY || desc.is_empty()) && !args.all {
            continue;
        }
        println!("  slot {i:>3}  {desc}");
        shown += 1;
    }
    if shown == 0 {
        println!("  (empty)");
    }
    Ok(())
}

fn set(cli: &Cli, args: &SetArgs) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    let profile = active_profile(&mut s, args.profile);
    let mut layer = s.read_layer(profile, args.fn_layer)?;

    let slot = match (args.slot, &args.key) {
        (Some(n), _) => n,
        (None, Some(name)) => layer.find_by_name(name).ok_or_else(|| {
            anyhow::anyhow!("no key named {name:?} in the current layer; use --slot")
        })? as u8,
        (None, None) => bail!("give --slot or --key"),
    };
    let value = parse_bind(&args.to)?;
    let new = Slot::from_value(value);
    layer.set(slot as usize, new);

    // The LK67 ignores the single-slot write (0x13), so upload the whole layer.
    s.write_layer(profile, &layer, args.fn_layer)?;
    if cli.dry_run {
        for pkt in s.withheld() {
            let hex: Vec<String> = pkt.iter().take(16).map(|b| format!("{b:02x}")).collect();
            println!("dry run: {}", hex.join(" "));
        }
    } else {
        println!("slot {slot} -> {}", new.describe());
    }
    Ok(())
}

fn reset(cli: &Cli, args: &ResetArgs) -> Result<()> {
    if !args.yes {
        bail!("this resets the whole keyboard to defaults; re-run with --yes to confirm");
    }
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    s.factory_reset()?;
    if cli.dry_run {
        for pkt in s.withheld() {
            let hex: Vec<String> = pkt.iter().take(8).map(|b| format!("{b:02x}")).collect();
            println!("dry run: {}", hex.join(" "));
        }
    } else {
        println!("factory reset sent; the keyboard needs a few seconds to come back");
    }
    Ok(())
}

/// Parse a `--to` binding.
pub(crate) fn parse_bind(s: &str) -> Result<SlotValue> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("none") || t == "-" || t.eq_ignore_ascii_case("dead") {
        return Ok(SlotValue::Empty);
    }
    if let Some(rest) = t.strip_prefix("consumer:") {
        let c = u16::from_str_radix(rest.trim_start_matches("0x"), 16)
            .with_context(|| format!("bad consumer usage {rest:?}"))?;
        return Ok(SlotValue::Consumer(c));
    }
    if let Some(rest) = t.strip_prefix("macro:") {
        let (idx, mode) = match rest.split_once(':') {
            Some((i, m)) => (i.trim(), parse_macro_mode(m.trim())?),
            None => (rest.trim(), 0u8),
        };
        let index: u8 = idx.parse().context("macro index (0..49)")?;
        return Ok(SlotValue::Macro { mode, index });
    }
    match keycode::usage(t) {
        Some(u) => Ok(SlotValue::Hid(u)),
        None => bail!("unknown key {t:?} (a name like A, Esc, F5, or 0xNN)"),
    }
}

fn parse_macro_mode(s: &str) -> Result<u8> {
    match s.to_ascii_lowercase().as_str() {
        "repeat" => Ok(0),
        "toggle" => Ok(1),
        "hold" => Ok(2),
        other => other.parse().context("macro mode (repeat, toggle, hold)"),
    }
}
