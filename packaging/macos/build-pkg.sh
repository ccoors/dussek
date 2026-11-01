#!/bin/bash
# Dussek's macOS package from the portable folder (plugin/tools/package.sh macos):
#   packaging/macos/build-pkg.sh <version> <dist/dussek-<version>-macos>  ->  dist/dussek-<version>-macos.pkg
# Each bundle signed ad hoc (Apple silicon runs nothing unsigned), one component package each, then
# the product with the choices of distribution.xml. Bundles are not relocatable: the installer puts
# them where it says, not where another copy with the same identifier happens to be.
set -euo pipefail
version=${1:?usage: build-pkg.sh <version> <portable folder>}
src=$(cd "${2:?usage: build-pkg.sh <version> <portable folder>}" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
out=$(pwd)/dist/dussek-$version-macos.pkg
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# the app, named for the Finder, with its icon
mkdir -p "$work/app"
cp -R "$src/dussek.app" "$work/app/Dussek.app"
cp "$here/../icon/dussek.icns" "$work/app/Dussek.app/Contents/Resources/dussek.icns"
plutil -replace CFBundleIconFile -string dussek "$work/app/Dussek.app/Contents/Info.plist"
for b in lv2 clap vst3; do
    mkdir -p "$work/$b"
    cp -R "$src/dussek.$b" "$work/$b/"
done
for b in "$work/app/Dussek.app" "$work/lv2/dussek.lv2" "$work/clap/dussek.clap" "$work/vst3/dussek.vst3"; do
    codesign --force --deep -s - "$b"
done

component() { # <name> <root> <install location> [scripts]
    local plist="$work/$1.plist"
    pkgbuild --analyze --root "$2" "$plist" >/dev/null
    local i=0
    while plutil -extract "$i" raw "$plist" >/dev/null 2>&1; do
        plutil -replace "$i.BundleIsRelocatable" -bool false "$plist"
        i=$((i + 1))
    done
    pkgbuild --root "$2" --component-plist "$plist" --install-location "$3" \
        --identifier "de.ccoors.dussek.$1" --version "$version" ${4:+--scripts "$4"} "$work/pkgs/$1.pkg"
}
mkdir -p "$work/pkgs" "$work/instruments"
cp -R "$src/instruments/." "$work/instruments/"
component app "$work/app" /Applications
component lv2 "$work/lv2" /Library/Audio/Plug-Ins/LV2
component clap "$work/clap" /Library/Audio/Plug-Ins/CLAP
component vst3 "$work/vst3" /Library/Audio/Plug-Ins/VST3
component instruments "$work/instruments" "/Library/Application Support/Dussek/instruments" "$here/scripts"

mkdir -p "$(dirname "$out")"
productbuild --distribution "$here/distribution.xml" --package-path "$work/pkgs" \
    --resources "$here/resources" --version "$version" "$out"
echo "$out"
