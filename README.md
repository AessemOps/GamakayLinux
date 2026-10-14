# GamakayLinux

A Linux-native replacement for the GamaKay keyboard driver app, in Rust, for
the **GamaKay LK67**. It configures the keyboard over USB, the 2.4 GHz dongle
and Bluetooth with no resident process: every setting is written to the
keyboard's own memory and the program exits.

> Status: **Phase 2** — `list` / `info` (read-only) and the lighting commands
> (`light`, `apply`) are implemented and verified on hardware. Keymap and
> macros land in Phases 3–4, the iced GUI in Phase 5. See
> `.kilo/plans/1792003709046-gamakay-linux-rust-app.md`.

## Why

The vendor app is a Windows-only Electron bundle. This project speaks the
board's HID protocol directly from Linux:

- **hidraw, not libusb.** The kernel driver stays bound, so the keyboard keeps
  typing while you configure it.
- **No resident process by default.** Settings go to onboard memory
  (`support_onboard: 2`); nothing stays running.
- **Minimal footprint.** The CLI is one-shot; the GUI only runs while open and
  idle at ~0% CPU.
- **Safe.** A command allow-list and a destructive/boot block-list sit in front
  of every write, and there is no firmware-flashing path.

## Layout

| Path | What it is |
|---|---|
| `crates/gamakay-core/` | Protocol encode/decode, device table, safety guard, `Transport` trait and the fixture `MockTransport` |
| `crates/gamakay-hidraw/` | Linux hidraw transport: sysfs enumeration and the `HIDIOCSFEATURE`/`HIDIOCGFEATURE` ioctls |
| `crates/gamakay-cli/` | The `gamakay` binary |
| `docs/protocol.md` | Command reference, byte layouts, transport matrix (the allow-list source of truth) |
| `docs/re-workflow.md` | How the protocol was captured and how to extend it |
| `tests/fixtures/` | Decoded packet fixtures (hex) |
| `packaging/` | udev rule, Arch/CachyOS `PKGBUILD`, desktop entry |

## Build and test

```bash
cargo build --release          # gamakay, statically usable, stripped + LTO
cargo test                     # unit tests + fixture replay
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

## Use

`gamakay list` shows every hidraw node and marks the one carrying the 64-byte
settings report. `gamakay info` identifies the board and reads its current
settings. `gamakay light` reads and sets the backlight, and `gamakay apply`
applies a TOML profile.

```bash
gamakay list
gamakay info
gamakay --device /dev/hidraw0 info

gamakay light                                        # show backlight (and edge)
gamakay light set --mode wave --color ff0000 --brightness 3 --speed 2 --direction 1
gamakay light off                                    # options bit, not a black write
gamakay light on
gamakay apply ~/.config/gamakay/profiles/olivia.toml
```

A profile is TOML; unspecified fields keep their current value:

```toml
[backlight]
mode = "wave"        # static, breathing, spectrum, wave, ripple, ...
color = "#9507e1"
brightness = 3       # 0..5 on the LK67
speed = 2            # 0 slowest .. 4 fastest
direction = 1        # for modes that use one
rainbow = false      # false = fixed colour

[edge]
mode = "static"      # only on boards that have an edge light
color = "#00ff00"
```

`--dry-run` prints the packet a command would send and sends nothing.

### Writing

A write is **fire-and-forget**: the board applies it and commits it to onboard
memory, and `gamakay` pauses before the next command so the settings endpoint
stays healthy. Verify a write with a following `gamakay light`. If a write is
interrupted and the endpoint stalls, unplug and replug the keyboard (typing is
never affected). See `docs/protocol.md`.

### Permissions

`/dev/hidraw*` is root-only by default. Install the udev rule once and replug:

```bash
sudo install -Dm644 packaging/udev/70-gamakay.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=hidraw
```

The rule grants the active local user access with `TAG+="uaccess"` — no group
membership needed.

## Transport support

Cable is the only verified transport so far. Wireless support is limited to
what the firmware accepts, verified per transport in Phase 0; features a
transport does not carry are disabled rather than guessed. See the matrix in
`docs/protocol.md`.

## License

GPL-3.0-or-later. See `LICENSE` and `CREDITS.md`.
