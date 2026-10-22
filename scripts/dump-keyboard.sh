#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Read-only dump of a keyboard's HID settings interface, for adding support
# for a board this project does not drive yet.
#
#   scripts/dump-keyboard.sh                     # every hidraw node
#   scripts/dump-keyboard.sh --device /dev/hidraw2
#   scripts/dump-keyboard.sh --full              # scan 0x80..0xFF, not 0x80..0x9F
#   sudo scripts/dump-keyboard.sh                # if opening the node is denied
#
# For each node it records the identity (sysfs, udev), the raw report
# descriptor, and the reply to every documented app-mode GET opcode. Nothing is
# written to the board: the only feature reports sent are GET requests, and no
# SET opcode is ever used.
#
# Serial numbers and unique IDs are redacted. The dump goes into a new
# directory (mode 700) and nothing is uploaded anywhere.
set -euo pipefail

umask 077

PROG="${0##*/}"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT=""
FULL=0
SCAN=1
declare -a NODES=()

die() { echo "$PROG: $*" >&2; exit 1; }

usage() {
	sed -n '3,17p' "$0" | sed 's/^# \{0,1\}//'
	exit "${1:-0}"
}

while [ $# -gt 0 ]; do
	case "$1" in
	-d | --device)
		[ $# -ge 2 ] || die "--device needs a path"
		NODES+=("$2")
		shift 2
		;;
	-o | --out)
		[ $# -ge 2 ] || die "--out needs a directory"
		OUT="$2"
		shift 2
		;;
	--full) FULL=1; shift ;;
	--no-scan) SCAN=0; shift ;;
	-h | --help) usage 0 ;;
	*) die "unknown argument $1 (see --help)" ;;
	esac
done

command -v python3 >/dev/null 2>&1 || die "python3 is required"

[ -n "$OUT" ] || OUT="lk67ctl-dump-$STAMP"
mkdir -p "$OUT"
[ -w "$OUT" ] || die "cannot write to $OUT"

