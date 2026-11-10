#!/bin/sh
# Dussek and Weidinger (the brass, a preview) on Linux: each its standalone, its LV2, CLAP and VST3 plugins;
# the instruments (each plugin lists its own).
#   ./install.sh            for you: ~/.lv2, ~/.clap, ~/.vst3, ~/.local/bin, ~/.local/share
#   sudo ./install.sh --system   for every user, under /usr/local (CLAP: /usr/lib/clap)
# Run again to update. What it installed is listed in installed.txt beside the instruments (each
# path before its copy: a failed install leaves nothing unlisted), so an update and ./uninstall.sh
# remove that and nothing else (your presets and own instruments stay).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
if [ "${1:-}" = "--system" ]; then
    [ "$(id -u)" -eq 0 ] || { echo "install.sh --system: run it as root (sudo)" >&2; exit 1; }
    lib=/usr/local/lib bin=/usr/local/bin share=/usr/local/share
    # the CLAP spec's system folder is /usr/lib/clap (hosts do not look under /usr/local)
    lv2=$lib/lv2 clap=/usr/lib/clap vst3=$lib/vst3
elif [ -z "${1:-}" ]; then
    # the XDG spec: a relative XDG_DATA_HOME is invalid and ignored
    case "${XDG_DATA_HOME:-}" in /*) share=$XDG_DATA_HOME ;; *) share=$HOME/.local/share ;; esac
    bin=$HOME/.local/bin
    lv2=$HOME/.lv2 clap=$HOME/.clap vst3=$HOME/.vst3
else
    echo "usage: install.sh [--system]" >&2
    exit 2
fi
data=$share/dussek
manifest=$data/installed.txt

# `just install` links these to a clone (its instruments, its builds): writing through the links would
# change the clone, so a release install waits until they are gone (`just uninstall`)
for t in "$data/instruments" "$bin/dussek" "$lv2/dussek.lv2" "$clap/dussek.clap" "$vst3/dussek.vst3" \
         "$bin/weidinger" "$lv2/weidinger.lv2" "$clap/weidinger.clap" "$vst3/weidinger.vst3"; do
    if [ -L "$t" ]; then
        echo "install.sh: $t is a link (from \`just install\`?); remove the links first (\`just uninstall\`)" >&2
        exit 1
    fi
done

# the previous install's files (an update): removed first, so nothing it shipped lingers
if [ -f "$manifest" ]; then
    while IFS= read -r path; do
        [ -n "$path" ] && rm -rf "$path"
    done < "$manifest"
fi

mkdir -p "$bin" "$lv2" "$clap" "$vst3" "$data/instruments" "$share/applications" "$share/icons/hicolor/256x256/apps"
: > "$manifest"
chmod 644 "$manifest"
put() { # list $2, then copy $1 to $2 (replacing what is there: a test build's bundle copied by hand)
    echo "$2" >> "$manifest"
    rm -rf "$2"
    cp -R "$1" "$2"
}
for p in dussek weidinger; do
    put "$here/$p" "$bin/$p"
    put "$here/$p.lv2" "$lv2/$p.lv2"
    put "$here/$p.clap" "$clap/$p.clap"
    put "$here/$p.vst3" "$vst3/$p.vst3"
done
for dir in "$here"/instruments/*/; do
    id=$(basename "$dir")
    put "${dir%/}" "$data/instruments/$id"
done
[ -f "$here/instruments/aliases.txt" ] && put "$here/instruments/aliases.txt" "$data/instruments/aliases.txt"
for p in dussek weidinger; do
    echo "$share/applications/$p.desktop" >> "$manifest"
    sed "s|^Exec=.*|Exec=$bin/$p|" "$here/$p.desktop" > "$share/applications/$p.desktop"
done
put "$here/dussek.png" "$share/icons/hicolor/256x256/apps/dussek.png"

echo "Dussek and Weidinger installed: $bin/dussek and $bin/weidinger, the plugins in $lv2, $clap, $vst3, the instruments in $data/instruments."
case ":$PATH:" in *":$bin:"*) ;; *) echo "($bin is not on your PATH: start them from the menu or as $bin/dussek, $bin/weidinger)";; esac
