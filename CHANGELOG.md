# Changelog

All notable changes to this project. Version numbers are the tags; the workspace
`Cargo.toml` version is the source of truth and `scripts/check-release-version.sh`
keeps the PKGBUILD in step with it.

## Unreleased

### Added

- **A read-only dump for boards we do not support yet.**
  `scripts/dump-keyboard.sh` records a keyboard's HID identity, raw report
  descriptor and the reply to every documented GET opcode, which is what adding
  a board to `docs/protocol.md` starts from. It sends no SET opcode and redacts
  serial numbers and unique IDs.

### Fixed

- The rename also rewrote the vendor's own identifiers in the
  reverse-engineering notes and the device table (the download URL and the
  `company` string). They are restored.

## 0.3.1 — 2026-10-20

### Fixed

- **The GUI shows its own name again.** The window title and the sidebar
  heading still said "GamaKay" after the rename; they now read `lk67ctl`. The
  device-recognition messages in the GUI and the CLI say "not a recognised
  LK67", since that is the only board the tool drives.

## 0.3.0 — 2026-10-20

### Changed

- **Renamed to `lk67ctl`.** The tool supports exactly one board, the GamaKay
  LK67, and the old name put the vendor's trademark where a product name
  belongs. The binaries are `lk67ctl` and `lk67-gui`; the crates, package,
  desktop entry, AppStream id, icons and udev rule follow, and the repository
  moves to `AessemOps/lk67ctl`. The Arch, deb and rpm packages declare that they
  replace `gamakaylinux` rather than colliding with it, and named profiles are
  read from `~/.config/lk67ctl/profiles`, falling back to the old
  `~/.config/gamakay/profiles` when the new directory does not exist.

## 0.2.1 — 2026-10-20

### Added

- **The Underglow panel gains a Rainbow toggle.** The flags nibble is not
  cosmetic: with the rainbow bit set the board shows its own palette and ignores
  the colour, so it is now a visible control with the colour wheel instead of a
  hidden flag. Turning it off applies the chosen colour.

### Fixed

- **The underglow colour now applies in static mode.** The board ignores RGB
  while the rainbow bit is set, and both the panel and `light edge set --color`
  preserved that bit from the last read, so a colour write showed the firmware's
  rainbow gradient instead of the chosen colour. Writing a colour now clears the
  bit; `--rainbow` still sets it explicitly, and a write that touches neither
  keeps the stored flags.

## 0.2.0 — 2026-10-18

**The underglow works.** The LK67's side and bottom-mesh LEDs were treated as
absent, because that is what GamaKay's software says: it exposes no control for
them, and its record for this board reports no side light. The board disagrees —
the LEDs are there, the firmware drives them through `SET 0x08`, and it answers
`GET 0x88` with the stored parameter. The GUI gains an **Underglow** panel
(mode, including off, and colour), `lk67ctl light edge` works without `--force`
and answers to the name `underglow` too, and the record is corrected.

Measured rather than assumed, with the key backlight pinned to a fixed colour so
the two lights could be told apart: the underglow honours **mode and colour
only**. Brightness (4, 1, 0 are identical), speed (the wave animates at one
fixed rate) and the flags nibble (the "rainbow" flag stays a single steady
colour over 45 s) are stored without changing anything visible, so the app
offers none of them and a write preserves whatever the board holds.
`tests/fixtures/lk67-edge-light.txt` pins four written states read back from the
board; the vendor app never set any of them.

The panel's colour has a wheel of its own — smaller than the backlight's, and
starting from the colour the board holds — and a drag only sets the field:
**Apply** sends it, because the board commits every write to flash and a live
drag would queue writes inside the commit window.

Also fixed, from reviewing the branch: a connected board was announced twice, so
a tab that reads on demand ran its read twice — and the macro list is 200 feature
transfers, the burst the on-demand rule exists to keep out of the connect path; a
Reconnect kept the previous board's underglow, whose stored fields the next Apply
would have written to a different board; `light edge get` printed `spectrum`, the
backlight's name, for a mode `--mode neon` sets; and the underglow's off state is
accepted as `--mode 0`, as the protocol documents, not only as `off`.

## 0.1.1 — 2026-10-18

**Fixed: the app has its icon now.** Every package already installed the icons
(8 hicolor sizes, the same PNG the AppImage uses) — what was missing was the link
between the *running window* and the desktop entry. iced sets a window's
application id only when the application asks, so `WM_CLASS` was empty and a
desktop shell had nothing to match `lk67ctl.desktop` against: the taskbar showed
a generic icon and the window did not group under its launcher entry. The window
now announces `application_id = "lk67ctl"`, matching the desktop file's basename
as iced documents, which is `WM_CLASS` on X11 and the app id on Wayland.

