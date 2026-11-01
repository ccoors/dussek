#!/bin/sh
# The Linux install, checked in a throwaway home (CI's release job; locally after package.sh):
#   packaging/linux/check-install.sh dist/dussek-<version>-linux
# Installs, reinstalls over a version that shipped an instrument since removed, lists the
# instruments through the installed standalone, uninstalls; the player's own files stay.
set -eu
dist=$(cd "${1:?usage: check-install.sh <dist folder>}" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export HOME="$tmp/home" XDG_DATA_HOME="$tmp/home/.local/share" XDG_CONFIG_HOME="$tmp/home/.config"
export XDG_DATA_DIRS="$tmp/no-system"
unset DUSSEK_INSTRUMENTS
data="$XDG_DATA_HOME/dussek"
fail() { echo "check-install: $*" >&2; exit 1; }

"$dist/install.sh"
# a previous version's instrument, removed since: listed as installed, present on disk
mkdir -p "$data/instruments/stale"
cp "$data/instruments/harp/instrument.json" "$data/instruments/harp/data.bin" "$data/instruments/stale/"
echo "$data/instruments/stale" >> "$data/installed.txt"
# the player's own: an instrument (the editor's export), a preset
mkdir -p "$data/instruments/mine" "$data/presets"
cp "$data/instruments/harp/instrument.json" "$data/instruments/harp/data.bin" "$data/instruments/mine/"
echo "version = 1" > "$data/presets/mine.dussek-preset"
"$dist/install.sh"

list=$("$HOME/.local/bin/dussek" list) || fail "dussek list found no instruments"
shipped=$(find "$dist/instruments" -mindepth 1 -maxdepth 1 -type d | wc -l)
listed=$(printf '%s\n' "$list" | grep -vc '^#' || true)
# the menu leaves hidden instruments out (one: the registers fixture); "mine" comes on top
[ "$listed" -ge "$((shipped - 1))" ] || fail "$listed instruments listed, $shipped shipped"
printf '%s\n' "$list" | grep -q '^stale	' && fail "a removed instrument stayed"
printf '%s\n' "$list" | grep -q '^mine	' || fail "the player's instrument is gone"
for f in "$HOME/.lv2/dussek.lv2" "$HOME/.clap/dussek.clap" "$HOME/.vst3/dussek.vst3" \
         "$XDG_DATA_HOME/applications/dussek.desktop" "$XDG_DATA_HOME/icons/hicolor/256x256/apps/dussek.png"; do
    [ -e "$f" ] || fail "missing after install: $f"
done
grep -q "^Exec=$HOME/.local/bin/dussek" "$XDG_DATA_HOME/applications/dussek.desktop" || fail "the desktop entry's Exec"

"$dist/uninstall.sh"
for f in "$HOME/.local/bin/dussek" "$HOME/.lv2/dussek.lv2" "$HOME/.clap/dussek.clap" "$HOME/.vst3/dussek.vst3" \
         "$data/instruments/harp" "$data/installed.txt" "$XDG_DATA_HOME/applications/dussek.desktop"; do
    [ ! -e "$f" ] || fail "left after uninstall: $f"
done
[ -e "$data/instruments/mine/instrument.json" ] || fail "uninstall removed the player's instrument"
[ -e "$data/presets/mine.dussek-preset" ] || fail "uninstall removed the player's preset"
echo "check-install: ok ($listed instruments listed)"
