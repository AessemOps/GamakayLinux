# LK67 protocol notes

Our own working notes for the GamaKay LK67. Every wire fact here is either
read off this board in Phase 0 (marker **[HW]**) or taken from the GPL-3.0
`sharkfin` protocol document and cross-checked against the LK67's descriptors
(marker **[REF]**). Nothing is copied from the vendor's JavaScript bundle; the
bundle stays in a gitignored `vendor/` and is only read to confirm a layout.

This file is the source of truth for the command allow-list in
`gamakay-core::guard`. A command that is not here is not sendable.

## Transport

The keyboard is driven over Linux **hidraw**. No libusb, no libudev: the
kernel's `hid-generic` driver stays bound, so the board keeps typing while we
configure it, and the same path covers USB, the 2.4 GHz dongle and Bluetooth
HOGP (surfaced through `uhid`).

| | |
|---|---|
| Reports | 64-byte HID feature reports, report id 0, both directions |
| hidraw buffer | 65 bytes: one report-number byte (0) then the 64 data bytes |
| ioctls | `HIDIOCSFEATURE(65)` = `0xC0414806`, `HIDIOCGFEATURE(65)` = `0xC0414807` |
| Collection | wireless boards expose a `0xFFFF` usage-page collection; the LK67 answers on its **keyboard** collection (usage page `0x0001`, usage `0x0006`) — the same exception the vendor driver carries **[HW]** |
| Reply | a GET reply echoes the opcode in byte 0, except bulk page reads (keymatrix, Fn, macro, userpic), which return raw page bytes **[REF]** |

### The LK67 on USB

`0461:4002` (Primax / ROYUAN), two interfaces:

| node | interface | descriptor | role |
|---|---|---|---|
| `hidraw0` | 1.0, boot keyboard | usage `0x0001:0x0006`, a 64-byte **Feature** report (usage page `0x0C`, usage `0x00`) plus a 1-byte vendor input | the settings node |
| `hidraw1` | 1.1 | consumer/system/keyboard report ids, a 3-byte vendor input on `0xFFFF:0x0001`, **no** feature report | not used |

So the settings node is picked by "carries a ≥64-byte feature report",
preferring a known USB id. `gamakay list` shows which node that is.

The 2.4 GHz dongle VID/PID and the Bluetooth HID descriptor are **open
items** (Phase 0, not yet captured). Until they are known, selection falls
back to "carries the settings report"; do not assume a dongle relays every
command.

## Checksums

`0xFF - (sum & 0xFF)`.

| mode | covers | stored at | used by |
|---|---|---|---|
| Bit7 | bytes 0..=6 | byte 7 | every app-mode command except the next row |
| Bit8 | bytes 0..=7 | byte 8 | `SET_LEDPARAM`, `SET_SLEDPARAM` |
| none | | | the USB boot data path (never used by us) |

Only the **request** carries a checksum we compute and verify. The LK67 does
**not** recompute a reply checksum: every GET reply echoes the opcode and
leaves byte 7 holding the request's checksum byte (`0xFF - opcode`), whatever
payload it wrote into bytes 1..6. The one exception is `0x87`, whose RGB data
runs through byte 7, so its byte 8 is 0 on GET. **[HW]** Do not validate GET
reply checksums.

### Writes (SET) — the flow matters **[HW]**

A SET is applied, but the board then spends a moment committing the change
(to flash), and during that window it NAKs the control endpoint. Any feature
transfer sent in that window times out and leaves the endpoint **stalled**
(`ioctl (SFEATURE): Protocol error`) until the board is re-enumerated. Two
rules follow:

1. **Fire-and-forget.** After a SET, do not read a reply in the same exchange.
   A SET does answer with the applied state, but reading it back immediately is
   exactly the transfer that stalls the pipe. The value is applied either way.
2. **Commit pause.** Leave a gap (~1.5 s is safe) after a SET before the next
   command, in this run or the next. `gamakay` sleeps this automatically
   (`WRITE_COMMIT_MS`); a command run by hand right after a write should wait.

Reads never stall, and a write is preceded by a small gap (`WRITE_GAP_MS`)
because the firmware also dislikes a SET issued immediately after a read reply.



## Families

