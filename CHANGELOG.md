# Changelog

All notable changes to this project. Version numbers are the tags; the workspace
`Cargo.toml` version is the source of truth and `scripts/check-release-version.sh`
keeps the PKGBUILD in step with it.

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
- TOML profiles for lighting and remaps (`gamakay apply`).
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

**Not in this release:** 2.4 GHz and Bluetooth. They were investigated and
dropped — the receiver declares no HID feature report, so this protocol cannot
be addressed over it. The measurements are in `docs/protocol.md`.

**Known limitations:**

- USB cable only; mode switching and pairing are physical.
- Only the LK67 is tested; other boards are refused before a write.
- Slots 126 and 127 cannot be remapped (a bulk write does not reach them).
