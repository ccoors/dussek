#!/usr/bin/env bash
# The builds and the instruments in one folder to try on another machine (CI's artifacts):
#   plugin/tools/package.sh <platform>   ->  dist/dussek-<platform>/
# The standalone, the LV2, CLAP and VST3 bundles as built, the instruments' committed files, the
# licenses and TRY-IT.txt. Run from the repository's root after `just plugin`.
set -euo pipefail
platform=${1:?usage: package.sh linux|macos|windows}
bin=plugin/build/bin
out=dist/dussek-$platform
rm -rf "$out"
mkdir -p "$out/instruments"
for f in dussek dussek.exe dussek.app dussek.lv2 dussek.clap dussek.vst3; do
    [ -e "$bin/$f" ] && cp -R "$bin/$f" "$out/"
done
# the exports as committed (no caches, nothing untracked)
git ls-files -z instruments | while IFS= read -r -d '' f; do
    mkdir -p "$out/$(dirname "$f")"
    cp "$f" "$out/$f"
done
cp LICENSE "$out/LICENSE.txt"
commit=$(git describe --always --dirty 2>/dev/null || echo unknown)
case $platform in
    linux)
        plugins="~/.lv2, ~/.clap, ~/.vst3"
        data="~/.local/share/dussek"
        run="./dussek (the standalone needs JACK, or PipeWire's JACK: pw-jack ./dussek)"
        note="Built on Ubuntu 24.04: a Linux with glibc 2.39 or newer." ;;
    macos)
        plugins="~/Library/Audio/Plug-Ins/LV2, …/CLAP, …/VST3"
        data="~/Library/Application Support/Dussek"
        run="dussek.app. It is not signed: the first time, right-click it and choose Open, or run
  xattr -dr com.apple.quarantine dussek.app dussek.lv2 dussek.clap dussek.vst3"
        note="Built for Apple silicon (arm64), macOS 13.3 or newer; not for Intel Macs." ;;
    windows)
        plugins="C:\\Program Files\\Common Files\\VST3 (dussek.vst3), C:\\Program Files\\Common Files\\CLAP (dussek.clap), %APPDATA%\\LV2 (dussek.lv2)"
        data="%APPDATA%\\Dussek"
        run="dussek.exe (the standalone plays through the system's audio device; started from a
  console, it first says \"Failed to load JACK DLL\": expected without JACK, it then uses the system's)"
        note="Built with MinGW (UCRT): Windows 10 or newer, 64 bit. Windows may warn about an unknown publisher." ;;
esac
cat > "$out/TRY-IT.txt" <<TXT
Dussek, a test build from commit $commit ($platform). Not a release.

1. Copy the folder "instruments" into $data
   (make the folder if needed); the plugin and the standalone find the instruments there.
2. The standalone: $run
3. The plugins: copy the bundles you want to $plugins.
   LV2 and the standalone are the tested targets; CLAP and VST3 are built but untested.

$note
The code is AGPL-3.0-or-later (LICENSE.txt); each instrument's license is in its folder.
TXT
echo "$out"
