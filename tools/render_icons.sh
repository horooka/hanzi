#!/bin/sh
# Render PNG icon sizes from the scalable source. PNG fallbacks keep the icon
# visible on systems whose gdk-pixbuf build lacks the SVG loader.
set -eu
cd "$(dirname "$0")/.."

SRC=data/icons/hicolor/scalable/apps/io.github.horooka.hanzi.svg
for size in 64 128 256 512; do
    dir="data/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    rsvg-convert -w "$size" -h "$size" "$SRC" -o "$dir/io.github.horooka.hanzi.png"
    echo "wrote $dir/io.github.horooka.hanzi.png"
done