Verified on the hardware: `WM_CLASS` went from `"", ""` to
`"lk67ctl", "lk67ctl"`, with the window geometry unchanged.

**Packaging and CI.** The Arch job builds from the *published* archive — the
bytes the PKGBUILD's `sha256sums` pins, and the bytes a user builds from — rather
than a tarball synthesised from the checkout, whose different gzip and metadata
made the checksum fail. Versions that are not tagged yet still synthesise one,
with the checksum skipped. A guard step now compares the pinned hash against the
published archive, so a stale or wrong hash fails in CI instead of for a user;
while the hash is still `SKIP` (it cannot exist before the tag does) it warns.

## 0.1.0 — 2026-10-17

First release. Configure a GamaKay LK67 from Linux over USB, with a GUI and a
CLI, packaged for the four common Linux targets.

**Features**

- Device info: model, firmware, active profile, and the options the firmware
  exposes.
- Lighting: onboard modes with brightness, speed and direction, rainbow, RGB
  colour, and a live preview of the board in the GUI.
- Keymap: remap keys on the base **and Fn** layers across the board's 8
  profiles, by slot, by key name, by pressing a key on the keyboard (the GUI's
  VIA-style flow), or with a key picker. Plus `keymap reset`.
- Macros: 50 onboard slots, a GUI recorder (press Record, type, Stop, save) that
  captures delays, and a CLI step DSL.
- TOML profiles for lighting and remaps (`lk67ctl apply`).
- `--dry-run` on every command: prints the packets a command would send.
- An iced GUI whose device access runs on a worker thread, so a slow board never
  freezes the window.

**Packaging** — Arch/CachyOS (`makepkg`), Debian/Ubuntu (nfpm),
Fedora/RHEL (nfpm), and a portable AppImage. Every format installs the CLI, the
GUI, the udev rule, the desktop entry and its icons; each is installed into a
container of its own distribution and run in CI before a release is published.

**Audit fixes before the first release** (a full code-quality and security pass):

- The CLI's `--profile` did not select the profile: the keymap commands address
  the *active* profile, so it read and wrote the profile the user was already on
  and left their other layer alone. It now selects the requested profile, runs
  the whole read-modify-write under it, and restores the one that was active.
