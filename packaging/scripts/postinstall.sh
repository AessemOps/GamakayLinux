#!/bin/sh
# Package postinstall: make the udev rule take effect without a reboot.
#
# The rule has already been installed by the package manager; udev only reads
# rules on start, on reload, or on a device event, so a reload plus a trigger for
# the hidraw subsystem is enough — no replug, no reboot. Failure is not fatal:
# the tool works read-only without the rule, and an unprivileged container may
# have no udev at all.
set -eu

if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || true
    udevadm trigger --subsystem-match=hidraw >/dev/null 2>&1 || true
fi

exit 0
