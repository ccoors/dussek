#!/bin/sh
# Removes what install.sh installed (its installed.txt); your presets, MIDI maps and own
# instruments stay.   ./uninstall.sh   or   sudo ./uninstall.sh --system
set -eu
if [ "${1:-}" = "--system" ]; then
    data=/usr/local/share/dussek
elif [ -z "${1:-}" ]; then
    # the XDG spec: a relative XDG_DATA_HOME is invalid and ignored
    case "${XDG_DATA_HOME:-}" in /*) data=$XDG_DATA_HOME/dussek ;; *) data=$HOME/.local/share/dussek ;; esac
else
    echo "usage: uninstall.sh [--system]" >&2
    exit 2
fi
manifest=$data/installed.txt
[ -f "$manifest" ] || { echo "uninstall.sh: nothing installed here ($manifest missing)" >&2; exit 1; }
while IFS= read -r path; do
    [ -n "$path" ] && rm -rf "$path"
done < "$manifest"
rm -f "$manifest"
echo "Dussek removed."
