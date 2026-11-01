#!/bin/sh
# The Linux install, checked in a throwaway home (CI's release job; locally after package.sh):
#   packaging/linux/check-install.sh dist/dussek-<version>-linux
# Refuses `just install`'s links, lists a failed install's copies, installs, reinstalls over a version
# that shipped an instrument since removed, lists the instruments through the installed standalone,
# uninstalls; the player's own files stay.
set -eu
dist=$(cd "${1:?usage: check-install.sh <dist folder>}" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export HOME="$tmp/home" XDG_DATA_HOME="$tmp/home/.local/share" XDG_CONFIG_HOME="$tmp/home/.config"
export XDG_DATA_DIRS="$tmp/no-system"
unset DUSSEK_INSTRUMENTS
data="$XDG_DATA_HOME/dussek"
fail() { echo "check-install: $*" >&2; exit 1; }

# the unpacked archive itself, nothing installed: its standalone plays the instruments beside it
"$dist/dussek" list | grep -q "^harp	.*$dist/instruments/harp" || fail "the archive's standalone does not find its instruments"

# `just install`'s links (a clone's instruments and builds): refused, nothing written through them
clone="$tmp/clone"
mkdir -p "$clone/instruments/harp" "$clone/dussek.lv2" "$data" "$HOME/.lv2"
echo kept > "$clone/instruments/harp/marker"
echo kept > "$clone/dussek.lv2/marker"
ln -s "$clone/instruments" "$data/instruments"
ln -s "$clone/dussek.lv2" "$HOME/.lv2/dussek.lv2"
"$dist/install.sh" 2>/dev/null && fail "install.sh wrote through just install's links"
[ -f "$clone/instruments/harp/marker" ] && [ -f "$clone/dussek.lv2/marker" ] || fail "install.sh changed the linked clone"
rm "$data/instruments" "$HOME/.lv2/dussek.lv2"

# a failed install (a folder it cannot write; root writes anyway): what it copied is listed, so
# uninstall.sh removes it
if [ "$(id -u)" -ne 0 ]; then
    mkdir -p "$HOME/.vst3" && chmod 555 "$HOME/.vst3"
    "$dist/install.sh" >/dev/null 2>&1 && fail "install.sh went through a folder it cannot write"
    chmod 755 "$HOME/.vst3"
    grep -qx "$HOME/.lv2/dussek.lv2" "$data/installed.txt" || fail "a failed install left its copies unlisted"
    "$dist/uninstall.sh" >/dev/null
    [ ! -e "$HOME/.lv2/dussek.lv2" ] && [ ! -e "$HOME/.local/bin/dussek" ] || fail "uninstall.sh left a failed install's copies"
fi

# a test build's bundles copied by hand (no manifest): replaced, not nested
mkdir -p "$HOME/.lv2/dussek.lv2" "$HOME/.vst3/dussek.vst3"
echo old > "$HOME/.lv2/dussek.lv2/old"
echo old > "$HOME/.vst3/dussek.vst3/old"

# a relative XDG_DATA_HOME: invalid (the XDG spec), the default instead
(cd "$tmp" && XDG_DATA_HOME=relative "$dist/install.sh")
[ ! -e "$tmp/relative" ] && [ -f "$data/installed.txt" ] || fail "install.sh used a relative XDG_DATA_HOME"
for b in "$HOME/.lv2/dussek.lv2" "$HOME/.vst3/dussek.vst3"; do
    [ ! -e "$b/old" ] && [ ! -e "$b/$(basename "$b")" ] || fail "an old bundle kept or nested: $b"
done
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
missing=$(printf '%s\n' "$list" | "$(dirname "$0")/../unlisted.sh" "$dist/instruments")
[ -z "$missing" ] || fail "shipped but not listed: $(echo $missing)"
listed=$(printf '%s\n' "$list" | grep -vc '^#' || true)
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
