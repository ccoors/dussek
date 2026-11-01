#!/bin/bash
# The macOS package, checked on a runner (sudo without a password):
#   packaging/macos/check-install.sh dist/dussek-<version>-macos.pkg <portable folder>
# Installs, reinstalls over a version that left an instrument behind, lists the instruments
# through the installed app, checks the ad hoc signatures.
set -eu
pkg=${1:?usage: check-install.sh <pkg> <portable folder>}
dist=${2:?usage: check-install.sh <pkg> <portable folder>}
lib="/Library/Application Support/Dussek/instruments"
plugins=/Library/Audio/Plug-Ins
fail() { echo "check-install: $*" >&2; exit 1; }

# the unpacked archive itself, nothing installed: its app plays the instruments beside it
"$dist/Dussek.app/Contents/MacOS/dussek" list | grep -q "^harp	" || fail "the archive's app does not find its instruments"

sudo installer -pkg "$pkg" -target / || fail "the first install failed"
sudo mkdir -p "$lib/stale"
sudo cp "$lib/harp/instrument.json" "$lib/harp/data.bin" "$lib/stale/"
sudo installer -pkg "$pkg" -target / || fail "the update failed"

for f in /Applications/Dussek.app "$plugins/LV2/dussek.lv2" "$plugins/CLAP/dussek.clap" "$plugins/VST3/dussek.vst3"; do
    [ -e "$f" ] || fail "missing: $f"
done
# the bundles' signatures; the LV2's (a plain folder) on its libraries
for f in /Applications/Dussek.app "$plugins/CLAP/dussek.clap" "$plugins/VST3/dussek.vst3" \
         $(find "$plugins/LV2/dussek.lv2" -type f \( -name '*.dylib' -o -name '*.so' \)); do
    codesign -v "$f" || fail "no valid signature: $f"
done
list=$(/Applications/Dussek.app/Contents/MacOS/dussek list) || fail "dussek list found no instruments"
shipped=$(find "$dist/instruments" -mindepth 1 -maxdepth 1 -type d | wc -l)
listed=$(printf '%s\n' "$list" | grep -vc '^#' || true)
[ "$listed" -ge "$((shipped - 1))" ] || fail "$listed instruments listed, $shipped shipped"
printf '%s\n' "$list" | grep -q "^stale	" && fail "a removed instrument stayed"
printf '%s\n' "$list" | grep -q "^harp	.*/Library/Application Support/Dussek/" || fail "the instruments do not come from /Library"
echo "check-install: ok ($listed instruments listed)"
