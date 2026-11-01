#!/bin/sh
# A release's five files to itch.io (packaging/itch/README.md), one channel each: itch names the
# platform from the channel ("windows", "mac", "linux") and serves a single file as it is; the
# portable zip butler unpacks (the itch app installs it as a folder, the website zips it again).
#   packaging/itch.sh <version> <user/game> [folder]     # folder: the files (default dist)
# BUTLER: the butler to run (default butler; its key from `butler login` or BUTLER_API_KEY).
# Every file is checked first: a missing one pushes nothing.
set -eu
version=${1:?usage: itch.sh <version> <user/game> [folder]}
target=${2:?usage: itch.sh <version> <user/game> [folder]}
dir=${3:-dist}
butler=${BUTLER:-butler}
case $target in
    */*) ;;
    *) echo "itch.sh: the target is <user>/<game>, not $target" >&2; exit 1 ;;
esac
channels="windows-setup:windows-setup.exe windows-portable:windows-portable.zip mac-pkg:macos.pkg mac-portable:macos-portable.tar.gz linux:linux.tar.gz"
for c in $channels; do
    f="$dir/dussek-$version-${c#*:}"
    [ -s "$f" ] || { echo "itch.sh: missing $f" >&2; exit 1; }
done
for c in $channels; do
    "$butler" push "$dir/dussek-$version-${c#*:}" "$target:${c%%:*}" --userversion "$version"
done
