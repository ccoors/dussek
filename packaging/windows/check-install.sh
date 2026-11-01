#!/bin/bash
# The Windows installer, checked on a runner (MSYS2 bash, as administrator):
#   packaging/windows/check-install.sh dist/dussek-<version>-windows-setup.exe <portable folder>
# Plugins only first (the instruments come regardless), then everything over a version that left an
# instrument behind; the standalone lists the instruments; the uninstaller removes the program and
# keeps the player's own files.
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

run /COMPONENTS="lv2,clap"
[ -e "$cf/LV2/dussek.lv2" ] && [ -e "$cf/CLAP/dussek.clap" ] || fail "plugins missing"
[ ! -e "$cf/VST3/dussek.vst3" ] && [ ! -e "$pf/Dussek/dussek.exe" ] || fail "an unchosen component was installed"
[ -e "$pd/Dussek/instruments/harp/data.bin" ] || fail "the instruments are missing without the standalone"

# a previous version's instrument, removed since; the player's own preset
mkdir -p "$pd/Dussek/instruments/stale" "$ad/Dussek/presets"
cp "$pd/Dussek/instruments/harp/instrument.json" "$pd/Dussek/instruments/harp/data.bin" "$pd/Dussek/instruments/stale/"
echo "version = 1" > "$ad/Dussek/presets/mine.dussek-preset"
run /COMPONENTS="standalone,lv2,clap,vst3"
[ -e "$cf/VST3/dussek.vst3" ] && [ -e "$pf/Dussek/dussek.exe" ] || fail "VST3 or the standalone missing"

list=$("$pf/Dussek/dussek.exe" list) || fail "dussek list found no instruments"
shipped=$(find "$dist/instruments" -mindepth 1 -maxdepth 1 -type d | wc -l)
listed=$(printf '%s\n' "$list" | grep -vc '^#' || true)
[ "$listed" -ge "$((shipped - 1))" ] || fail "$listed instruments listed, $shipped shipped"
printf '%s\n' "$list" | grep -q "^stale	" && fail "a removed instrument stayed"
printf '%s\n' "$list" | grep -q "^harp	.*ProgramData" || fail "the instruments do not come from ProgramData"

# the uninstaller hands over to a copy of itself and returns: wait for the program folder to go
"$pf/Dussek/unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
for _ in $(seq 60); do [ -e "$pf/Dussek/dussek.exe" ] || break; sleep 1; done
for f in "$pf/Dussek/dussek.exe" "$cf/LV2/dussek.lv2" "$cf/CLAP/dussek.clap" "$cf/VST3/dussek.vst3" "$pd/Dussek/instruments"; do
    [ ! -e "$f" ] || fail "left after uninstall: $f"
done
[ -e "$ad/Dussek/presets/mine.dussek-preset" ] || fail "uninstall removed the player's preset"
echo "check-install: ok ($listed instruments listed)"
