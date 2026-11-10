#!/bin/bash
# A macOS standalone as an app: DPF builds a plain executable.
#   packaging/macos/make-app.sh <the executable> <version> <Name.app>
# The app's name from its folder (Dussek, Weidinger), its identifier de.ccoors.<executable>; Weidinger
# has no icon of its own yet (Dussek's), and no audio input (no microphone to ask for).
set -euo pipefail
exe=${1:?usage: make-app.sh <executable> <version> <app>}
version=${2:?usage: make-app.sh <executable> <version> <app>}
app=${3:?usage: make-app.sh <executable> <version> <app>}
here=$(cd "$(dirname "$0")" && pwd)
name=$(basename "$app" .app)
bin=$(basename "$exe")
mic=""
[ "$bin" = dussek ] && mic="    <key>NSMicrophoneUsageDescription</key><string>The strings can ring along to the audio input.</string>"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$exe" "$app/Contents/MacOS/$bin"
cp "$here/../icon/dussek.icns" "$app/Contents/Resources/dussek.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>de.ccoors.$bin</string>
    <key>CFBundleExecutable</key><string>$bin</string>
    <key>CFBundleIconFile</key><string>dussek</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>13.3</string>
    <key>NSHighResolutionCapable</key><true/>
$mic
</dict>
</plist>
PLIST
