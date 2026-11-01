#!/bin/bash
# The macOS standalone as an app: DPF builds a plain executable.
#   packaging/macos/make-app.sh <the dussek executable> <version> <Dussek.app>
set -euo pipefail
exe=${1:?usage: make-app.sh <executable> <version> <app>}
version=${2:?usage: make-app.sh <executable> <version> <app>}
app=${3:?usage: make-app.sh <executable> <version> <app>}
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$exe" "$app/Contents/MacOS/dussek"
cp "$here/../icon/dussek.icns" "$app/Contents/Resources/dussek.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Dussek</string>
    <key>CFBundleDisplayName</key><string>Dussek</string>
    <key>CFBundleIdentifier</key><string>de.ccoors.dussek</string>
    <key>CFBundleExecutable</key><string>dussek</string>
    <key>CFBundleIconFile</key><string>dussek</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>13.3</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSMicrophoneUsageDescription</key><string>The strings can ring along to the audio input.</string>
</dict>
</plist>
PLIST
