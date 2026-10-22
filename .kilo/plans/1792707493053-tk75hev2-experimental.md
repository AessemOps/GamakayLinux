# Experimental support: GamaKay TK75HE V2

## Why

The TK75HE V2 is a 75% Hall-effect board in the same ROYUAN firmware line as
the LK67. It is a different **command set** (gen2), not just a different layout,
and the two collide: gen2's `SET_KBOPTION` (`0x09`) is the LK67's
`SET_KEYMATRIX`, gen2's `SET_KEYMATRIX` is `0x0A`, gen2's `SET_DEBOUNCE` is
`0x06`. A write with the wrong family's opcode lands on a live register, so
guesswork is not an option.

This branch adds the safe half: the board is **recognised**, `info` and
`light get` work, and every write is refused — `--force` included.

## Evidence

- **Board identity:** `3151:5030`, device id **3833**, protocol family
  **gen2**, four onboard profiles, edge light, TFT screen, magnetic switches.
  Source: sharkfin's board page and registry (GPL-3.0-or-later, see
  `CREDITS.md`).
- **gen2 command set and layouts:** sharkfin `docs/PROTOCOL.md`, marked `[FW]`
  there — read out of a gen2 firmware, not round-tripped on this board. Recorded
  in `docs/protocol.md` → "The gen2 family (TK75HE V2, experimental)", with the
  collision table.
- **Shared parts:** 64-byte feature reports, report ID 0, `0xFF - (sum & 0xFF)`
  checksums (Bit7 / Bit8), the `0x8F` handshake, and the lighting payload.
- The board is **not** attached to the development machine, so nothing has been
  verified on hardware in this branch.

## What changes

- `device.rs`: `Family::Gen2`; `DeviceRecord.experimental`; the `TK75HEV2`
  record (`usage page 0xFFFF`, usage 2 — not the LK67's boot-keyboard
  collection).
- `cmd.rs`: gen2 opcodes, documented as unverified.
- `guard.rs`: the guard is keyed to the family, and the gen2 allow-list is
  **reads only** (`0x8F`, `0x80`, `0x87`, `0x88`, `0x84`). `--force` does not
  widen it: `check_writable` refuses experimental records before its `--force`
  branch.
- `ops.rs`: `profile()` picks `0x84` for gen2. `Session::set_family` re-keys
  the session and its guard after the shared handshake.
- CLI: `info` prints the family and the read-only status, skips the yc500-only
  options read for gen2, and the read paths set the family before their first
  family-specific command. `light get` is unchanged (shared opcodes).
- GUI: the worker tracks the family, keys its guard to it, and does not read the
  keymap on gen2 (that opcode is the options register there). Writes stay
  gated to the LK67 as before.
- udev: `3151:5030` granted, by USB id (the interface number is unverified).

## Next steps (after this lands)

1. Attach a TK75HE V2 and record `info` + `light get` + `dump-keyboard.sh`.
2. Verify each allowed GET against that dump; extend the allow-list one opcode
   at a time.
3. Keymap read (`0x8A`, `[profile, 0xFF|slot, page, os]`, four profiles,
   per-OS layers).
4. Then one write at a time: write, read back, power-cycle, diff — the same
   gate as `docs/re-workflow.md` §4 before anything joins the write allow-list.

## Verification

- `cargo fmt --check`, `cargo build --workspace`, `cargo test --workspace`
  (new tests: the record/family, the gen2 read-only guard, gen2 profile read),
  `cargo clippy --workspace --all-targets -- -D warnings`.
- On the LK67 (yc500), `info` and `light get` still work, and `keymap get`
  still reads the right register — the family dispatch must not change the
  board this project does support.
