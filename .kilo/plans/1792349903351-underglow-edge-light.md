# Underglow (edge light) support

Status: implementation. Owner: session 2026-10-18.

## What was wrong

`DeviceRecord::side_light` was `false` for the LK67, so the CLI refused
`light edge` without `--force` and the GUI had no underglow UI at all. The
value came from the **vendor software's** device-table record, and it describes
what that software exposes, not what the hardware has. GamaKay's own product
page ("mesh structure at the bottom of the keyboard and side RGB lights") and
the LK67 manual ("Sidelights: RGB", adjusted with Fn+PgUp) both say the board
has one, and an owner review complains the vendor app "lets me customize
everything but those LED's" — which is exactly why the feature looked absent.

## Evidence (measured on the test unit, 2026-10-18)

Writes went out through `gamakay light edge set --force`; the readings came
from a phone camera pointed at the board, analysed with
`/tmp/kilo/cam/measure.sh` and `analyze.py` (mean RGB of the underglow region,
plus a per-frame spread that separates a static glow from an animated one, and
a fixed reference region to rule out camera auto-exposure).

- **`0x08` drives the underglow.** With the key backlight pinned to fixed
  green, `SET 0x08` fixed red turned the sides/bottom red while the keys stayed
  green — independent control, verified visually and by pixels.
- **`0x88` answers real data** on the 2026-10 firmware (`mode 1, speed 3,
  brightness 4, flags 8, rgb #ff00b6`), where the pre-update board zero-acked.
  Every write round-trips exactly; captured as `tests/fixtures/lk67-edge-light.txt`.
- **Mode 0 turns it off** (lit pixels in the region fall from ~78k to ~14k, the
  residue being keycap bleed) and leaves the keys alone.
- **Modes 1..5 all light it**, each with its own character: 1 solid colour
  (honours RGB), 2 breathing, 3 colour cycle (ignores RGB), 4 travelling wave,
  5 moving segments.
- **Brightness is ignored**: fixed red at brightness 4, 1 and 0 gives 78.4k /
  76.5k / 77.2k lit pixels at the same mean (207, 14, 29). The reference region
  is bit-identical across the three, so this is not the camera compensating.
- **Speed is ignored**: wave at speed 0 and speed 4 produce the same
  frame-to-frame spread (10.7 vs 9.0).
- **The rainbow flag has no visible effect**: 15 frames over 45 s with the flag
  set stay at mean 17/87/100, spread 2.5.

> **Correction, 2026-10-19.** That last bullet is wrong. With the rainbow bit
> set (`flags 8`) the board shows its own rainbow palette and ignores RGB in
> every mode — static included, where it shows a static gradient rather than the
> colour that was written. The probe behind the bullet observed a cycling colour
> (mode 3), which ignores RGB regardless, so it could not see the difference. A
> colour write must therefore clear the bit: see `LedParam::with_mode_and_colour`
> and the CLI's `merge`, and the corrected table in `docs/protocol.md`.

So on this board the underglow honours **mode and colour only**. Everything
else is stored and must survive a write (read-modify-write) but has nothing to
control.

## Design

- `crates/gamakay-core/src/device.rs`: `side_light: true` for the LK67, with the
  evidence and the caveat in the comment.
- CLI: the existing `light edge` path becomes usable without `--force`; add a
  visible `underglow` alias, since that is what the hardware page calls it.
- GUI: an **Underglow** section in the Lighting tab — mode picker (including
  Off), colour and a **Rainbow** toggle — read on demand when the tab opens (the
  connect path already spends its feature-transfer budget; see the `ReadLayer`
  note), written with the same fire-and-forget flow as the backlight. Brightness
  and speed are deliberately **not** offered: the measurements above show they do
  nothing. (The rainbow bit is offered even though the first probe read it as
  inert; see the correction below — it overrides the colour, so leaving it out
  made the colour unusable.)
- Docs: `docs/protocol.md` gets the corrected finding and the measured
  semantics; README/AGENTS/CHANGELOG follow.
- Fixture: `tests/fixtures/lk67-edge-light.txt` + a decoder test.

## Not in scope

- Rendering the underglow in the app's keyboard preview (follow-up).
- The unmodelled wire modes 6..25 on the edge light: accepted by the codec and
  round-tripped, but their rendering is untested.

## Verification

`cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`, a build of the GUI, and a live drill: read the underglow in the
app, change mode and colour, confirm on the board, and confirm the keys are
untouched.
