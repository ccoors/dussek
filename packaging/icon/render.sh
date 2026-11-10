#!/bin/sh
# dussek.svg (a keyboard on three plates, 512 px; from a generator script in the maintainer's
# notes) → the PNGs (Linux's desktop entry, the README), dussek.ico (Windows), dussek.icns (macOS), and the
# standalones' window icon on X11 (plugin/src/ui/WindowIcon.inc, x11_icon.py).
# Needs rsvg-convert, ImageMagick, png2icns (libicns) and Python's PIL; the results are committed.
set -eu
cd "$(dirname "$0")"
for s in 16 32 48 64 128 256 512; do
    rsvg-convert -w "$s" -h "$s" dussek.svg -o "dussek-$s.png"
done
cp dussek-256.png dussek.png
magick dussek-16.png dussek-32.png dussek-48.png dussek-256.png dussek.ico
png2icns dussek.icns dussek-16.png dussek-32.png dussek-128.png dussek-256.png dussek-512.png >/dev/null 2>&1
python3 x11_icon.py ../../plugin/src/ui/WindowIcon.inc dussek-32.png dussek-64.png dussek-128.png
rm -f dussek-16.png dussek-32.png dussek-48.png dussek-64.png dussek-128.png dussek-256.png dussek-512.png
