#!/bin/bash
# Dussek's macOS package, Weidinger in it as a choice of its own, from the portable folder (plugin/tools/package.sh macos):
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

# the apps (package.sh made them: DPF builds plain executables), Dussek's in app/, lv2/, …, Weidinger's in
# weidinger-app/, weidinger-lv2/, …
for p in dussek weidinger; do
    pre=$([ $p = weidinger ] && echo weidinger- || true)
    app=$([ $p = weidinger ] && echo Weidinger.app || echo Dussek.app)
    mkdir -p "$work/${pre}app"
    cp -R "$src/$app" "$work/${pre}app/$app"
    for b in lv2 clap vst3; do
        mkdir -p "$work/$pre$b"
        cp -R "$src/$p.$b" "$work/$pre$b/"
    done
    for b in "$work/${pre}app/$app" "$work/${pre}clap/$p.clap" "$work/${pre}vst3/$p.vst3"; do
        codesign --force --deep -s - "$b"
    done
    # an LV2 bundle is a plain folder, not a macOS bundle: its libraries are signed one by one
    find "$work/${pre}lv2/$p.lv2" -type f \( -name '*.dylib' -o -name '*.so' \) -exec codesign --force -s - {} \;
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
# the brass with Weidinger only: installed after the instruments' package, whose preinstall empties the folder
"$here/../split-brass.sh" "$work/instruments" "$work/brass"
component app "$work/app" /Applications
component lv2 "$work/lv2" /Library/Audio/Plug-Ins/LV2
component clap "$work/clap" /Library/Audio/Plug-Ins/CLAP
component vst3 "$work/vst3" /Library/Audio/Plug-Ins/VST3
component instruments "$work/instruments" "/Library/Application Support/Dussek/instruments" "$here/scripts"
component weidinger-app "$work/weidinger-app" /Applications
component weidinger-lv2 "$work/weidinger-lv2" /Library/Audio/Plug-Ins/LV2
component weidinger-clap "$work/weidinger-clap" /Library/Audio/Plug-Ins/CLAP
component weidinger-vst3 "$work/weidinger-vst3" /Library/Audio/Plug-Ins/VST3
component brass "$work/brass" "/Library/Application Support/Dussek/instruments"

mkdir -p "$(dirname "$out")"
productbuild --distribution "$here/distribution.xml" --package-path "$work/pkgs" \
    --resources "$here/resources" --version "$version" "$out"
echo "$out"
