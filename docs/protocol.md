# LK67 protocol notes

Our own working notes for the GamaKay LK67. Every wire fact here was either
read off this board (marker **[HW]**) or taken from GAMAKAY's own driver —
opcode tables and packet builders extracted from
`GAMAKAY_v4_setup_3.0.27_WIN20250820.exe` (marker **[REF]**). The installer
stays outside the repo; it is only read to confirm a wire fact. **sharkfin is
no longer a reference: it does not support the LK67.**

This file is the source of truth for the command allow-list in
`lk67-core::guard`. A command that is not here is not sendable.

## Transport

The keyboard is driven over Linux **hidraw** on its **USB cable**. No libusb, no
libudev: the kernel's `hid-generic` driver stays bound, so the board keeps typing
while we configure it.

| | |
|---|---|
| Reports | 64-byte HID feature reports, report id 0, both directions |
| hidraw buffer | 65 bytes: one report-number byte (0) then the 64 data bytes |
| ioctls | `HIDIOCSFEATURE(65)` = `0xC0414806`, `HIDIOCGFEATURE(65)` = `0xC0414807` |
| Collection | the LK67 answers on its **keyboard** collection (usage page `0x0001`, usage `0x0006`) — the same exception the vendor driver carries **[HW]** |
| Reply | a GET reply echoes the opcode in byte 0, except bulk page reads (keymatrix, Fn, macro, userpic), which return raw page bytes **[REF]** |

### The LK67 on USB

`0461:4002` (Primax / ROYUAN), two interfaces:

| node | interface | descriptor | role |
|---|---|---|---|
| `hidraw0` | 1.0, boot keyboard | usage `0x0001:0x0006`, a 64-byte **Feature** report (usage page `0x0C`, usage `0x00`) plus a 1-byte vendor input | the settings node |
| `hidraw1` | 1.1 | consumer/system/keyboard report ids, a 3-byte vendor input on `0xFFFF:0x0001`, **no** feature report | not used |

So the settings node is picked by "carries a ≥64-byte feature report",
preferring a known USB id. `lk67ctl list` shows which node that is. Selection
therefore looks for "carries the settings report", which on a machine with other
HID hardware picks the wrong device — this machine's Corsair mouse carries one
too — so front-ends gate on the device's identity
(`lk67_core::device::is_lk67ctl`) before writing anything.

### Scope: USB cable

**2.4 GHz and Bluetooth are dropped.** Investigated 2026-10-17 and closed:

- The receiver is **`25a7:fa17`** ("Areson Technology Corp / RONGYUAN 2.4G
  Wireless Device"), the board's own dongle, three HID interfaces: a mouse
  collection, a vendor collection (`0xFF00:0x0E`) whose only writable channel is
  **Report ID `0xBA`, an output and an input report of 31 bytes each**, and a boot
  keyboard. **No interface declares a feature report**, so the protocol above
  cannot be addressed over it — and 64 bytes do not fit the 31-byte channel
  either.
- GAMAKAY's own registry treats a dongle as a separate device (`type:"dongle"`,
  usage page `0xFFFF` usage `2`) carrying a protobuf status message
  (`isOnline`/`battery` for a keyboard and a mouse); its configuration still goes
  over feature reports. Its `dongle_common` ids all sit under GAMAKAY's vendor id
  `0x3151`, not under `0x25a7`, so this receiver is likely a generic Areson part
  its own app does not configure either.
- Bluetooth was never captured.

Nothing above is implemented, and no udev rule grants either access. Re-opening
this means starting from the receiver's `0xBA` channel with a capture of the
vendor tooling driving it.

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
   command, in this run or the next. `lk67ctl` sleeps this automatically
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
| keymap bulk | `0x09` | `0x89` | SET `[profile, 0xF8, 1, page]` + 56 bytes at 8, 9 pages, commit on page 8; GET is raw pages `[profile, page]` |
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

Never sent by this tool, by name (`lk67-core::guard::BLOCKED`):

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

