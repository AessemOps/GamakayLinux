// SPDX-License-Identifier: GPL-3.0-or-later
//! `lk67ctl`: configure the GamaKay LK67 from Linux over hidraw.
//!
//! Read commands (`list`, `info`) and the Phase 2 lighting commands
//! (`light`, `apply`). Every write goes through the core safety guard.

mod keymap;
mod light;
mod macros;
mod profile;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use lk67_core::device;
use lk67_core::guard::Guard;
use lk67_core::transport::Session;
use lk67_hidraw::{HidrawDevice, HidrawInfo};

#[derive(Parser)]
#[command(
    name = "lk67ctl",
    version,
    about = "Configure a GamaKay LK67 from Linux over its USB cable.",
    long_about = None,
)]
struct Cli {
    /// Do not send anything; print the packets that would be sent.
    #[arg(long, global = true)]
    dry_run: bool,

    /// Use a specific hidraw node instead of auto-detecting one.
    #[arg(long, global = true, value_name = "PATH")]
    device: Option<PathBuf>,

    /// Allow writes on a board whose reported id is not in the table.
    #[arg(long, global = true)]
    force: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List attached GamaKay (and other HID) hidraw nodes.
    List,
    /// Identify the attached board and read its current settings.
    Info,
    /// Read or set the backlight and edge light.
    Light {
        #[command(subcommand)]
        action: Option<light::LightCmd>,
    },
    /// Apply a TOML lighting profile (name or path).
    Apply {
        /// Profile name (looked up in ~/.config/lk67ctl/profiles) or a path.
        profile: String,
    },
    /// Read, remap and reset the onboard key layers.
    Keymap {
        #[command(subcommand)]
        action: keymap::KeymapCmd,
    },
    /// List, write and clear onboard macros.
    Macro {
        #[command(subcommand)]
        action: macros::MacroCmd,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::List => cmd_list(),
        Command::Info => cmd_info(&cli),
        Command::Light { action } => light::run(&cli, action.as_ref()),
        Command::Apply { profile } => profile::run(&cli, profile),
        Command::Keymap { action } => keymap::run(&cli, action),
        Command::Macro { action } => macros::run(&cli, action),
    };
    if let Err(e) = result {
        let text = format!("{e:#}");
        eprintln!("lk67ctl: {text}");
        if text.contains("Permission denied") {
            eprintln!();
            eprintln!("hint: /dev/hidraw* is root-only by default. Install the udev rule once:");
            eprintln!("  sudo install -Dm644 packaging/udev/70-lk67ctl.rules /etc/udev/rules.d/");
            eprintln!(
                "  sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=hidraw"
            );
            eprintln!("then replug the keyboard. See docs/re-workflow.md.");
        } else if text.contains("Protocol error") || text.contains("Connection timed out") {
            eprintln!();
            eprintln!("hint: the keyboard's settings endpoint has stalled. It does not recover");
            eprintln!("on reopen: unplug and replug the keyboard (typing is unaffected).");
            eprintln!("See \"Writes (SET) — the flow matters\" in docs/protocol.md.");
        }
        std::process::exit(1);
    }
}

fn cmd_list() -> Result<()> {
    let nodes = lk67_hidraw::enumerate();
    if nodes.is_empty() {
        println!("no hidraw devices found (is the keyboard plugged in?)");
        return Ok(());
    }
    println!(
        "{:<16} {:<10} {:<14} {:>4}  {:<9}  NAME",
        "NODE", "USB ID", "USAGE", "FEAT", "LINK"
    );
    for n in &nodes {
        let supported = if n.has_settings_report() { "*" } else { " " };
        println!(
            "{:<16} {:<10} {:#06x}:{:#06x} {:>4}  {:<9}  {}{}",
            n.path.display().to_string(),
            format!("{:04x}:{:04x}", n.vendor_id, n.product_id),
            n.usage_page,
            n.usage,
            n.feature_bytes,
            n.connection.label(),
            supported,
            n.name,
        );
    }
    println!("\n* carries the 64-byte settings feature report used by `lk67ctl`.");
    Ok(())
}

