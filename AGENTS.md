# AGENTS.md

Instructions for AI agents (and humans in a hurry) working on GamakayLinux.
This is the stable operating manual: commands, workflow, conventions.
For **current state** — what is implemented, what is open — read
[`README.md`](README.md) and [`docs/protocol.md`](docs/protocol.md). The
implementation plan is `.kilo/plans/1792003709046-gamakay-linux-rust-app.md`.

## What this is

A Linux-native Rust replacement for the GamaKay keyboard driver app, for the
**GamaKay LK67**. It configures the board over USB, the 2.4 GHz dongle and
Bluetooth by writing settings to the keyboard's own memory, then exits — no
resident process. It speaks the ROYUAN `yc500` protocol over Linux **hidraw**
directly (no libusb, no libudev), so the kernel driver stays bound and the
keyboard keeps typing while it is configured.

**There is no firmware-flashing path**, and there never will be. A safety
guard refuses a block-list of destructive and bootloader opcodes by name.

## Layout

| Path | What it is |
|---|---|
| `crates/gamakay-core/` | Protocol packets/checksums, opcode table, device table, safety guard, `Transport` trait, `Session`, decode/encode, `MockTransport` |
| `crates/gamakay-hidraw/` | Linux hidraw transport: sysfs enumeration, report-descriptor parse, `HIDIOCSFEATURE`/`HIDIOCGFEATURE` ioctls, plus the `raw` example |
| `crates/gamakay-cli/` | The `gamakay` binary (`clap`) |
| `crates/gamakay-gui/` | The iced app (**Phase 5**, not yet present) |
| `docs/protocol.md` | Command reference, byte layouts, transport matrix — the **allow-list source of truth** |
| `docs/re-workflow.md` | How the protocol was captured and how to extend it |
| `tests/fixtures/` | Decoded packet fixtures (hex captured from hardware) |
| `packaging/` | udev rule, `gamakay.desktop`, Arch/CachyOS `PKGBUILD` |
| `.kilo/plans/` | Implementation plans (committed) |

## Commands

There is no `make test`; run the gates directly.

```bash
cargo build --release            # gamakay: LTO, stripped, panic=abort
cargo test                       # unit tests + hardware fixture replay

cargo fmt --all --check          # formatting gate
cargo clippy --all-targets -- -D warnings   # lint gate (warnings are errors)

make udev-install                # install the hidraw rule (needs root)
make check                       # fmt + clippy + test
```

Reading one raw reply during protocol work (open the node first, see below):

```bash
cargo run -p gamakay-hidraw --example raw -- /dev/hidraw0 0x92
```

On-device checks need `/dev/hidraw*` access; `gamakay list` only reads sysfs
and needs none:

```bash
gamakay list                     # enumerate nodes, marks the settings node
gamakay info                     # identify + read current settings
gamakay --device /dev/hidraw0 info
gamakay light                    # show backlight (and edge light)
gamakay light set --mode wave --color ff0000 --brightness 3 --speed 2
gamakay apply ~/.config/gamakay/profiles/<name>.toml
gamakay keymap get               # dump the base layer
gamakay keymap set --key CapsLock --to Esc
gamakay macro list               # onboard macros
gamakay macro set 0 --repeat 1 --steps "A+@20, A-@20"
gamakay keymap set --slot 20 --to macro:0   # bind macro 0 to a key
```

## Workflow

- **Always commit and push your work** — never leave uncommitted changes at
  the end of a session.
- **Never push to `main`** without explicit user approval. Work on a branch
  (`feat/…`, `fix/…`, `chore/…`, `docs/…`) and open a PR. Fetch first.
- Commit messages follow Conventional Commits (`feat(core): …`,
  `fix(hidraw): …`, `docs(protocol): …`, `chore: …`).
- Write a plan in `.kilo/plans/` for any non-trivial change and commit it.
- Only commit/push/create PRs when the work is complete and verified. Never
  force-push, skip hooks, or amend a failed commit — add a new commit.