if [ ${#NODES[@]} -eq 0 ]; then
	for n in /dev/hidraw*; do
		[ -e "$n" ] && NODES+=("$n")
	done
fi
[ ${#NODES[@]} -gt 0 ] || die "no /dev/hidraw* nodes; is the keyboard plugged in?"

for n in "${NODES[@]}"; do
	[ -e "$n" ] || die "no such device: $n"
done

# Anything that could identify the machine rather than the board is not
# recorded; these keys can identify the individual unit, so blank them.
redact() {
	sed -E 's/^((.*)(ID_SERIAL|ID_SERIAL_SHORT|HID_UNIQ|uniq|serial)[^=]*=).*/\1(redacted)/I'
}

{
	echo "# lk67ctl keyboard dump — $STAMP"
	echo
	echo "## Host"
	uname -srm
	echo
	echo "## Distribution"
	grep -E '^(PRETTY_NAME|ID|VERSION_ID)=' /etc/os-release 2>/dev/null || true
	echo
	echo "## lk67ctl"
	if command -v lk67ctl >/dev/null 2>&1; then
		lk67ctl --version
	else
		echo "(not installed)"
	fi
	echo
	echo "## USB"
	if command -v lsusb >/dev/null 2>&1; then
		lsusb
		echo
		lsusb -t
	else
		echo "(lsusb not installed)"
	fi
	echo
	echo "## hidraw nodes"
	ls -l /dev/hidraw* 2>/dev/null || true
	echo
	echo "## Groups"
	id -nG
} >"$OUT/system.txt"

for node in "${NODES[@]}"; do
	name="${node##*/}"
	dir="$OUT/$name"
	sysfs="/sys/class/hidraw/$name/device"
	mkdir -p "$dir"

	{
		echo "# $node"
		echo
		if [ -d "$sysfs" ]; then
			for attr in uevent modalias country physical_location; do
				[ -f "$sysfs/$attr" ] || continue
				echo "== $attr"
				redact <"$sysfs/$attr"
				echo
			done
			# Parent USB device attributes, when this is a USB device.
			for up in "$sysfs" "$sysfs/.." "$sysfs/../.."; do
				[ -r "$up/idVendor" ] || continue
				for attr in idVendor idProduct product manufacturer serial bcdDevice version; do
					[ -f "$up/$attr" ] || continue
					printf '%s: %s\n' "$attr" "$(redact <"$up/$attr")"
				done
				break
			done
		else
			echo "no sysfs directory at $sysfs"
		fi
		echo
		if command -v udevadm >/dev/null 2>&1; then
			echo "== udevadm"
			udevadm info -q all -n "$node" 2>/dev/null | redact || true
		else
			echo "(udevadm not installed)"
		fi
	} >"$dir/device.txt"

	if [ -f "$sysfs/report_descriptor" ]; then
		cp "$sysfs/report_descriptor" "$dir/report-descriptor.bin"
		if command -v xxd >/dev/null 2>&1; then
			xxd -p -c 32 "$dir/report-descriptor.bin" >"$dir/report-descriptor.hex"
		else
			od -An -tx1 -v "$dir/report-descriptor.bin" |
				tr -s ' \n' ' ' | fold -w 64 >"$dir/report-descriptor.hex"
		fi
		if command -v hid-decode >/dev/null 2>&1; then
			hid-decode "$dir/report-descriptor.bin" >"$dir/report-descriptor.decoded" 2>&1 || true
		fi
	else
		echo "report descriptor not readable (try sudo)" >"$dir/report-descriptor.txt"
	fi

	if [ "$SCAN" -eq 1 ]; then
		start="0x80"
		end="0x9F"
		[ "$FULL" -eq 1 ] && end="0xFF"
		python3 - "$node" "$sysfs" "$start" "$end" >"$dir/feature-scan.txt" 2>"$dir/feature-scan.err" <<'PY' || echo "scan failed, see feature-scan.err" >"$dir/feature-scan.txt"
import fcntl
import os
import select
import sys

node, sysfs, start, end = sys.argv[1], sys.argv[2], int(sys.argv[3], 16), int(sys.argv[4], 16)

# The kernel's hidraw ioctls take one leading report-ID byte plus the 64-byte
# report. Everything here is a GET: the LK67's app-mode GET opcodes carry bit 7,
# and no SET opcode is ever sent.
IOC_WRITE, IOC_READ = 1, 2


def ioc(direction, type_, nr, size):
    return (direction << 30) | (size << 16) | (ord(type_) << 8) | nr


BUF = 65
SET_FEATURE = ioc(IOC_READ | IOC_WRITE, "H", 0x06, BUF)
GET_FEATURE = ioc(IOC_READ | IOC_WRITE, "H", 0x07, BUF)


def feature_sizes(descriptor):
    """(report id, bytes) for each Feature main item in a HID descriptor."""
    sizes, report_size, report_count, report_id = [], 0, 0, 0
    i = 0
    while i < len(descriptor):
        prefix = descriptor[i]
        i += 1
        if prefix == 0xFE:  # long item: size, tag, data
            if i + 1 >= len(descriptor):
                break
            i += 2 + descriptor[i]
            continue
        size_code = prefix & 0x03
        length = 4 if size_code == 3 else size_code
        tag = (prefix >> 4) & 0x0F
        kind = (prefix >> 2) & 0x03
        data = descriptor[i:i + length]
        i += length
        value = int.from_bytes(data, "little") if data else 0
        if kind == 0 and tag == 0xB:  # main item, Feature
            sizes.append((report_id, report_count * report_size // 8))
        elif kind == 1:  # global item
            if tag == 7:
                report_size = value  # Report Size 0x75
            elif tag == 8:
                report_id = value  # Report ID 0x85
            elif tag == 9:
                report_count = value  # Report Count 0x95
    return sizes


def request(opcode):
    """A zero-padded GET packet, Bit7-checksummed (bytes 0..6, stored at 7)."""
    buf = bytearray(64)
    buf[0] = opcode
    buf[7] = 0xFF - (sum(buf[:7]) & 0xFF)
    return buf


def send(fd, packet):
    wire = bytearray(BUF)
    wire[1:] = packet
    fcntl.ioctl(fd, SET_FEATURE, wire, True)


def reply_from_input(fd, opcode):
    for attempt in range(5):
        ready, _, _ = select.select([fd], [], [], 0.02 * (attempt + 1))
        if not ready:
            continue
        try:
            data = os.read(fd, 64)
        except BlockingIOError:
            continue
        if data and data[0] == opcode:
            return data, "input report"
    return None, None


def reply_from_feature(fd, opcode):
    wire = bytearray(BUF)
    fcntl.ioctl(fd, GET_FEATURE, wire, True)
    data = bytes(wire[1:])
    if data[0] == opcode:
        return data, "feature report"
    return None, None


descriptor_path = os.path.join(sysfs, "report_descriptor")
try:
    with open(descriptor_path, "rb") as fh:
        descriptor = fh.read()
except OSError as exc:
    print(f"cannot read {descriptor_path}: {exc}")
    sys.exit(0)

sizes = feature_sizes(descriptor)
print("Feature reports in the report descriptor (report id: bytes):")
print("  " + (", ".join(f"{rid}: {size}" for rid, size in sizes) if sizes else "(none)"))
if not any(size >= 64 for _, size in sizes):
    print()
    print("No 64-byte Feature report on this interface: it is not a settings")
    print("interface for the LK67 protocol. Opcode scan skipped.")
    sys.exit(0)

try:
    fd = os.open(node, os.O_RDWR | os.O_NONBLOCK)
except OSError as exc:
    print(f"cannot open {node}: {exc}")
    print("The udev rule grants access to the logged-in user; if this was run over")
    print("ssh or as another user, try sudo.")
    sys.exit(0)

try:
    # Whatever is queued (keystrokes, stale replies) is not ours to record.
    while True:
        try:
            os.read(fd, 64)
        except OSError:
            break
    print()
    print(f"GET scan 0x{start:02X}..0x{end:02X} (read-only; a reply is not proof a")
    print("feature exists — see docs/re-workflow.md):")
    for opcode in range(start, end + 1):
        send(fd, request(opcode))
        reply, source = reply_from_input(fd, opcode)
        if reply is None:
            try:
                reply, source = reply_from_feature(fd, opcode)
            except OSError:
                pass
        if reply is None:
            print(f"0x{opcode:02X}  no reply")
        else:
            print(f"0x{opcode:02X}  {reply.hex(' ')}  ({source})")
finally:
    os.close(fd)
PY
	else
		echo "scan skipped (--no-scan)" >"$dir/feature-scan.txt"
	fi
done

{
	echo "lk67ctl keyboard dump — $STAMP"
	echo
	echo "What this is: the HID identity, raw report descriptor and feature-report"
	echo "replies of each attached HID device. It is meant to be published with a"
	echo "request to support a board that lk67ctl does not drive yet."
	echo
	echo "Read-only: the only reports written are GET requests. No settings are"
	echo "changed and nothing is stored on the board."
	echo
	echo "Privacy: serial numbers and unique IDs are redacted (see the redact()"
	echo "function in the script). Device names, vendor/product IDs, the kernel and"
	echo "distribution, and the report contents are included — read the files"
	echo "before sharing them publicly."
	echo
	echo "Reading feature-scan.txt: one line per opcode, the reply bytes, and where"
	echo "the reply came from. \"no reply\" means the board ignored the opcode or"
	echo "does not implement it. A reply is not proof that a feature exists: the"
	echo "firmware answers for parts of its family it lacks."
	echo
	echo "Docs: docs/re-workflow.md and docs/protocol.md."
} >"$OUT/README.txt"

echo "wrote $OUT"
echo "review it for anything you do not want public, then attach it to an issue."