fn cmd_info(cli: &Cli) -> Result<()> {
    let info = select_device(cli)?;
    let guard = Guard::new(cli.dry_run);
    let mut dev = HidrawDevice::open(&info.path)
        .with_context(|| format!("opening {}", info.path.display()))?;
    let mut session = Session::new(&mut dev, guard);

    let id = session.device_id().context("identify (0x8F) failed")?;
    let record = device::by_id(id);
    if let Some(r) = record {
        session.set_family(r.family);
    }
    let variant = device::is_lk67_variant(id);

    println!("node:        {}", info.path.display());
    println!("name:        {}", info.name);
    println!(
        "usb id:      {:04x}:{:04x}",
        info.vendor_id, info.product_id
    );
    println!("link:        {}", info.connection.label());
    println!(
        "usage:       page {:#06x} usage {:#06x}",
        info.usage_page, info.usage
    );
    println!("device id:   {id}");
    match record {
        Some(r) => {
            println!(
                "model:       {} ({}) [{} family]",
                r.display_name,
                r.name,
                r.family.name()
            );
            let (set_profile, get_profile) = match r.family {
                device::Family::Yc500 => (lk67_core::cmd::SET_PROFILE, lk67_core::cmd::GET_PROFILE),
                device::Family::Gen2 => (
                    lk67_core::cmd::SET_PROFILE_GEN2,
                    lk67_core::cmd::GET_PROFILE_GEN2,
                ),
            };
            println!(
                "layers:      {} keymaps; the active one is selected with {set_profile:#04x}/{get_profile:#04x}",
                r.layer
            );
            if r.experimental {
                println!(
                    "support:     experimental, read-only ({} command set); every write is refused",
                    r.family.name()
                );
            }
        }
        None => println!("model:       unknown (no table record for id {id})"),
    }
    if variant {
        println!(
            "WARNING:     id {id} is the documented mismatched firmware for the LK67; \
             writes are refused without --force"
        );
    }

    if let Ok(rev) = session.revision()
        && let Some(s) = format_revision(rev)
    {
        println!("firmware:    {s}");
    }
    if let Ok(profile) = session.profile() {
        println!("profile:     {profile}");
    }
    if let Ok(led) = session.led_param() {
        println!(
            "lighting:    mode {} speed {} brightness {} rgb #{:02x}{:02x}{:02x} flags {}",
            led.mode, led.speed, led.brightness, led.rgb[0], led.rgb[1], led.rgb[2], led.flags
        );
    }
    // gen2 answers 0x89 with decoded fields whose meanings are not established,
    // so there is nothing honest to print for it yet.
    if record.map(|r| r.family) != Some(device::Family::Gen2)
        && let Ok(o) = session.options()
    {
        println!(
            "options:     win_lock={} mac_mode={} wasd_swap={} backlight_off={} \
             side_light_off={} keyboard_mode={} keyboard_lock={}",
            o.win_lock,
            o.mac_mode,
            o.wasd_swap,
            o.backlight_off,
            o.side_light_off,
            o.keyboard_mode,
            o.keyboard_lock
        );
    }
    if let Ok(d) = session.debounce() {
        println!("debounce:    {d}");
    }
    if let Ok(s) = session.sleep_timers() {
        println!(
            "sleep:       bt={}s 2.4g={}s deep_bt={}s deep_2.4g={}s",
            s.sleep_bt, s.sleep_24g, s.deep_bt, s.deep_24g
        );
    }
    if let Ok(auto) = session.auto_os() {
        println!("auto-os:     {auto}");
    }

    if guard.dry_run {
        println!("(dry run: reads only, nothing was written)");
    }
    Ok(())
}

/// The node to talk to: `--device`, else the auto-detected settings node.
/// Refuse a write to a board that is not the one this build targets.
///
/// The USB ids only pick a *candidate*: the LK67's ids are shared with a sibling
/// board, and a board with mismatched firmware answers the handshake with a
/// different id, so the decision is made on what the board says it is rather
/// than on what it enumerates as. `--force` overrides, as documented.
/// Identify the attached board and key the session to its command set. The
/// identify opcode is shared, so this happens once, before any family-specific
/// command. Unknown ids stay on yc500; the callers refuse them separately.
pub(crate) fn attach_family(s: &mut Session<'_>) -> Result<device::Family> {
    let id = s.device_id().context("identifying the board")?;
    let family = device::by_id(id)
        .map(|r| r.family)
        .unwrap_or(device::Family::Yc500);
    s.set_family(family);
    Ok(family)
}

pub(crate) fn check_writable(cli: &Cli, s: &mut Session<'_>) -> Result<()> {
    let id = s.device_id().context("identifying the board")?;
    let Some(rec) = device::by_id(id) else {
        if cli.force {
            return Ok(());
        }
        bail!(
            "the attached board reports unknown id {id}; refusing to write \
             (pass --force to override)"
        );
    };
    s.set_family(rec.family);
    // Experimental support is read-only, and `--force` does not change that:
    // the family's write opcodes mean different registers than the ones this
    // project has verified, so a "forced" write is a wrong-register write.
    if rec.experimental {
        bail!(
            "the attached board reports id {id} ({}): support for its {} command set is \
             experimental and read-only, so nothing was written. See docs/protocol.md, \
             \"The gen2 family\"",
            rec.display_name,
            rec.family.name()
        );
    }
    if rec.id == device::LK67.id || cli.force {
        return Ok(());
    }
    bail!(
        "the attached board reports id {id} ({}), not the LK67; refusing to write \
         (pass --force to override)",
        rec.display_name
    )
}

pub(crate) fn select_device(cli: &Cli) -> Result<HidrawInfo> {
    let found = match &cli.device {
        Some(path) => lk67_hidraw::find(path)
            .with_context(|| format!("no hidraw node at {}", path.display()))?,
        None => lk67_hidraw::find_supported().ok_or_else(|| anyhow::anyhow!("no LK67 found"))?,
    };
    // "Carries a 64-byte feature report" alone picks the wrong device: on this
    // machine the Corsair mouse has one too, so a node whose ids (or Bluetooth
    // connection) are not ours must be refused unless the user insists.
    if !found.is_lk67ctl() && !cli.force {
        bail!(
            "the node {} ({:04x}:{:04x}, {}) is not a recognised LK67; pass --force to try it",
            found.path.display(),
            found.vendor_id,
            found.product_id,
            found.connection.label()
        );
    }
    if found.usage != 0 || found.feature_bytes > 0 {
        return Ok(found);
    }
    bail!("no usable hidraw node found");
}

/// The vendor renders the revision as hex nibbles, each a decimal digit, so
/// `0x102` prints as `102`.
fn format_revision(rev: u16) -> Option<String> {
    if rev == 0 {
        None
    } else {
        Some(format!("{rev:X}"))
    }
}
