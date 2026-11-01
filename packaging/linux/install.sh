#!/bin/sh
# Dussek on Linux: the standalone, the LV2, CLAP and VST3 plugins and the instruments.
#   ./install.sh            for you: ~/.lv2, ~/.clap, ~/.vst3, ~/.local/bin, ~/.local/share
#   sudo ./install.sh --system   for every user, under /usr/local (CLAP: /usr/lib/clap)
# Run again to update. What it installed is listed in installed.txt beside the instruments, so an
# update and ./uninstall.sh remove that and nothing else (your presets and own instruments stay).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
if [ "${1:-}" = "--system" ]; then
    [ "$(id -u)" -eq 0 ] || { echo "install.sh --system: run it as root (sudo)" >&2; exit 1; }
    lib=/usr/local/lib bin=/usr/local/bin share=/usr/local/share
    # the CLAP spec's system folder is /usr/lib/clap (hosts do not look under /usr/local)
    lv2=$lib/lv2 clap=/usr/lib/clap vst3=$lib/vst3
elif [ -z "${1:-}" ]; then
    bin=$HOME/.local/bin share=${XDG_DATA_HOME:-$HOME/.local/share}
    lv2=$HOME/.lv2 clap=$HOME/.clap vst3=$HOME/.vst3
else
    echo "usage: install.sh [--system]" >&2
    exit 2
fi
data=$share/dussek
manifest=$data/installed.txt

# the previous install's files (an update): removed first, so nothing it shipped lingers
if [ -f "$manifest" ]; then
    while IFS= read -r path; do
        [ -n "$path" ] && rm -rf "$path"
    done < "$manifest"
    rm -f "$manifest"
fi

mkdir -p "$bin" "$lv2" "$clap" "$vst3" "$data/instruments" "$share/applications" "$share/icons/hicolor/256x256/apps"
new=$(mktemp)
put() { # copy $1 to $2, list $2
    cp -R "$1" "$2"
    echo "$2" >> "$new"
}
put "$here/dussek" "$bin/dussek"
put "$here/dussek.lv2" "$lv2/dussek.lv2"
put "$here/dussek.clap" "$clap/dussek.clap"
put "$here/dussek.vst3" "$vst3/dussek.vst3"
for dir in "$here"/instruments/*/; do
    id=$(basename "$dir")
    rm -rf "${data:?}/instruments/$id" # an older copy of a shipped instrument
    put "${dir%/}" "$data/instruments/$id"
done
[ -f "$here/instruments/aliases.txt" ] && put "$here/instruments/aliases.txt" "$data/instruments/aliases.txt"
sed "s|^Exec=.*|Exec=$bin/dussek|" "$here/dussek.desktop" > "$share/applications/dussek.desktop"
echo "$share/applications/dussek.desktop" >> "$new"
put "$here/dussek.png" "$share/icons/hicolor/256x256/apps/dussek.png"
mv "$new" "$manifest"
chmod 644 "$manifest"

echo "Dussek installed: $bin/dussek, the plugins in $lv2, $clap, $vst3, the instruments in $data/instruments."
case ":$PATH:" in *":$bin:"*) ;; *) echo "($bin is not on your PATH: start it from the menu or as $bin/dussek)";; esac