Two command sets share opcode numbers. The LK67 is **yc500**. A write sent to
the wrong family lands on a live register (e.g. yc500 `SET_KEYMATRIX 0x09` is
the gen2 options bitfield), so the family is resolved from the table record,
never guessed from USB ids.

## Command reference (yc500)

Offsets are into the 64-byte report before the checksum byte.

| command | SET | GET | payload |
|---|---|---|---|
| identify | — | `0x8F` | reply `[0x8F, id:u32 LE, …]` |
| revision | — | `0x80` | reply `(reply[2]<<8) \| reply[1]`, elided below |
| profile | `0x05` | `0x85` | `[index]`; reply `reply[1]` |
| options | `0x06` | `0x86` | `[0, flags, fnMatrix, powerSave]`; reply byte2 flags |
| auto-OS | `0x17` | `0x97` | `[0\|1]`; reply `reply[1]==1` |
| debounce | `0x11` | `0x91` | value at byte 2, range 1..10 |
| sleep timers | `0x12` | `0x92` | four u16 LE seconds; SET at bytes 8..16, GET reply also at 8..16 (see note) |
| LED param | `0x07` | `0x87` | `[mode, 5-speed, brightness, (option<<4)\|flags, R, G, B]`, Bit8 |
| edge param | `0x08` | `0x88` | same layout as LED, speed **not** inverted |
| keymap one slot | `0x13` | `0x89` | `[profile, slot]`, value at bytes 8..12; GET is raw pages `[profile, page]` |
| Fn one slot | `0x15` | `0x90` | `[profile, slot]`, value at bytes 8..12; GET is raw pages |
| keymap bulk | `0x09` | `0x89` | `[profile, 0xF8, 1, page]` + 56 bytes at 8, 9 pages, commit on page 8 |
| macro | `0x0B` | `0x8B` | `[slot, 0, 1, page, 0, 0]` + 56 bytes at byte 8, five pages |
| per-key colour | `0x0C` | `0x8C` | 7 × 56-byte pages of RGB triples in matrix order |
| factory reset | `0x02` | — | bare opcode; the board needs ~4 s |

### Options bitfield (byte 2 of `0x86`)

`1` Win lock, `2` Mac mode, `8` WASD swap, `16` backlight off, `32` side light
off, `64` keyboard mode, `128` keyboard lock. Byte 3 is the Fn matrix flag,
byte 4 power save. **Read-modify-write** so unmodelled bits survive.

### LED parameters

```
[0x07, mode, wire_speed, brightness, (option<<4)|flags, R, G, B, ck8]
```

- yc500 wire speed is 1..5: the host's 0..4 is **inverted** (host 0 → 5).
- Brightness is direct, **0..4**. The LK67 stores 5 when the LEDs are blank;
  sending 5 blanks them, so 5 is rejected and a stored 5 reads back as 4. **[HW]**
- Flags nibble: for the LK67 the default pair is `7` = fixed colour,
  `8` = rainbow. This is per board; confirm by read-back before trusting it.
- White `0xFFFFFF` transmits as `0xFAFAFA`; any colour below `0x080808`
  renders unlit, so "off" is the options bit, not a black write.
- On GET, a flags nibble 0..6 is a firmware preset index that overrides RGB.
- Wire modes: 1 static, 2 breathing, 3 spectrum, 4 wave, 5 ripple, 6 star
  dots, 7 flow, 8 key shadow, 9 layers, 10 sine, 11 spring, 12 neon,
  14 radiant, 15 loop, 16 colour grid, 17 snowfall, 18 meteor, 19 silent snow.
  Modes 13/20/21/22/25 need host-side data and are out of scope for v1.

### Keymap slots

Each slot is 4 bytes tagged by byte 0:

| byte 0 | meaning |
|---|---|
| `0x00` | HID usage in byte 2 |
| `0x03` | consumer usage, `[0, lo, hi]` |
| `0x09` | macro, `[mode, index, 0]` |
| `0x0A` | special, `[1,0,0]` Fn |
| all zero | Fn layer: fall through; base layer: the key is dead |

Which physical key maps to which slot is the **LK67 LED/key index map**, added
in Phase 3.

## Block-list

Never sent by this tool, by name (`gamakay-core::guard::BLOCKED`):

