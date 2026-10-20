#!/usr/bin/env bash
# Rasterise packaging/icons/lk67ctl.svg into the hicolor sizes packages install.
# The PNGs are committed, so a package build needs no image tooling; run this
# only when the SVG changes.
#
#   scripts/make-icons.sh
set -euo pipefail

cd "$(dirname "$0")/.."
src=packaging/icons/lk67ctl.svg
out=packaging/icons/hicolor

command -v rsvg-convert >/dev/null || { echo "rsvg-convert not found" >&2; exit 1; }
command -v magick >/dev/null || { echo "ImageMagick (magick) not found" >&2; exit 1; }

for size in 16 24 32 48 64 128 256 512; do
  dir="$out/${size}x${size}/apps"
  mkdir -p "$dir"
  rsvg-convert -w "$size" -h "$size" -o "$dir/lk67ctl.png" "$src"
done

# The AppImage wants a single icon whose name matches the desktop entry.
magick "$out/256x256/apps/lk67ctl.png" packaging/icons/lk67ctl.png
echo "wrote packaging/icons/lk67ctl.png and $out/*/apps/lk67ctl.png"
