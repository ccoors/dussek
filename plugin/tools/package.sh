#!/usr/bin/env bash
# The builds and the instruments in one folder (the release's portable archive; the installers
# are made from it):
#   plugin/tools/package.sh <platform>   ->  dist/dussek-<version>-<platform>/ (its path printed last)
# The standalone, the LV2, CLAP and VST3 bundles as built, the instruments' committed files, the
# license and README.txt; on Linux also install.sh, uninstall.sh, the desktop entry and the icon.
# Run from the repository's root after `just plugin`.
set -euo pipefail
platform=${1:?usage: package.sh linux|macos|windows}
version=$(sed -n 's/^project(dussek-plugin VERSION \([0-9.]*\).*/\1/p' plugin/CMakeLists.txt)
[ -n "$version" ] || { echo "package.sh: no version in plugin/CMakeLists.txt" >&2; exit 1; }
bin=plugin/build/bin
out=dist/dussek-$version-$platform
rm -rf "$out"
mkdir -p "$out/instruments"
for f in dussek dussek.exe dussek.lv2 dussek.clap dussek.vst3; do
    [ -e "$bin/$f" ] && cp -R "$bin/$f" "$out/"
done
# macOS: the standalone as an app (DPF builds a plain executable)
if [ "$platform" = macos ]; then
    packaging/macos/make-app.sh "$out/dussek" "$version" "$out/Dussek.app"
    rm "$out/dussek"
fi
# the exports as committed (no caches, nothing untracked)
git ls-files -z instruments | while IFS= read -r -d '' f; do
    mkdir -p "$out/$(dirname "$f")"
    cp "$f" "$out/$f"
done
cp LICENSE "$out/LICENSE.txt"
commit=$(git describe --always --dirty 2>/dev/null || echo unknown)
tested="The standalone, LV2, CLAP and VST3 are tested on Linux and Windows; the macOS build is untested
(reports welcome). Where a host offers LV2 or CLAP, prefer them: the VST3 has limits (no presets in
the host's menu, no diagnostics panels in some hosts)."
case $platform in
    linux)
        cp packaging/linux/install.sh packaging/linux/uninstall.sh packaging/linux/dussek.desktop "$out/"
        cp packaging/icon/dussek.png "$out/dussek.png"
        glibc=$(ldd --version 2>/dev/null | sed -n '1s/.* \([0-9][0-9.]*\)$/\1/p')
        glibc=${glibc:-unknown}
        how="Install: ./install.sh (for you: ~/.lv2, ~/.clap, ~/.vst3, ~/.local/bin, the instruments in
~/.local/share/dussek), or sudo ./install.sh --system (for every user, under /usr/local; CLAP in /usr/lib/clap).
Run it again to update; ./uninstall.sh removes it (your presets and own instruments stay).
The standalone plays through JACK when a JACK server runs (PipeWire's too), else through the
system's audio.
Built with glibc $glibc: a Linux with glibc $glibc or newer." ;;
    macos)
        how="Install: the release's dussek-$version-macos.pkg does all of this. By hand: copy the folder
\"instruments\" into ~/Library/Application Support/Dussek, Dussek.app into Applications, the
bundles into ~/Library/Audio/Plug-Ins/LV2, …/CLAP, …/VST3. Not signed: the first time,
right-click the app and choose Open, or run
  xattr -dr com.apple.quarantine Dussek.app dussek.lv2 dussek.clap dussek.vst3
Built for Apple silicon (arm64), macOS 13.3 or newer; not for Intel Macs." ;;
    windows)
        how="Install: the release's dussek-$version-windows-setup.exe does all of this. By hand: copy the
folder \"instruments\" into %APPDATA%\\Dussek, dussek.vst3 into C:\\Program Files\\Common Files\\VST3,
dussek.clap into C:\\Program Files\\Common Files\\CLAP, dussek.lv2 into %APPDATA%\\LV2.
Started from a console, the standalone first says \"Failed to load JACK DLL\": expected without
JACK, it then uses the system's audio.
Built with MinGW (UCRT): Windows 10 or newer, 64 bit. Not signed: Windows may warn about an
unknown publisher (\"More info\", then \"Run anyway\")." ;;
    *)
        echo "package.sh: unknown platform $platform" >&2; exit 2 ;;
esac
cat > "$out/README.txt" <<TXT
Dussek $version ($platform, commit $commit): a physically modeled piano, harpsichords, harps and
percussion, as a standalone and LV2, CLAP and VST3 plugins. https://github.com/ccoors/dussek

$how

$tested
The code is AGPL-3.0-or-later (LICENSE.txt); each instrument's license is in its folder.
TXT
echo "$out"
