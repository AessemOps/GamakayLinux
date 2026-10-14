# Reverse-engineering workflow

How the facts in `docs/protocol.md` are established, and how to redo it.

## Clean-room rules

- The vendor app is Electron. Extract it into the gitignored `vendor/` and
  **never commit** the bundle, its JS, or its assets. Only our own notes go
  into `docs/`.
- Raw captures (`*.pcapng`) are gitignored. Decoded, minimal packets may be
  committed as hex fixtures under `tests/fixtures/`.
- Third-party open source that is ported is GPL-3.0 compatible and gets a
  provenance header plus a `CREDITS.md` entry.

## 0. Descriptors on the Linux host

```bash
lsusb -v -d 0461:4002                     # junction: VID/PID, interfaces
for h in /sys/class/hidraw/hidraw*; do
  echo "== $h"; cat "$h/device/uevent"
  cat "$h/device/report_descriptor" | xxd   # find the ≥64-byte Feature report
done
```

Pick the node whose descriptor has a 64-byte Feature report. On the LK67 that
is the boot-keyboard interface (usage page 1 / usage 6).

`hid-tools` (`pip install hid-tools`, provides `hid-decode`) decodes the
descriptor if xxd is not enough.

## 1. Extract the vendor app

The LK67 software downloads from <https://gamakay.com/pages/downloads>. It is
an Electron build:

```bash
npx @electron/asar extract app.asar vendor/app
```

Then read `vendor/app/main.js` / `index.js` for the device table (look for
`yzw_lk67`, `featureReportByteLength`, `company: "gamakay2"`) and the packet
builders (header, opcode, checksum, page/chunk loops). Write only the decoded
layout into `docs/protocol.md`.

## 2. Captures

Capture on the **Linux host**, not in the VM: when the device is passed
through with QEMU's `usb-host`, traffic still crosses the host's usbfs and
`usbmon` sees the URBs. Change one setting per capture, with distinctive
values (`#FF0000`, speed 0, brightness 100), so a diff is unambiguous.

```bash
sudo modprobe usbmon
sudo tshark -i usbmon3 -Y 'usb.urb_type=="S"' -w cap.pcapng
```

For Bluetooth, capture the passed-through adapter and decode the HCI/ATT
writes.

Save only the decoded packets that matter as `tests/fixtures/*.txt` and the
board identity (`0x8F` id) with them.

## 3. Read-back and the family

The firmware is shared across a lineage and answers for parts it lacks, so a
reply is **not** evidence a feature exists. For every read command, write,
read back, and diff. Resolve the family from the table record — never from the
USB product id, which vendors recycle.

## 4. Gate before any write

A write command may enter the allow-list only after a confirming capture or a
hardware read-back closes the loop:

- [ ] opcode, payload offsets and checksum mode recorded
- [ ] the value was set, read back, and survives a power-cycle
- [ ] typing never stops during the write
- [ ] the reset procedure is documented and tested

## 5. On-device drill (per phase, per transport)

Set a value, read it back, power-cycle, confirm persistence. Cross-check
against the official app in the Windows VM in both directions.

## Testing the tool on hardware

`/dev/hidraw*` is root-only by default. Install the udev rule once:

```bash
sudo install -Dm644 packaging/udev/70-gamakay.rules /etc/udev/rules.d/70-gamakay.rules
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=hidraw
# replug the keyboard, then:
gamakay list
gamakay info
```

Without the rule, `gamakay info` reports a permission error; `gamakay list`
still works, because it only reads sysfs.
