#!/bin/sh
# Weidinger's instruments apart from Dussek's, for an installer that installs them with Weidinger only:
#   packaging/split-brass.sh <instruments folder> <brass folder>
# moves each instrument whose export has a brass block (the key `"brass": {` at the top level, as
# export_brass.py writes it; a mallet named "brass" is a value) from the first folder into the second.
set -eu
from=${1:?usage: split-brass.sh <instruments folder> <brass folder>}
to=${2:?usage: split-brass.sh <instruments folder> <brass folder>}
mkdir -p "$to"
for d in "$from"/*/; do
    [ -f "$d/instrument.json" ] && grep -q '"brass": {' "$d/instrument.json" || continue
    mv "${d%/}" "$to/"
done