| opcode | why |
|---|---|
| `0xAC` | flash-chip erase (~55 s) |
| `0x2C` | display erase |
| `0x7F` | USB boot entry (only with trailer `55 AA 55 AA`) |
| `0x30` | display-chip boot entry |
| `0x1C` / `0x1E` | sensor recalibration; on-then-off commits zeroed tables |
| `0xF8` | RF boot entry |
| `0xBA` | boot-mode image transfer |

There is **no firmware flashing path** in this project.

## Recovery

Before any write phase is exercised on hardware, record and verify the reset
procedure:

1. Preferred: `gamakay keymap reset` / `gamakay settings reset` once
   implemented (sends `0x02`).
2. Hardware, if the board stops responding to the tool: the LK67 factory
   reset key combo (`Fn` + `Esc` held ~3 s, then release) restores defaults.
   Confirm against the vendor manual before relying on it.
3. After a bad write the board normally keeps typing; a stalled control
   endpoint recovers on USB re-enumeration (unplug/replug).

## Transport support matrix

Filled in as transports are captured. Cable is the only verified column so
far; the others are open Phase 0 items and unsupported features must be
disabled rather than guessed.

| feature | USB cable | 2.4 GHz | Bluetooth |
|---|---|---|---|
| identify / revision | yes **[HW]** | ? | ? |
| info / options / debounce | yes **[HW]** | ? | ? |
| lighting | yes **[HW]** | ? | ? |
| keymap (base + Fn) | yes **[HW]** | ? | ? |
| macros | yes **[HW]** | ? | ? |
| battery | — (no command; receiver status only) | ? | ? |

## Phase 1 hardware findings (LK67, USB cable)

Captured from the test unit. Fixtures: `tests/fixtures/lk67-usb-reads.txt`.

| field | observed |
|---|---|
| identify `0x8F` | id **149** (so the board is the correct LK67 firmware, not variant 71) |
| revision `0x80` | reply `05 02` → `0x205`, renders **205** |
| profile `0x85` | 0 |
| LED `0x87` | `01 04 05 07 95 07 e1` → mode 1 (static), speed 4, brightness 5, flags 7 (fixed colour), RGB `#9507E1` |
| options `0x86` | all bits 0 |
| debounce `0x91` | 0 (below the documented 1..10 range, so likely "not reported") |
| sleep `0x92` | all four timers 0 |
| auto-OS `0x97` | 0 (off) |

**Sleep GET offsets.** Reference notes place the `0x92` reply at bytes 1..9,
but on this board byte 7 is the request's checksum byte, so a 4×u16 payload at
1..9 would overwrite it. The reply is all zero past the opcode and byte 7 is
`0xFF - 0x92 = 0x6D`, so we read the timers from bytes 8..16 (the SET
offsets). **Open item:** confirm by writing a timer and reading it back in
Phase 2, which also settles whether the 1..9 layout ever applies.

**Reply checksum.** Confirmed on this board: no reply recompute (see
Checksums). Our earlier synthetic fixtures had wrong reply checksums; the
committed fixtures are the real captures.

## Phase 2 hardware findings (lighting, USB cable)

- **Backlight SET works** and persists across re-enumeration (the setting is
  saved to onboard memory). Verified: set `wave #ff0000` brightness 3 speed 2
  direction 1, read back in a separate process, then restored.
- **Brightness is 0..4.** The unit reported a stored 5 (LEDs blank); sending 5
  from the GUI blanked the lighting, so 5 is now rejected and clamped. **[HW]**
- **Speed** is the host scale 0..4; the wire value is `5 - host` for the
  backlight and `host` for the edge light. The unit's original state was host
  speed 1 (wire 4).
- **Edge light:** the LK67 has none. `GET 0x88` still answers (a zero ack), so
  presence comes from the device record, not the reply; `gamakay light edge`
  is refused without `--force`.
- **Off** is the options bit `0x10` (`SET 0x06`), applied with
  read-modify-write; `on` clears it. Verified round-trip with the other
  options untouched.
- **Write flow:** see "Writes (SET) — the flow matters" above.

## Phase 3 hardware findings (keymap, USB cable)

- **Base-layer read works:** `0x89 [profile, page]`, eight raw 64-byte pages.
  The LK67's 67 keys land in a non-row-major matrix order (e.g. slot 9 = `A`,
  slot 3 = CapsLock, slot 71 = Fn); the physical layout is therefore derived
  from the layer itself (key name -> slot), not from a static table.
