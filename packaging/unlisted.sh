#!/bin/sh
# The shipped instruments a `dussek list` leaves out (the install checks):
#   dussek list | packaging/unlisted.sh <the portable folder's instruments>
# prints each id, one per line; hidden instruments (the menu's "hidden") are not expected.
set -eu
dir=${1:?usage: unlisted.sh <instruments folder> < list}
listed=$(grep -v '^#' | cut -f1)
for d in "$dir"/*/; do
    id=$(basename "$d")
    grep -q '"hidden": *true' "$d/instrument.json" 2>/dev/null && continue
    printf '%s\n' "$listed" | grep -qx "$id" || echo "$id"
done