1. Preferred: `lk67ctl keymap reset --yes` (sends `0x02`).
2. Hardware, if the board stops responding to the tool: the LK67 factory
   reset key combo (`Fn` + `Esc` held ~3 s, then release) restores defaults.
   Confirm against the vendor manual before relying on it.
3. After a bad write the board normally keeps typing; a stalled control
   endpoint recovers on USB re-enumeration (unplug/replug).

## Feature matrix

USB cable only, verified on the test unit. Features no supported transport
carries are disabled rather than guessed; see "Scope: USB cable" for what the
wireless transports would have needed.

| feature | USB cable |
|---|---|
| identify / revision | yes **[HW]** |
| info / options / debounce | yes **[HW]** |
| lighting | yes **[HW]** |
| underglow (edge light) | yes, mode + colour **[HW]** |
| keymap (base + Fn) | yes **[HW]** |
| macros | yes **[HW]** |
| battery | — (no command over the cable) |

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
- **Edge light (the underglow):** the board *has* one and `SET 0x08` drives it
  — but not the vendor software, which offers no control for it. Its
  device-table record says the board has none, and that is what this project had
  taken as the answer. See "Underglow (edge light)" below. **[HW]**
- **Off** is the options bit `0x10` (`SET 0x06`), applied with
  read-modify-write; `on` clears it. Verified round-trip with the other
  options untouched.
- **Write flow:** see "Writes (SET) — the flow matters" above.

## Underglow (edge light)

**Corrected 2026-10-18, on the 2026-10 firmware. [HW]** The LK67 has a bottom
mesh and side LEDs: GamaKay's product page calls it a "mesh structure at the
bottom of the keyboard and side RGB lights", and the manual's parameters list
"Sidelights: RGB", adjusted from the board with `Fn`+`PgUp`. What the board
lacks is a *vendor control* for them — the software exposes none, and its record
for this model says the board has no side light, which is exactly why the
feature looked absent. The firmware itself answers for them.

`0x08`/`0x88` drive the underglow, in the backlight's payload shape with the
speed **not** inverted:

    [0x08, mode, speed, brightness, (option << 4) | flags, R, G, B, ck8]

Measured on the test unit with the key backlight pinned to a fixed colour, so
the two lights could be told apart by eye and by camera (a region of the frame
was sampled per state, with a reference region to rule out the camera's
auto-exposure):

| what was written | what the LEDs did |
|---|---|
| static, fixed colour | solid colour; RGB honoured, `#123456` round-trips as `12 34 56` |
| mode 0 (`off`) | underglow dark, keys lit — the two are independent |
| mode 2 | breathing, RGB honoured |
| mode 3 | colour cycle through the firmware's own palette (RGB ignored) |
| mode 4 | travelling wave, RGB honoured |
| mode 5 | moving segment(s) |
| speed 0 vs 4 | **no difference**: the wave animates at one fixed rate |
| brightness 4 / 1 / 0 | **no difference**: identical lit-pixel counts and colour |
| the flags nibble (`7`/`8`) | **`8` overrides the colour**: the firmware's rainbow palette on every mode, static included (a static gradient, not the chosen colour); `7` shows RGB |

So the underglow answers to **mode, colour and the rainbow bit**. Brightness and
speed are stored: they survive a write, so a write must preserve them
(read-modify-write) rather than invent them. The flags nibble is not cosmetic —
bit 3 selects the rainbow palette over RGB, so a write that sets a colour sends
`7` (fixed) and only an explicit rainbow request sends `8`. Mode 0 is the off
switch, and it does not touch the keys.

`GET 0x88` returns the stored parameter on this firmware (mode, speed,
brightness, flags, RGB); the pre-update board answered it with zeros, which is
part of why the feature was written off. Capture:
`tests/fixtures/lk67-edge-light.txt` — four written states, read back.

## Phase 3 hardware findings (keymap, USB cable)

- **Base-layer read works:** `0x89 [profile, page]`, eight raw 64-byte pages.
  The LK67's 67 keys land in a non-row-major matrix order (e.g. slot 9 = `A`,
  slot 3 = CapsLock, slot 71 = Fn); the physical layout is therefore derived
  from the layer itself (key name -> slot), not from a static table.
