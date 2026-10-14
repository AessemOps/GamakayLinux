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

#[derive(Deserialize, Debug, Default)]
struct Profile {
    backlight: Option<Section>,
    edge: Option<Section>,
    /// Base-layer remaps: key is a slot number or a key name, value is the
    /// binding to write (a key name, `0xNN`, or `none`).
    keymap: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
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
        let profile_idx = s.profile().unwrap_or(0);
        let mut layer = s.read_layer(profile_idx, false)?;
        for (key, to) in map {
            let slot = match key.trim().parse::<usize>() {
                Ok(n) => n,
                Err(_) => layer
                    .find_by_name(key)
                    .ok_or_else(|| anyhow::anyhow!("no key named {key:?} in the base layer"))?,
            };
            let value = crate::keymap::parse_bind(to)?;
            layer.set(slot, gamakay_core::keymap::Slot::from_value(value));
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

fn resolve_path(arg: &str) -> Result<PathBuf> {
    let direct = Path::new(arg);
    if direct.is_file() {
        return Ok(direct.to_path_buf());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| anyhow::anyhow!("cannot find a config directory (no HOME)"))?;
    let candidate = base.join("gamakay/profiles").join(format!("{arg}.toml"));
    if candidate.is_file() {
        return Ok(candidate);
    }
    bail!(
        "no profile at {arg:?} (looked for {} and {})",
        direct.display(),
        candidate.display()
    )
}
