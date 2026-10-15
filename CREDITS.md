# Credits

GamakayLinux is GPL-3.0-or-later. It stands on protocol work done by others;
this file records what was used and how. `Last reviewed: 2026-10-14`.

## Protocol references

### sharkfin

- Project: [sharkfin](https://github.com/dniminenn/sharkfin), © JR Lanteigne
  `<root@dnim.dev>`, **GPL-3.0-or-later**.
- What: the ROYUAN keyboard HID protocol document (`docs/PROTOCOL.md`), which
  describes the `yc500` command set the LK67 speaks, and a Rust/Tauri
  implementation that informed the transport/guard/session split.
- How used: `docs/protocol.md` **[REF]** rows are derived from sharkfin's
  protocol document; the crate layout (`wire`/`protocol` separation, the
  checksum split, the receiver relay description) follows the same structure.
  No sharkfin source file is copied verbatim; where a decode/encode routine is
  adapted closely it carries a provenance header.
- Reason GPL-compatible: sharkfin is GPL-3.0-or-later, as is this project.

### OpenRGB (LK67 fork)

- Project: [open-rgb-gamakay-lk67](https://gitlab.com/kuba.dajek4/open-rgb-gamakay-lk67),
  a fork of [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB), **GPL-2.0-or-later**.
- What: an independent Linux implementation of GamaKay lighting.
- How used: reference only, if at all; any ported file will carry a provenance
  header and a note here. GPL-2.0-or-later code may be combined into a
  GPL-3.0-or-later work.

### Vendor software

- The GamaKay Electron app (`gamakay.com/pages/downloads`) is proprietary. It
  is extracted locally into the gitignored `vendor/` for protocol study only
  and is **never** committed, bundled or linked.

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
