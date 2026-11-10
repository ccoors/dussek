#!/bin/sh
# A release's text: the version's section of CHANGELOG.md (its "## <version> — <date>" heading
# left out: the release's title carries it), then what every release says.
#   packaging/release-notes.sh <version> [CHANGELOG.md]
# Exit 1 without a section for the version (the release workflow then publishes nothing).
set -eu
version=${1:?usage: release-notes.sh <version> [changelog]}
log=${2:-$(dirname "$0")/../CHANGELOG.md}
section=$(awk -v v="$version" '
    /^## / { on = ($2 == v); next }
    on { print }
' "$log" | sed -e '/./,$!d')
[ -n "$section" ] || { echo "release-notes.sh: no section for $version in $log" >&2; exit 1; }
printf '%s\n' "$section" | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}'
cat <<TXT

---

**Install:** Windows: \`dussek-$version-windows-setup.exe\`; macOS (Apple silicon):
\`dussek-$version-macos.pkg\`; Linux: unpack \`dussek-$version-linux.tar.gz\` and run \`./install.sh\`.
\`dussek-$version-windows-portable.zip\` and \`dussek-$version-macos-portable.tar.gz\` hold the same
files to copy by hand (their \`README.txt\` says where). If you tried a test build, remove the
bundles you copied by hand first (\`dussek.lv2\`, \`.clap\`, \`.vst3\` in \`%APPDATA%\\LV2\` or
\`~/Library/Audio/Plug-Ins\`): hosts prefer them to the installed ones.

**Weidinger**, the brass (trumpet, horn, tenor and bass trombone, tuba), comes in the same files as a
preview: its own standalone and plugins, physically modeled from the same sources. The installers
offer it as a component that can be left out; its sound and controls may still change.

**Tested** on Linux and Windows (the standalone, LV2, CLAP and VST3); the macOS build is untested,
reports welcome. Where a host offers LV2 or CLAP, prefer them to the VST3.

**Not signed:** Windows' SmartScreen warns about an unknown publisher ("More info", then "Run
anyway"); macOS asks before opening the package (right-click it and choose Open, or System
Settings → Privacy & Security → "Open Anyway").

The code is AGPL-3.0-or-later; each instrument carries its license (CC BY 4.0, or CC BY-SA 4.0
where its folder's \`LICENSE.md\` says so).
TXT
