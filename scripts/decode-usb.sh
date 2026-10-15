#!/usr/bin/env bash
# Decode an LK67 usbmon capture into a readable feature-report dump.
#
#   bash scripts/decode-usb.sh /tmp/kilo/lk67-*.pcapng
#
# Prints every control transfer with its setup and data. For a feature SET
# (bmRequestType 0x21) the data byte 0 is the opcode; for a feature GET
# (0xa1) the completion carries the reply.
set -euo pipefail

IN="${1:?usage: decode-usb.sh <capture.pcapng>}"

echo "# feature/output report control transfers (wValue 0x02xx/0x03xx)"
echo "# frame dir bmReq req wValue len  op  data"
tshark -r "$IN" \
  -Y 'usb.transfer_type==0x02 && usb.setup.wValue>=0x0200 && usb.setup.wValue<=0x03ff' \
  -T fields \
  -e frame.number -e usb.urb_type -e usb.bmRequestType -e usb.setup.bRequest \
  -e usb.setup.wValue -e usb.setup.wLength -e usb.capdata -e usb.data_fragment 2>/dev/null \
| awk -F'\t' '{
    d = ($7 != "") ? $7 : $8;
    gsub(/:/, "", d);
    op = (length(d) >= 2) ? substr(d, 1, 2) : "";
    printf "%-6s %-3s 0x%-2s %-3s 0x%-4s %-4s %-2s %s\n", $1, $2, $3, $4, $5, $6, op, d
  }'
