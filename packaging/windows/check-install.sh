#!/bin/bash
# The Windows installer, checked on a runner (MSYS2 bash, as administrator):
#   packaging/windows/check-install.sh dist/dussek-<version>-windows-setup.exe <portable folder>
# Plugins only first (Dussek's instruments come regardless, Weidinger and the brass not), then everything
# over a version that left an instrument behind; the standalones list the instruments; an update without
# Weidinger removes it and the brass, one without the standalone removes it and its Start menu entry; the
# uninstaller removes the program and keeps the player's own files.
set -eu
export MSYS2_ARG_CONV_EXCL="*"   # /VERYSILENT and the like are options, not paths to convert
setup=${1:?usage: check-install.sh <setup.exe> <portable folder>}
dist=${2:?usage: check-install.sh <setup.exe> <portable folder>}
# MSYS2 keeps some variables in their Windows spelling (ProgramData); Windows' getenv ignores case
var() { printenv "$1" || printenv "$2" || { echo "check-install: $1 unset" >&2; exit 1; }; }
pf=$(cygpath -u "$(var PROGRAMFILES ProgramFiles)")
cf=$(cygpath -u "$(var COMMONPROGRAMFILES CommonProgramFiles)")
pd=$(cygpath -u "$(var PROGRAMDATA ProgramData)")
ad=$(cygpath -u "$(var APPDATA AppData)")
fail() { echo "check-install: $*" >&2; exit 1; }
run() { "$setup" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART "$@" || fail "the installer failed ($*)"; }

# the unpacked archive itself, nothing installed: its standalone plays the instruments beside it
"$dist/dussek.exe" list | grep -q "^harp	" || fail "the archive's standalone does not find its instruments"
"$dist/weidinger.exe" list | grep -q "^trumpet	" || fail "the archive's Weidinger does not find its instruments"

run /COMPONENTS="lv2,clap"
[ -e "$cf/LV2/dussek.lv2" ] && [ -e "$cf/CLAP/dussek.clap" ] || fail "plugins missing"
[ ! -e "$cf/VST3/dussek.vst3" ] && [ ! -e "$pf/Dussek/dussek.exe" ] || fail "an unchosen component was installed"
[ -e "$pd/Dussek/instruments/harp/data.bin" ] || fail "the instruments are missing without the standalone"
for f in "$cf/LV2/weidinger.lv2" "$cf/CLAP/weidinger.clap" "$pd/Dussek/instruments/trumpet"; do
    [ ! -e "$f" ] || fail "Weidinger unchosen, yet installed: $f"
done

# a previous version's instrument, removed since; the player's own preset
mkdir -p "$pd/Dussek/instruments/stale" "$ad/Dussek/presets"
cp "$pd/Dussek/instruments/harp/instrument.json" "$pd/Dussek/instruments/harp/data.bin" "$pd/Dussek/instruments/stale/"
echo "version = 1" > "$ad/Dussek/presets/mine.dussek-preset"
run /COMPONENTS="standalone,lv2,clap,vst3,weidinger"
[ -e "$cf/VST3/dussek.vst3" ] && [ -e "$pf/Dussek/dussek.exe" ] || fail "VST3 or the standalone missing"
for f in "$pf/Dussek/weidinger.exe" "$cf/LV2/weidinger.lv2" "$cf/CLAP/weidinger.clap" "$cf/VST3/weidinger.vst3" \
         "$pd/Dussek/instruments/trumpet/data.bin"; do
    [ -e "$f" ] || fail "Weidinger chosen, yet missing: $f"
done

list=$("$pf/Dussek/dussek.exe" list) || fail "dussek list found no instruments"
brass=$("$pf/Dussek/weidinger.exe" list) || fail "weidinger list found no instruments"
missing=$(printf '%s\n%s\n' "$list" "$brass" | "$(dirname "$0")/../unlisted.sh" "$dist/instruments")
[ -z "$missing" ] || fail "shipped but not listed: $(echo $missing)"
listed=$(printf '%s\n' "$list" | grep -vc '^#' || true)
printf '%s\n' "$list" | grep -q "^stale	" && fail "a removed instrument stayed"
printf '%s\n' "$list" | grep -q "^harp	.*ProgramData" || fail "the instruments do not come from ProgramData"
listed=$((listed + $(printf '%s\n' "$brass" | grep -vc '^#' || true)))

# an update that leaves Weidinger out removes it, its Start menu entry and the brass; Dussek stays
wmenu="$pd/Microsoft/Windows/Start Menu/Programs/Weidinger.lnk"
[ -e "$wmenu" ] || fail "no Start menu entry: $wmenu"
run /COMPONENTS="standalone,lv2,clap,vst3"
for f in "$pf/Dussek/weidinger.exe" "$cf/LV2/weidinger.lv2" "$cf/CLAP/weidinger.clap" "$cf/VST3/weidinger.vst3" \
         "$pd/Dussek/instruments/trumpet" "$wmenu"; do
    [ ! -e "$f" ] || fail "Weidinger left out, yet still there: $f"
done
[ -e "$pf/Dussek/dussek.exe" ] && [ -e "$pd/Dussek/instruments/harp/data.bin" ] || fail "Dussek went with Weidinger"

# an update that leaves the standalone out removes it and its Start menu entry
menu="$pd/Microsoft/Windows/Start Menu/Programs/Dussek.lnk"
[ -e "$menu" ] || fail "no Start menu entry: $menu"
run /COMPONENTS="lv2,clap,vst3"
[ ! -e "$menu" ] && [ ! -e "$pf/Dussek/dussek.exe" ] || fail "the standalone or its Start menu entry stayed"

# the uninstaller hands over to a copy of itself and returns: wait for it to remove itself
"$pf/Dussek/unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
for _ in $(seq 60); do [ -e "$pf/Dussek/unins000.exe" ] || break; sleep 1; done
for f in "$pf/Dussek/dussek.exe" "$cf/LV2/dussek.lv2" "$cf/CLAP/dussek.clap" "$cf/VST3/dussek.vst3" "$pd/Dussek/instruments" \
         "$pf/Dussek/weidinger.exe" "$cf/LV2/weidinger.lv2" "$cf/CLAP/weidinger.clap" "$cf/VST3/weidinger.vst3"; do
    [ ! -e "$f" ] || fail "left after uninstall: $f"
done
[ -e "$ad/Dussek/presets/mine.dussek-preset" ] || fail "uninstall removed the player's preset"
echo "check-install: ok ($listed instruments listed)"
