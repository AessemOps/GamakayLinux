#!/usr/bin/env bash
# Rasterise packaging/icons/gamakay.svg into the hicolor sizes packages install.
# The PNGs are committed, so a package build needs no image tooling; run this
# only when the SVG changes.
#
#   scripts/make-icons.sh
set -euo pipefail

cd "$(dirname "$0")/.."
src=packaging/icons/gamakay.svg
out=packaging/icons/hicolor

command -v rsvg-convert >/dev/null || { echo "rsvg-convert not found" >&2; exit 1; }
command -v magick >/dev/null || { echo "ImageMagick (magick) not found" >&2; exit 1; }

for size in 16 24 32 48 64 128 256 512; do
  dir="$out/${size}x${size}/apps"
  mkdir -p "$dir"
  rsvg-convert -w "$size" -h "$size" -o "$dir/gamakay.png" "$src"
done

# The AppImage wants a single icon whose name matches the desktop entry.
magick "$out/256x256/apps/gamakay.png" packaging/icons/gamakay.png
echo "wrote packaging/icons/gamakay.png and $out/*/apps/gamakay.png"
