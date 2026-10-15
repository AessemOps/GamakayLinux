#!/usr/bin/env bash
# Capture LK67 USB traffic and write a decoded dump. Run as root:
#   sudo bash scripts/capture-usb.sh 60
#
# Captures the LK67's bus for N seconds (default 60), then decodes it to a
# text file. Do the vendor actions in the Windows VM while it runs.
#
# Output: /var/tmp/lk67-<timestamp>.pcapng (raw) and .txt (decoded).
set -euo pipefail

VID="${VID:-3151}"
PID="${PID:-4003}"
DUR="${1:-60}"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="${2:-/var/tmp/lk67-${STAMP}}"

if [ "$(id -u)" -ne 0 ]; then
  echo "run me with sudo (usbmon needs root)" >&2
  exit 1
fi

modprobe usbmon

line="$(lsusb -d "${VID}:${PID}" || true)"
if [ -z "$line" ]; then
  echo "device ${VID}:${PID} not found on the host bus" >&2
  exit 1
fi
bus="$((10#$(printf '%s' "$line" | sed -n 's/^Bus \([0-9]*\) .*/\1/p')))"

mkdir -p "$(dirname "$OUT")"
echo "device ${VID}:${PID} on usbmon${bus}; capturing ${DUR}s -> ${OUT}.pcapng"
echo
echo "Now, in the Windows VM (keyboard passed through), in the web driver:"
echo "  1. set a macro on a slot and apply/save it   <- the important one"
echo "  2. open the keymap / Fn-layer page"
echo "  3. change the lighting"
echo

tshark -i "usbmon${bus}" -a "duration:${DUR}" -w "${OUT}.pcapng"

echo
echo "decoding -> ${OUT}.txt"
bash "$(dirname "$0")/decode-usb.sh" "${OUT}.pcapng" > "${OUT}.txt" 2>&1
chmod 0644 "${OUT}.pcapng" "${OUT}.txt" 2>/dev/null || true
echo "done."
echo "  raw:      ${OUT}.pcapng"
echo "  decoded:  ${OUT}.txt"
