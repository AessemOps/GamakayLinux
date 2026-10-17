# Credits

GamakayLinux is GPL-3.0-or-later. It stands on protocol work done by others;
this file records what was used and how. `Last reviewed: 2026-10-16`.

## Protocol references

### sharkfin

- Project: [sharkfin](https://github.com/dniminenn/sharkfin), © JR Lanteigne
  `<root@dnim.dev>`, **GPL-3.0-or-later**.
- What: a ROYUAN keyboard tool. It does **not** support the LK67, so it is no
  longer a reference for this board's protocol, geometry or command set.
- How used: exactly one file — `crates/gamakay-gui/src/patterns.rs` is a port
  of sharkfin's `app/src/lib/effects.ts` (per-mode preview animations); the
  ported file carries the upstream copyright header. Its fidelity for the LK67
  is unverified, since the upstream tool targets other boards; the real fix is
  to derive the animations from this board's own userpic/LED output.
  `docs/protocol.md` no longer derives anything from sharkfin.
- Reason GPL-compatible: sharkfin is GPL-3.0-or-later, as is this project.

### OpenRGB (LK67 fork)

- Project: [open-rgb-gamakay-lk67](https://gitlab.com/kuba.dajek4/open-rgb-gamakay-lk67),
  a fork of [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB), **GPL-2.0-or-later**.
- What: an independent Linux implementation of GamaKay lighting.
- How used: reference only, if at all; any ported file will carry a provenance
  header and a note here. GPL-2.0-or-later code may be combined into a
  GPL-3.0-or-later work.

### Vendor software

- GAMAKAY's own driver, `GAMAKAY_v4_setup_3.0.27_WIN20250820.exe`: an NSIS
  installer around an Electron app plus a native `iot_driver.exe`.
  Proprietary. It is extracted locally (outside the repo) for protocol study
  only and is **never** committed, bundled or linked. `docs/protocol.md`'s
  **[REF]** rows, the opcode table and the layer findings come from it.

### Noto Emoji (bundled font)

- Project: [Noto Emoji](https://fonts.google.com/noto/specimen/Noto+Emoji),
  © Google, **SIL Open Font License 1.1**.
- What: the monochrome emoji font.
- How used: bundled as `crates/gamakay-gui/assets/NotoEmoji.ttf`, with its
  licence alongside as `NotoEmoji-OFL.txt`, and loaded at startup. It is the
  only font available here with the emoji the keyboard previews draw (🐧 ⏸ 🔒 🍎
  🤖 💡): DejaVu Sans has none of them, the packaged Noto fonts are missing some,
  and the colour-emoji font cannot be rendered as text by this renderer.
- Reason GPL-compatible: the OFL 1.1 permits bundling and requires the licence
  to travel with the font, which it does. The font stays under the OFL and is
  not linked into the program.

## Direct dependencies

Bump the `Last reviewed` line above when any of these change.

| Dependency | Crate | License |
|---|---|---|
| `libc` | `libc` | MIT OR Apache-2.0 |
| `clap` | `clap`, `clap_derive`, `clap_builder`, `clap_lex` | MIT OR Apache-2.0 |
| `anyhow` | `anyhow` | MIT OR Apache-2.0 |
| `serde` | `serde`, `serde_derive` | MIT OR Apache-2.0 |
| `toml` | `toml`, `toml_edit`, `serde_spanned`, `toml_datetime`, `winnow` | MIT OR Apache-2.0 |
| `iced` | `iced` (GUI) and its tree (wgpu, winit, cosmic-text, …) | MIT |
