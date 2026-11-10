#!/bin/bash
# The macOS package, checked on a runner (sudo without a password):
#   packaging/macos/check-install.sh dist/dussek-<version>-macos.pkg <portable folder>
# Installs, reinstalls over a version that left an instrument behind, lists the instruments
# through the installed apps (Dussek's and Weidinger's), checks the ad hoc signatures.
set -eu
pkg=${1:?usage: check-install.sh <pkg> <portable folder>}
dist=${2:?usage: check-install.sh <pkg> <portable folder>}
lib="/Library/Application Support/Dussek/instruments"
plugins=/Library/Audio/Plug-Ins
fail() { echo "check-install: $*" >&2; exit 1; }

# the unpacked archive itself, nothing installed: its app plays the instruments beside it
"$dist/Dussek.app/Contents/MacOS/dussek" list | grep -q "^harp	" || fail "the archive's app does not find its instruments"
"$dist/Weidinger.app/Contents/MacOS/weidinger" list | grep -q "^trumpet	" || fail "the archive's Weidinger does not find its instruments"

sudo installer -pkg "$pkg" -target / || fail "the first install failed"
sudo mkdir -p "$lib/stale"
sudo cp "$lib/harp/instrument.json" "$lib/harp/data.bin" "$lib/stale/"
sudo installer -pkg "$pkg" -target / || fail "the update failed"

for f in /Applications/Dussek.app "$plugins/LV2/dussek.lv2" "$plugins/CLAP/dussek.clap" "$plugins/VST3/dussek.vst3" \
         /Applications/Weidinger.app "$plugins/LV2/weidinger.lv2" "$plugins/CLAP/weidinger.clap" "$plugins/VST3/weidinger.vst3"; do
    [ -e "$f" ] || fail "missing: $f"
done
# the bundles' signatures; the LV2's (a plain folder) on its libraries
for f in /Applications/Dussek.app "$plugins/CLAP/dussek.clap" "$plugins/VST3/dussek.vst3" \
         /Applications/Weidinger.app "$plugins/CLAP/weidinger.clap" "$plugins/VST3/weidinger.vst3" \
         $(find "$plugins/LV2/dussek.lv2" "$plugins/LV2/weidinger.lv2" -type f \( -name '*.dylib' -o -name '*.so' \)); do
    codesign -v "$f" || fail "no valid signature: $f"
done
list=$(/Applications/Dussek.app/Contents/MacOS/dussek list) || fail "dussek list found no instruments"
brass=$(/Applications/Weidinger.app/Contents/MacOS/weidinger list) || fail "weidinger list found no instruments"
printf '%s\n' "$brass" | grep -q "^trumpet	.*/Library/Application Support/Dussek/" || fail "the brass do not come from /Library"
missing=$(printf '%s\n%s\n' "$list" "$brass" | "$(dirname "$0")/../unlisted.sh" "$dist/instruments")
[ -z "$missing" ] || fail "shipped but not listed: $(echo $missing)"
listed=$(printf '%s\n' "$list" | grep -vc '^#' || true)
printf '%s\n' "$list" | grep -q "^stale	" && fail "a removed instrument stayed"
printf '%s\n' "$list" | grep -q "^harp	.*/Library/Application Support/Dussek/" || fail "the instruments do not come from /Library"
listed=$((listed + $(printf '%s\n' "$brass" | grep -vc '^#' || true)))
echo "check-install: ok ($listed instruments listed)"