- **Tests before hand-off**: `cargo fmt --all --check`, `cargo clippy
  --all-targets -- -D warnings`, `cargo test`, and a live drill for anything
  touching the wire (see below).

## Safety rules (non-negotiable)

1. **No flashing.** `gamakay-core::guard::BLOCKED` lists the destructive and
   bootloader opcodes (`0xAC`, `0x2C`, `0x7F`, `0x30`, `0x1C`, `0x1E`, `0xF8`,
   `0xBA`). Every packet leaves through `Guard::build`; never bypass it.
2. **A command may enter `ALLOWED` only after its write is capture- or
   read-back-confirmed** on hardware. No guessed packets.
3. **Read-modify-write** for bitfields (options) so unmodelled bits survive.
4. **Respect the write flow.** A feature SET is fire-and-forget: do **not**
   read a reply in the same exchange, and leave the commit pause
   (`WRITE_COMMIT_MS`, ~1.5 s) before the next command, or the settings
   endpoint stalls and needs USB re-enumeration (typing is unaffected).
   Per-key colour uploads also land in flash; space reports and never loop
   uploads. See "Writes (SET)" in `docs/protocol.md`.
5. Wire the reset/recovery procedure in `docs/protocol.md` before exercising
   any new write path on hardware.

## Conventions

- **Protocol rule**: `docs/protocol.md` is the allow-list source of truth. A
  change that adds or alters a command updates `docs/protocol.md`, the guard
  allow-list and a test **in the same change**. New wire facts get a marker:
  **[HW]** (a plugged-in board), **[REF]** (derived from the GPL `sharkfin`
  document), or **[FW]** (a firmware image).
- **Clean-room rule**: the vendor Electron bundle is extracted into the
  gitignored `vendor/` for study only and is **never** committed, bundled or
  linked. Raw captures (`*.pcapng`) are gitignored; only decoded hex fixtures
  are committed.
- **Credits rule**: `CREDITS.md` is always current. A change that ports or
  adapts third-party code adds a provenance header (SPDX + source) and a
  `CREDITS.md` entry; a change to a direct dependency updates `CREDITS.md` and
  bumps its `Last reviewed` line, **in the same change**.
- **Every source file** carries an `// SPDX-License-Identifier:
  GPL-3.0-or-later` header.
- **Docs rule**: `README.md`, `AGENTS.md`, `docs/protocol.md` and
  `docs/re-workflow.md` are kept up to date in the same change as the code.
- **No comments that narrate the obvious**; comment the wire facts and the
  *why*, not the *what*.

## Deploying / testing on the VM

- The board is configured on the **Linux host**, not in the VM. When the
  device is passed through with QEMU's `usb-host`, traffic still crosses the
  host's usbfs and `usbmon`/`tshark` sees the URBs.
- While the device is passed through to the VM, the host loses it: only run
  the Linux tool with the device detached from the VM.
- The official app runs in the Windows VM for cross-checks (values written by
  us must show up there, and the reverse).

## Environment facts

- Test board: GamaKay LK67. The 2026-10 firmware re-enumerated it as wired USB
  `3151:4003` (pre-update: `0461:4002`). The settings node is `hidraw0` (usage
  page `0x0001`, usage `0x0006`, 64-byte feature report); `hidraw1` has no
  feature report and is unused.
- Identity is the `0x8F` handshake id, which is firmware-dependent: the LK67
  reports **290** on the 2026-10 firmware and **149** before it (the
  mismatched-firmware variant reports **71**). Never identify by VID/PID:
  hundreds of boards share `1121:4002` / `3151:4002`.
- The 2.4 GHz dongle VID/PID and the Bluetooth HID descriptor are **open
  items**; do not assume a dongle relays every command. Bluetooth HOGP reaches
  us as hidraw through `uhid`.
- `/dev/hidraw*` is root-only by default; `make udev-install` grants the
  active user access via `TAG+="uaccess"` (the rule covers both USB ids).
