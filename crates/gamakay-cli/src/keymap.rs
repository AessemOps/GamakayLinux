// SPDX-License-Identifier: GPL-3.0-or-later
//! `gamakay keymap`: read, remap and reset the onboard layers.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use gamakay_core::guard::Guard;
use gamakay_core::keycode;
use gamakay_core::keymap::{Slot, SlotValue};
use gamakay_core::transport::Session;
use gamakay_hidraw::HidrawDevice;

use crate::{Cli, check_writable, select_device};

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

/// Run `f` against the requested profile, activating it first and restoring the
/// previously active one afterwards.
///
/// The keymap commands always address the **active** profile — the firmware
/// ignores the profile byte in their payload — so `--profile N` has to select
/// the profile, and it must not leave the board on a different layer than the
/// user was typing on.
fn with_profile<T>(
    s: &mut Session<'_>,
    requested: Option<u8>,
    f: impl FnOnce(&mut Session<'_>, u8) -> Result<T>,
) -> Result<T> {
    let previous = s.profile().context("reading the active profile")?;
    let Some(want) = requested else {
        return f(s, previous);
    };
    // Identify rather than assume a count: guessing "one profile" turned a
    // transient reply failure into "this board has profiles 0..0".
    let id = s.device_id().context("identifying the board")?;
    let layers = match gamakay_core::device::by_id(id) {
        Some(rec) => rec.layer,
        None => bail!(
            "the attached board reports unknown id {id}, so its profile count is not known; \
             refusing to guess a profile index"
        ),
    };
    if want >= layers {
        bail!(
            "profile {want} does not exist: this board has profiles 0..{}",
            layers - 1
        );
    }
    if want == previous {
        return f(s, previous);
    }
    if s.is_dry_run() {
        // The select is a write, so dry-run holds it back and the board stays
        // on `previous`: reading it as `want` would show (and, for a set,
        // write) a layer the command never selected.
        eprintln!("dry run: profile {want} would be selected; reading profile {previous} instead");
        return f(s, previous);
    }
    s.set_profile(want).context("selecting the profile")?;
    let out = f(s, want);
    if let Err(e) = s.set_profile(previous) {
        eprintln!("warning: could not restore profile {previous}: {e}");
    }
    out
}

fn get(cli: &Cli, args: &GetArgs) -> Result<()> {
    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    let (profile, layer) = with_profile(&mut s, args.profile, |s, profile| {
        let layer = s
            .read_layer(profile, args.fn_layer)
            .with_context(|| format!("reading keymap of profile {profile}"))?;
        Ok((profile, layer))
    })?;
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
    check_writable(cli, &mut s)?;

    let value = parse_bind(&args.to)?;
    let new = Slot::from_value(value);

    // The read-modify-write has to happen while the requested profile is the
    // active one: the write addresses the active profile, not the one in its
    // payload.
    let (profile, slot) = with_profile(&mut s, args.profile, |s, profile| {
        let mut layer = s.read_layer(profile, args.fn_layer)?;
        let slot = match (args.slot, &args.key) {
            (Some(n), _) => n,
            (None, Some(name)) => layer
                .find_by_name(name)
                .ok_or_else(|| anyhow::anyhow!("no key named {name:?} in this layer; use --slot"))?
                as u8,
            (None, None) => bail!("give --slot or --key"),
        };
        // Slot 126/127 are past what a bulk write transmits, and anything at or
        // past SLOTS is not a slot at all: say so rather than report a write
        // that cannot land (or abort on an out-of-bounds index).
        let index = slot as usize;
        if index >= gamakay_core::keymap::TX_SLOTS {
            bail!(
                "slot {slot} cannot be written: a bulk write reaches slots 0..{}; \
                 slots {}..{} are read-only on this firmware",
                gamakay_core::keymap::TX_SLOTS - 1,
                gamakay_core::keymap::TX_SLOTS,
                gamakay_core::keymap::SLOTS - 1
            );
        }
        assert!(layer.set(index, new));
        // The LK67 ignores the single-slot write (0x13), so upload the layer.
        s.write_layer(profile, &layer, args.fn_layer)?;
        Ok((profile, slot))
    })?;

    if cli.dry_run {
        for pkt in s.withheld() {
            let hex: Vec<String> = pkt.iter().take(16).map(|b| format!("{b:02x}")).collect();
            println!("dry run: {}", hex.join(" "));
        }
    } else {
        println!("slot {slot} in profile {profile} -> {}", new.describe());
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
    check_writable(cli, &mut s)?;
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
        let index: u8 = idx.parse().context("macro index")?;
        if usize::from(index) >= usize::from(gamakay_core::macros::MACRO_SLOTS) {
            bail!(
                "macro index {index} does not exist: the board has {} slots (0..{})",
                gamakay_core::macros::MACRO_SLOTS,
                gamakay_core::macros::MACRO_SLOTS - 1
            );
        }
        return Ok(SlotValue::Macro { mode, index });
    }
    match keycode::usage(t) {
        Some(u) => Ok(SlotValue::Hid(u)),
        None => bail!("unknown key {t:?} (a name like A, Esc, F5, or 0xNN)"),
    }
}

/// 0 repeat, 1 toggle, 2 hold. Anything else is a byte the firmware does not
/// define, so it is refused rather than written.
fn parse_macro_mode(s: &str) -> Result<u8> {
    match s.to_ascii_lowercase().as_str() {
        "repeat" => Ok(0),
        "toggle" => Ok(1),
        "hold" => Ok(2),
        other => match other.parse::<u8>() {
            Ok(n) if n <= 2 => Ok(n),
            _ => bail!("macro mode must be repeat, toggle or hold (0..=2), got {s:?}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `macro:<index>` is bounded by the board's slot count. This assert exists
    /// because the bound was once dropped in an edit and nothing caught it.
    #[test]
    fn macro_index_is_bounded_by_the_slot_count() {
        assert!(parse_bind("macro:0").is_ok());
        assert!(parse_bind("macro:49").is_ok());
        let err = parse_bind("macro:50").unwrap_err().to_string();
        assert!(err.contains("does not exist"), "{err}");
        assert!(parse_bind("macro:200").is_err());
    }

    /// The syntax is `macro:<index>[:<mode>]`, and the mode is one of three.
    #[test]
    fn macro_bindings_take_an_index_and_an_optional_mode() {
        assert!(parse_bind("macro:5").is_ok(), "the mode defaults to repeat");
        assert!(parse_bind("macro:5:toggle").is_ok());
        assert!(parse_bind("macro:5:hold").is_ok());
        assert!(parse_bind("macro:5:9").is_err(), "mode is 0..=2");
        assert!(parse_bind("macro:5:nonsense").is_err());
    }

    #[test]
    fn bindings_and_keys_parse() {
        assert_eq!(parse_bind("Esc").unwrap(), SlotValue::Hid(0x29));
        assert_eq!(parse_bind("none").unwrap(), SlotValue::Empty);
        assert!(parse_bind("not_a_key").is_err());
    }
}