- **The keymap is US QWERTY while the caps are AZERTY:** slot 8 is `Q` and it
  sits under the `A` cap, slot 14 is `W` under `Z`, and so on, so the printed
  legends differ from the keymap names. The GUI preview draws the printed
  legends (the board as it looks), not the keymap names.
- **Right column wiring, top to bottom: `Ins` -> slot 85, `Del` -> slot 86,
  `PgDn` -> slot 87.** The `Ins` cap is *not* slot 86. The factory layer puts
  `Delete` on 85, `PageUp` on 86 and `PageDown` on 87 — so out of the box the
  `Ins` cap sends Delete and the `Del` cap sends PageUp. Binding 85 to `Insert`
  and 86 to `Delete` makes the caps match what they do. **Verified by pressing
  the caps:** the earlier mapping (Ins -> 86) drew a correct-looking preview but
  wrote the wrong slot, so remapping Ins/Del appeared to do nothing.
  There is no `Insert` in the factory keymap at all (`Ins` is Fn+J). The knob is
  programmable through slots 23 (Vol−), 29 (Vol+) and 75 (Play/Pause). Positions
  come from a photo of the board (`crates/lk67-core/src/layout.rs`).
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
  layer, so `lk67ctl keymap reset` requires `--yes`. Not exercised on
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

## Layers (keymaps)

The LK67 keeps **8 keymaps**, one per *profile*; the yc500 device table's
`layer` field is that count. A keymap is addressed by **making its profile
active**:

- `0x05` (`SET_PROFILE`) selects the active profile, `0x85` (`GET_PROFILE`)
  returns it in `reply[1]`. Fire-and-forget, like every SET.
- The keymap commands always address the **active** profile — the profile byte
  in their payload is ignored. Verified: with profile 1 active, writing slot 14
  to `Esc` changed profile 1 only; switching back to profile 0 still read `W`.
  Profile 0 is the factory value of every other profile until one is edited, so
  reading all eight looks identical out of the box.
- The GUI addresses layers this way: the Keymap tab's Layer selector switches
  the active profile and the reads/writes follow it.

### Which Fn-layer keys are fixed

The firmware drives some Fn keys itself, so they cannot be repurposed there.
GAMAKAY's **web driver** (which does support the LK67, unlike the 3.0.27 desktop
app) paints exactly these red with the Fn layer selected — read off a
screenshot and measured against the key grid:

- `A Z E R` — the US `Q W E R` positions — slots 8, 14, 20, 26
- `Win` 11, `Space` 41, `Fn` 71
- `Up` 82, `Left` 83, `Ins` 85, `Del` 86, `PgDn` 87, `Down` 88, `Right` 89

That is the firmware's own Fn set: the QWER/arrow lighting and speed keys, the
right column's mode keys, the Win lock and Space. Notably the keyboard accepts
a write to **every** Fn slot (probed: 0..125 all took F1 and were restored), so
the block is the firmware ignoring those entries, not the protocol refusing
them. `DeviceRecord::fn_locked` carries the list and the GUI paints those keys
red on the Fn layer.

### The newer packet shape is not supported

Vendor app 3.0.27 builds the keymap commands with a different header — read
`0x89 [profile, 0xFF, page, subProfile]`, write `0x09 [profile, 0xFF, page,
len, last, subProfile, <56 bytes at byte 8>]` — addressing a profile *and* one
of four "sub-profiles" directly. **This board echoes that request back instead
of answering it**, so the shape is unrecognised here (its firmware speaks the
older two-byte form). Its own Fn read is `0x90`, which this board does answer.

Checksum, both families: byte 7 = `255 - (sum(bytes 0..=6) & 0xFF)` for the
Bit7 form; byte 8 likewise for Bit8.

## Provenance

- GAMAKAY's own driver, app 3.0.27 — its opcode table (`FEA_CMD_*`) and its
  packet builders are the source of the **[REF]** rows, and its device table of
  the `layer` field. Extracted from the NSIS installer's `resources/app`
  (an Electron bundle) and `iot_driver.exe`; nothing is copied from it.
- LK67 USB and HID descriptors — read from this machine in Phase 0.