- **Single-slot write `0x13` is ignored** on this unit: the write is accepted
  but the slot does not change on read-back. Same for the Fn single write.
- **Bulk layer write `0x09` works:** `[profile, 0xF8, 1, page]` + 56 bytes at
  byte 8, nine pages, commit on page 8. Verified: read the layer, change one
  slot (CapsLock -> Esc), upload, read back, restore. Slots 126..127 never
  transmit (504 of 512 bytes), so they cannot be remapped.
- **Fn layer read `0x90`** did not return pages on the *pre-2026-10* firmware
  (it echoed the request); the **2026-10 update fixed it** — see the firmware
  section below. The Fn single write `0x15` is likewise ignored; the Fn bulk
  write `0x10` works.
- **Reset** is the factory reset `0x02`; it resets the whole keyboard, not one
  layer, so `gamakay keymap reset` requires `--yes`. Not exercised on
  hardware (destructive).
- Writes use the same fire-and-forget + commit-pause flow as lighting; a
  multi-page write spaces its pages (`PAGE_GAP_MS`) and takes one commit pause
  at the end.

## Phase 4 hardware findings (macros, USB cable)

- **Macro write is `0x0B`**, the base-class value, not the reference's yc500
  `0x16` (which this firmware ignores). Shape: header `[slot, 0, 1, page, 0,
  0]`, 56 bytes at byte 8, **five** pages (0..4). Verified by a vendor capture
  and then on hardware: write a slot, read it back, delete it.
- **Macro reads** (`0x8B [slot, page]`, four raw pages) return the stored blob;
  a vendor-written macro decodes cleanly, so the blob format is confirmed.
- **Binding works** through the keymap: slot value `[0x09, mode, index, 0]`,
  written with the bulk keymap upload `0x09`.

## 2026-10 firmware update (LK67, USB cable)

The board was updated to the latest firmware. It re-enumerates and behaves
differently; the new state is the current baseline.

| | pre-update | 2026-10 update |
|---|---|---|
| USB ids | `0461:4002` | **`3151:4003`** |
| identify `0x8F` id | 149 | **290** |
| revision `0x80` | 205 | **310** |
| Fn read `0x90` | echoed the request | **returns pages** |
| macro read `0x8B` | zeros (none stored) | **returns macros** |
| macro write | `0x16` did not stick | **`0x0B` works** |

- The identity (id) is firmware-dependent, so a record matches a set of ids
  (`LK67` accepts 149 and 290); USB ids are only a hint.
- **Fn layer now reads and writes.** Read `0x90` returns the same eight raw
  pages as `0x89`; the Fn bulk write is `0x10` with the same shape as the base
  bulk write `0x09` (`[profile, 0xF8, 1, page]` + 56 bytes at byte 8, nine
  pages). Verified: read, change a slot (Grave -> F1), read back, restore.
- **Macros now read.** `0x8B [slot, page]` returns the stored blob. A macro
  written by the vendor tooling decodes cleanly, so the blob format is right.

### Vendor capture (web driver, usbmon)

The desktop app does not detect the board, but a WebHID **web driver** does.
A host `usbmon` capture of that driver (scripts/capture-usb.sh) confirms the
protocol is unchanged and shows the exact packets:

- It uses the same opcodes: `0x8F`, `0x80`, `0x84`, `0x85`, `0x86`, `0x87`,
  `0x91`, `0x92` reads, plus `0x07` lighting and `0x10` Fn bulk write.
- **Fn bulk write** it sends is byte-for-byte our `write_layer` shape, which
  is why the Fn write now round-trips.
- **Lighting** it sends is our `[mode, wire_speed, brightness, (option<<4)|
  flags, R, G, B]` with a Bit8 checksum.
- **Macro write** it sends is `0x0B` (see Phase 4), which resolved the last
  open item.
- It sends one `0xFD` with a zero payload before the rest (purpose unknown;
  not required — our reads and writes work without it).

## Provenance

- `sharkfin` (GPL-3.0-or-later, © JR Lanteigne) — the ROYUAN/`yc500` protocol
  document this file's **[REF]** rows are derived from, and a structural
  reference for the transport/guard split. See `CREDITS.md`.
- LK67 USB and HID descriptors — read from this machine in Phase 0.
