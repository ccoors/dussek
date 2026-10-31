#!/usr/bin/env bash
# Runs the JACK standalone under PipeWire as a desktop app would: its outputs linked to the
# default sink, and an ALSA MIDI port ("PianoEngine MIDI", through a2jmidi_bridge) for programs
# that play to ALSA MIDI devices (aplaymidi, ALSA-only players). Ends the bridge when the
# standalone closes.
#   plugin/tools/run-standalone.sh [path/to/pianoengine]
# Needs pw-jack, pw-link, pw-dump, jq, pactl and a2jmidi_bridge (a2jmidid).
set -u

here=$(cd "$(dirname "$0")" && pwd)
app=${1:-$here/../build/bin/pianoengine}
bridge_name="PianoEngine MIDI"

for tool in pw-jack pw-link pw-dump jq pactl a2jmidi_bridge; do
    command -v "$tool" >/dev/null || { echo "run-standalone: $tool not found" >&2; exit 1; }
done
[ -x "$app" ] || { echo "run-standalone: no standalone at $app (build the plugin first: just plugin)" >&2; exit 1; }

# the PipeWire id of port `name` of the process `pid` (its client's node's); waits up to 10 s. By
# process, not by name: PipeWire's JACK layer lets two clients share a name ("PianoEngine" twice).
port_of() {
    local pid=$1 name=$2 id
    for _ in $(seq 100); do
        id=$(pw-dump | jq -r --argjson pid "$pid" --arg name "$name" '
            . as $all
            | [$all[] | select(.type == "PipeWire:Interface:Client" and .info.props["application.process.id"] == $pid) | .id] as $clients
            | [$all[] | select(.type == "PipeWire:Interface:Node" and (.info.props["client.id"] as $c | $clients | index($c))) | .id] as $nodes
            | $all[] | select(.type == "PipeWire:Interface:Port" and (.info.props["node.id"] as $n | $nodes | index($n))
                              and .info.props["port.name"] == $name) | .id' | head -1)
        [ -n "$id" ] && { echo "$id"; return 0; }
        sleep 0.1
    done
    return 1
}

pids=()
cleanup() { [ ${#pids[@]} -gt 0 ] && kill "${pids[@]}" 2>/dev/null; }
trap cleanup EXIT INT TERM

pw-jack "$app" &
app_pid=$!
pids+=("$app_pid")
out1=$(port_of "$app_pid" audio_out_1) && out2=$(port_of "$app_pid" audio_out_2) && events=$(port_of "$app_pid" events-in) \
    || { echo "run-standalone: the standalone's ports did not appear" >&2; exit 1; }

# the default sink's front left and right (a mono sink: its one port for both)
sink=$(pactl get-default-sink)
sink_port() { pw-link -i | grep -F -x -m1 -- "$sink:playback_$1"; }
left=$(sink_port FL || sink_port MONO)
right=$(sink_port FR || sink_port MONO)
if [ -z "$left" ] || [ -z "$right" ]; then
    echo "run-standalone: no FL/FR (or MONO) playback ports on the default sink '$sink'" >&2
else
    pw-link "$out1" "$left"
    pw-link "$out2" "$right"
fi

pw-jack a2jmidi_bridge "$bridge_name" >/dev/null 2>&1 &
bridge_pid=$!
pids+=("$bridge_pid")
if midi=$(port_of "$bridge_pid" capture); then
    pw-link "$midi" "$events"
else
    echo "run-standalone: the ALSA MIDI bridge did not start" >&2
fi

echo "run-standalone: playing on $sink; ALSA MIDI port \"$bridge_name\""
wait "$app_pid"
