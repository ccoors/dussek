#!/bin/sh
# Before a release: the version (a tag without its "v") is the code's (plugin/CMakeLists.txt), and
# CHANGELOG.md has its section.   packaging/check-release.sh <version>
set -eu
version=${1:?usage: check-release.sh <version>}
root=$(cd "$(dirname "$0")/.." && pwd)
code=$(sed -n 's/^project(dussek-plugin VERSION \([0-9.]*\).*/\1/p' "$root/plugin/CMakeLists.txt")
[ "$version" = "$code" ] || { echo "check-release.sh: $version is not the code's version ($code)" >&2; exit 1; }
sh "$root/packaging/release-notes.sh" "$version" "$root/CHANGELOG.md" > /dev/null