- `--slot` (and a profile's numeric keymap key) could index past the layer and
  abort the process — `panic = "abort"` — and slots 126/127, which a bulk write
  never reaches, were reported as written when nothing changed. Both are refused
  with a message.
- An unconfirmed layer read (the board answering differently on each pass) was
  handed back and then written whole in a read-modify-write, which could persist
  a mixture of the base and Fn keymaps. It is now an error.
- A short HID feature transfer was treated as a full one, so a failed read could
  be parsed as sixty-four zero bytes. The transfer length is now checked.
- Writes are gated on the **identified** board, not on USB ids: a sibling board
  shares the LK67's ids and a mismatched firmware reports a different id. The
  CLI and the GUI both refuse to write to a board that does not identify as an
  LK67; `--force` overrides, as documented.
- The GUI silently substituted defaults for input it could not parse — a
  mistyped colour became red, an unmodelled board mode became "static", a bad
  repeat count became 1 — and wrote them. It now refuses and says why.
- The CLI accepted any numeric mode code, any macro index and any macro mode.
  All are range-checked now.
- `apply` sent the edge-light command to boards that have no edge light, which
  `light edge set` already refused.
- Unknown TOML keys in a profile were ignored silently; they are an error now.
- The release workflow downloaded and executed two packaging tools with no
  integrity check, ran with a write token on pull requests, and copied the
  checkout (including the credential in `.git`) into the Arch container. Tools
  are pinned to a version and verified against a digest, the token is read-only
  except in the publishing job, and only the source tarball is mounted.
- Release assets are published with a `SHA256SUMS` file.
- The bundled Noto Emoji font's OFL licence now ships with every package, and
  `CREDITS.md` records `ttf-parser` and the real licences of the iced tree.
- Macro slots are read when the Macros tab is opened rather than during connect:
  the connect path used to make 200 feature transfers before the window could do
  anything, which is enough to stall a slow board.
- The desktop entry referenced an icon that did not exist — nothing would have
  appeared in a menu, and the AppImage could not be built. The icon is drawn for
  this project and shipped in the hicolor sizes, with AppStream metadata.
- The declared MSRV (1.85) could not compile the code (let chains); the two
  sites are rewritten so the declaration is true.

**Second audit (before the tag).** A second full pass over the released code —
recent-fix regressions, write safety, code quality and supply chain — found four
things that would have shipped. Fixed:

- **`keymap set --profile N` wrote to the wrong profile.** The keymap commands
  address the *active* profile, so selecting one is a real write; the selection
  was restored *before* the layer was written back, which sent the whole layer —
  including the edit — to the profile the user was already on. The
  read-modify-write now happens inside the activation. Under `--dry-run` the
  select is withheld, so the command says which profile it actually read instead
  of claiming the one it could not select.
- **`macro:<index>` was unbounded.** `--to macro:200` wrote a binding to a slot
  the board does not have; the changelog claimed indexes were range-checked.
  They are now, with a test that fails if the bound is dropped again.
- **The Arch package could not build from the real tag tarball.** GitHub extracts
  a tag to `<repo>-<version>` (`lk67ctl-0.1.0`), not `$pkgname-$pkgver`, so
  `cd "$pkgname-$pkgver"` failed. CI renamed the directory, which hid it; CI now
  produces the real layout, and the package is built from it here.
- **The README documented the tool as refusing the firmware-owned Fn slots.** It
  does not: the firmware ignores writes to them, and the GUI only paints them
  red. Two other README claims were wrong too (`--rgb` is `--color`; the key
  picker has ~115 keys, not ~200).

Also fixed: two silent `profile().unwrap_or(0)` fallbacks that would have read
and written profile 0 when the active profile could not be read; a lost identify
reply used to disable every GUI write until a manual reconnect, and an
unconfirmed profile switch used to leave the GUI showing the old keymap under
the new profile's highlight; the macro list is no longer cached across a
disconnect; the GUI now enforces the same 126-slot write limit as the CLI; an
edge-light speed and an out-of-range option nibble are refused rather than
truncated; parse errors in the macro step DSL no longer report themselves as
transport failures; `apply` and the profile loader reject unknown TOML keys and
confine profile *names* to the profile directory; the `setprobe` example is
read-only unless `SETPROBE_ALLOW_WRITE` is set; and the udev rule now matches
only the settings interface rather than every interface the board exposes.

Supply chain: every GitHub action is pinned to a commit SHA and the two
container images to a digest; the AppImage *runtime* is downloaded and verified
against a pinned digest instead of being fetched unverified at build time;
releases carry a build-provenance attestation as well as `SHA256SUMS`; the
version guard also checks the AppStream `<release version>`; `cargo audit` and a
declared-Minimum-Rust-Version check run in CI, with Dependabot for crates and
actions. The declared MSRV was corrected from 1.85 to **1.86** — the Wayland
stack requires it.

**Dependencies.** The GUI moved to **iced 0.14** (from 0.13), which is what the dependency tree
needed: it replaces `iced_glyphon` with `cryoglyph`, taking `lru` from 0.12 to
0.16, and clears three of the eight advisories the previous release carried
(`lru`'s `IterMut` unsoundness, `instant`, `rustybuzz`) while removing the
duplicate `ttf-parser` majors. It does **not** clear the unsound `lru` entirely:
0.16.4 is still affected by RUSTSEC-2026-0253, which is patched only in
`lru >= 0.18.2` — that needs an upstream bump, and the release profile's
`panic = "abort"` means the reported use-after-free (which needs an unwinding
panic) cannot occur in a shipped build. It does introduce three unmaintained-only
warnings from `zbus`, which 0.14 now uses for Linux theme detection.

The upgrade cost, measured rather than estimated: 15 compile errors, all in the
GUI — the application builder's first argument became a boot function (the title
moved to `.title(..)`), `horizontal_space` became `space::horizontal`,
`canvas::Program::update` takes the event by reference and returns
`Option<Action<Message>>` instead of `(Status, Option<Message>)`, and
`canvas::Text`'s `horizontal_alignment`/`vertical_alignment` fields became
`align_x`/`align_y`. Two dependency counts support the change: 417 crates in the
lock, down from 475.

The declared MSRV rises to **1.90**, which is more than iced 0.14's own 1.88:
two crates in its tree need it (`ordered-float` 5.5 wants 1.90, `uuid` 1.28 wants
1.89), and the CI job `cargo check`s on exactly the declared version so it cannot
drift. That also lets the code use let chains again — the three MSRV workarounds
from the previous pass are gone.

**Not in this release:** 2.4 GHz and Bluetooth. They were investigated and
dropped — the receiver declares no HID feature report, so this protocol cannot
be addressed over it. The measurements are in `docs/protocol.md`.

**Known limitations:**

- USB cable only; mode switching and pairing are physical.
- Only the LK67 is tested; other boards are refused before a write.
- Slots 126 and 127 cannot be remapped (a bulk write does not reach them).
