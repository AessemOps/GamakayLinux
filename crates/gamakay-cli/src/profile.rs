// SPDX-License-Identifier: GPL-3.0-or-later
//! `gamakay apply`: apply a TOML lighting profile.
//!
//! Profiles live in `$XDG_CONFIG_HOME/gamakay/profiles/<name>.toml` (or
//! `~/.config/gamakay/profiles/`). Example:
//!
//! ```toml
//! [backlight]
//! mode = "wave"
//! color = "#9507e1"
//! brightness = 3
//! speed = 2
//! direction = 1
//! rainbow = false
//!
//! [edge]
//! mode = "static"
//! color = "#00ff00"
//! brightness = 2
//! speed = 1
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use gamakay_core::guard::Guard;
use gamakay_core::light::LK67_FLAGS;
use gamakay_core::transport::Session;
use gamakay_hidraw::HidrawDevice;

use crate::light::{self, LightArgs};
use crate::{Cli, select_device};

// A typo in a profile would otherwise be silently ignored, leaving that setting
// at whatever the board already had while the run reports success.
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
struct Profile {
    backlight: Option<Section>,
    edge: Option<Section>,
    /// Base-layer remaps: key is a slot number or a key name, value is the
    /// binding to write (a key name, `0xNN`, or `none`).
    keymap: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
struct Section {
    mode: Option<String>,
    color: Option<String>,
    brightness: Option<u8>,
    speed: Option<u8>,
    direction: Option<u8>,
    rainbow: Option<bool>,
}

impl Section {
    fn to_args(&self) -> Result<LightArgs> {
        let color = match &self.color {
            Some(c) => Some(light::parse_color(c).map_err(|e| anyhow::anyhow!(e))?),
            None => None,
        };
        Ok(LightArgs {
            mode: self.mode.clone(),
            color,
            brightness: self.brightness,
            speed: self.speed,
            direction: self.direction,
            rainbow: self.rainbow == Some(true),
            fixed: self.rainbow == Some(false),
        })
    }
}

pub fn run(cli: &Cli, profile_arg: &str) -> Result<()> {
    let path = resolve_path(profile_arg)?;
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading profile {}", path.display()))?;
    let profile: Profile =
        toml::from_str(&text).with_context(|| format!("parsing profile {}", path.display()))?;
    if profile.backlight.is_none() && profile.edge.is_none() && profile.keymap.is_none() {
        bail!("profile has no [backlight], [edge] or [keymap] section");
    }
    println!("applying {}", path.display());

    let info = select_device(cli)?;
    let mut dev = HidrawDevice::open(&info.path)?;
    let mut s = Session::new(&mut dev, Guard::new(cli.dry_run));
    crate::check_writable(cli, &mut s)?;

    if let Some(sec) = &profile.backlight {
        let args = sec.to_args()?;
        let cur = s.led_param()?;
        let p = light::merge(&cur, &args, false)?;
        s.set_backlight(&p, LK67_FLAGS)?;
        if cli.dry_run {
            for pkt in s.withheld() {
                light::print_plan(pkt);
            }
        } else {
            light::print_param("backlight", &p, false);
        }
    }

    if let Some(sec) = &profile.edge {
        // Only a board that has an edge light may be sent 0x08; `light edge
        // set` refuses the same way, and `apply` must not be the hole in it.
        if !light::board_has_edge(&mut s) && !cli.force {
            anyhow::bail!(
                "the profile has an [edge] section but this board has no edge light \
                 (pass --force to try anyway)"
            );
        }
        let args = sec.to_args()?;
        let cur = s.edge_param()?;
        let p = light::merge(&cur, &args, true)?;
        s.set_edge(&p, LK67_FLAGS)?;
        if cli.dry_run {
            for pkt in s.withheld() {
                light::print_plan(pkt);
            }
        } else {
            light::print_param("edge", &p, true);
        }
    }
    if let Some(map) = &profile.keymap {
        // Reading (and then writing) profile 0 because the active profile could
        // not be read would edit a layer the user never chose.
        let profile_idx = s.profile().context("reading the active profile")?;
        let mut layer = s.read_layer(profile_idx, false)?;
        for (key, to) in map {
            let slot = match key.trim().parse::<usize>() {
                Ok(n) => n,
                Err(_) => layer
                    .find_by_name(key)
                    .ok_or_else(|| anyhow::anyhow!("no key named {key:?} in the base layer"))?,
            };
            let value = crate::keymap::parse_bind(to)?;
            if slot >= gamakay_core::keymap::TX_SLOTS {
                anyhow::bail!(
                    "keymap slot {slot} cannot be written: a bulk write reaches slots \
                     0..{} (slots {}..{} are read-only on this firmware)",
                    gamakay_core::keymap::TX_SLOTS - 1,
                    gamakay_core::keymap::TX_SLOTS,
                    gamakay_core::keymap::SLOTS - 1
                );
            }
            assert!(layer.set(slot, gamakay_core::keymap::Slot::from_value(value)));
            println!("  keymap slot {slot} -> {to}");
        }
        s.write_layer(profile_idx, &layer, false)?;
        if cli.dry_run {
            for pkt in s.withheld() {
                light::print_plan(pkt);
            }
        }
    }
    Ok(())
}

/// Resolve a profile argument.
///
/// A *name* is looked up only inside `~/.config/gamakay/profiles`, and the
/// looked-up path is checked to still be under it, so a name containing `..`
/// cannot walk out of the directory. A path (`./x.toml`, `/tmp/x.toml`) is
/// taken as given: that is an explicit choice, and the file can only drive the
/// documented settings through the guard.
fn resolve_path(arg: &str) -> Result<PathBuf> {
    let direct = Path::new(arg);
    let looks_like_a_path = arg.contains('/') || arg.starts_with('~') || arg.starts_with('.');
    if looks_like_a_path || direct.extension().is_some() {
        if direct.is_file() {
            return Ok(direct.to_path_buf());
        }
        bail!("no profile file at {arg:?}");
    }

    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| anyhow::anyhow!("cannot find a config directory (no HOME)"))?;
    let dir = base.join("gamakay/profiles");
    let candidate = dir.join(format!("{arg}.toml"));
    // Canonicalise both: a symlink inside the directory must not lead out of it.
    let under_dir = dir
        .canonicalize()
        .ok()
        .zip(candidate.canonicalize().ok())
        .is_some_and(|(d, c)| c.starts_with(&d));
    if candidate.is_file() && under_dir {
        return Ok(candidate);
    }
    bail!("no profile named {arg:?} in {}", dir.display())
}
