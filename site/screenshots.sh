#!/bin/bash
# The website's screenshots of the standalone, taken on a virtual display (Xvfb) at twice the size:
#   site/screenshots.sh <out folder>
# grand-sound.png, grand-felt.png, grand-mics.png (Grand A's tabs), grand-visualization.png (the
# Visualization panel, a chord ringing with the sustain latch), vibraphone.png (an instrument of
# another kind). Each run starts the standalone with one instrument and its own empty settings; its
# audio goes to an unconnected JACK client (a dummy jackd when none runs: CI), never to speakers.
# Needs Xvfb, xdotool, ImageMagick's import, and plugin/build/bin/dussek (`just plugin`).
set -euo pipefail
out=$(mkdir -p "${1:?usage: screenshots.sh <out folder>}" && cd "$1" && pwd)
repo=$(cd "$(dirname "$0")/.." && pwd)
bin=$repo/plugin/build/bin/dussek
scale=2
display=:${SCREENSHOT_DISPLAY:-97}
tmp=$(mktemp -d)
pids=()
cleanup() {
    for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
    rm -rf "$tmp"
}
trap cleanup EXIT

Xvfb "$display" -screen 0 2000x1400x24 >/dev/null 2>&1 &
pids+=($!)
# a JACK server for the standalone's audio: the one running (PipeWire's too), else a dummy one
if ! jack_lsp >/dev/null 2>&1; then
    jackd -d dummy -r 48000 -p 256 >/dev/null 2>&1 &
    pids+=($!)
    for _ in $(seq 50); do jack_lsp >/dev/null 2>&1 && break; sleep 0.1; done
fi
sleep 1

# a point of the window's layout at 1x (measured at its default 900 x 624), clicked at the scale
click() { xdotool mousemove --window "$win" "$(($1 * scale))" "$(($2 * scale))" click 1; sleep "${3:-0.4}"; }
tab() { # the tab bar's labels' centers at 1x
    case $1 in sound) click 478 67 ;; felt) click 597 67 ;; mics) click 631 67 ;; esac
}
key() { click "$(($1 * 31 / 2 + 7))" 616 0.12; } # white key n (0: A0), on the strip's lower half

start() { # <instrument folder>: the standalone with only it, settings of its own
    local inst=$1
    if [ -n "${app:-}" ]; then kill "$app" 2>/dev/null || true; wait "$app" 2>/dev/null || true; fi
    rm -rf "$tmp/run" && mkdir -p "$tmp/run/cfg" "$tmp/run/data" "$tmp/run/sys" "$tmp/run/inst"
    ln -s "$repo/instruments/$inst" "$tmp/run/inst/$inst"
    DISPLAY=$display DPF_SCALE_FACTOR=$scale WAYLAND_DISPLAY='' \
        XDG_CONFIG_HOME=$tmp/run/cfg XDG_DATA_HOME=$tmp/run/data XDG_DATA_DIRS=$tmp/run/sys \
        DUSSEK_INSTRUMENTS=$tmp/run/inst PIPEWIRE_PROPS='{ node.autoconnect = false }' DUSSEK_AUTOCONNECT=0 \
        "$bin" >"$tmp/run/log" 2>&1 &
    app=$!
    pids+=("$app")
    win=
    # the previous one waited for above: its window is gone (X may hand its id to this one)
    for _ in $(seq 200); do
        win=$(DISPLAY=$display xdotool search --pid "$app" --name '^Dussek$' 2>/dev/null | head -n 1 || true)
        [ -n "$win" ] && break
        sleep 0.1
    done
    [ -n "$win" ] || { cat "$tmp/run/log" >&2; echo "screenshots.sh: no window" >&2; exit 1; }
    sleep 2 # the instrument loads
}
shot() { DISPLAY=$display import -window "$win" "$out/$1.png"; echo "$out/$1.png"; }
# until a region (x y w h at 1x) is drawn in the selection's blue: the felt read off the UI's thread
im=$(command -v magick || echo convert) # ImageMagick 7, or 6 (CI's Ubuntu)
blue() {
    for _ in $(seq 150); do
        d=$(DISPLAY=$display import -window "$win" png:- | $im - -crop "$(($3 * scale))x$(($4 * scale))+$(($1 * scale))+$(($2 * scale))" \
            -format '%[fx:mean.b-mean.r]' info:)
        awk -v d="$d" 'BEGIN { exit !(d > 0.15) }' && return
        sleep 0.2
    done
    echo "screenshots.sh: still waiting at $*" >&2; exit 1
}

export DISPLAY=$display
start grand_a
tab sound; shot grand-sound
tab felt; blue 472 90 56 14; shot grand-felt # the Stiffness chip, selected
tab mics; shot grand-mics
tab sound
click 310 27        # Sustain latch: the pedal held
click 55 612 1      # Keyboard: the strip and its panels
click 258 566 1     # Visualization
for n in 9 16 20 23 25 27 30 34; do key "$n"; done # a chord over five octaves
sleep 0.6
shot grand-visualization

start vibraphone
tab sound; shot vibraphone

# each one a picture, not an empty window
for f in grand-sound grand-felt grand-mics grand-visualization vibraphone; do
    sd=$(identify -format '%[fx:standard_deviation]' "$out/$f.png")
    awk -v s="$sd" 'BEGIN { exit !(s > 0.02) }' || { echo "screenshots.sh: $f.png is (nearly) uniform" >&2; exit 1; }
done
