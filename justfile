# Dussek's commands. `just` lists them; `just --show <recipe>` prints one.

set shell := ["bash", "-euo", "pipefail", "-c"]

[private]
default:
    @just --list --unsorted

# ---------------------------------------------------------------- build

# The engine, the plugin and the editor
[group('build')]
build: engine plugin editor

# The real-time engine and its tools (render, bench, midi2wav, level, roll)
[group('build')]
engine:
    cargo build --release --manifest-path engine/Cargo.toml

# The plugin (LV2, CLAP, VST3, JACK standalone) into plugin/build/bin; it builds the engine library too
[group('build')]
plugin: dpf
    [ -f plugin/build/CMakeCache.txt ] || cmake -S plugin -B plugin/build -DCMAKE_BUILD_TYPE=Release
    cmake --build plugin/build

# The instrument editor
[group('build')]
editor:
    cargo build --release --manifest-path editor/Cargo.toml

# DPF (DISTRHO Plugin Framework, ISC license), a git submodule at plugin/dpf: fetched at the commit
# this repository pins (a no-op when it is there)
[group('build')]
dpf:
    git submodule update --init --recursive plugin/dpf

# ---------------------------------------------------------------- test

# Every suite in turn (each fills every core), each to its end, logs in $LOGS; fails if any failed
[group('test')]
test:
    #!/usr/bin/env bash
    set -u
    # per checkout: two checkouts testing at once wrote each other's logs
    logs=${LOGS:-${TMPDIR:-/tmp}/dussek-test-logs/$(basename "{{justfile_directory()}}")}
    mkdir -p "$logs"
    failed=()
    run() {
        local name=$1 start=$SECONDS
        if "{{just_executable()}}" --justfile "{{justfile()}}" "test-$name" > "$logs/$name.log" 2>&1; then
            printf '%-8s ok     %4d s\n' "$name" $((SECONDS - start))
        else
            printf '%-8s FAILED %4d s  (%s)\n' "$name" $((SECONDS - start)) "$logs/$name.log"
            failed+=("$name")
        fi
    }
    run proto
    run editor
    run engine
    run plugin
    if [ ${#failed[@]} -gt 0 ]; then
        echo "failed: ${failed[*]}"
        exit 1
    fi
    echo "all passed"

# The prototype's tests (pytest-xdist; arguments go to pytest)
[group('test')]
test-proto *args:
    python3 -m pytest proto/tests -q -n auto {{args}}

# The editor's tests (arguments go to nextest, e.g. a filter)
[group('test')]
test-editor *args:
    cargo nextest run --release --manifest-path editor/Cargo.toml {{args}}

# The engine's tests (arguments go to nextest, e.g. a filter)
[group('test')]
test-engine *args:
    cargo nextest run --release --manifest-path engine/Cargo.toml {{args}}

# The plugin's UI logic tests (builds the plugin first)
[group('test')]
test-plugin: plugin
    plugin/build/ui_tests

# ---------------------------------------------------------------- run

# The JACK standalone under PipeWire, linked to the default sink, with an ALSA MIDI port
[group('run')]
standalone *args:
    plugin/tools/run-standalone.sh {{args}}

# The instrument editor
[group('run')]
run-editor:
    cargo run --release --manifest-path editor/Cargo.toml

# Instruments from a git ref into the plugin's Snapshots group (names default to all; --remove deletes them)
[group('run')]
snapshot ref *args: engine
    python3 proto/snapshot.py {{ref}} {{args}}

# A project's export as a variant in the plugin's Snapshots group (<project>@<label>), project
# tables overridden for it: just variant grand_a even --with even_levels.window_keys=9
[group('run')]
variant project label *args: engine
    python3 proto/export.py --project {{project}} --variant {{label}} {{args}}

# The grands' traits and bass attack against the recordings' envelope, a page in proto/out/report
# (--snapshot REF draws that ref's snapshots beside today's)
[group('run')]
report *args: engine
    python3 proto/report.py {{args}}

# A MIDI file through the engine into a WAV (midi2wav's options follow, e.g. --param room=0.5)
[group('run')]
midi2wav instrument midi wav *args: engine
    engine/target/release/midi2wav {{instrument}} {{midi}} {{wav}} {{args}}

# Link the built plugin into ~/.lv2, ~/.clap, ~/.vst3 and instruments/ into the user data directory
[group('run')]
install:
    #!/usr/bin/env bash
    set -euo pipefail
    bin="$PWD/plugin/build/bin"
    data="${XDG_DATA_HOME:-$HOME/.local/share}/dussek"
    [ -d "$bin/dussek.lv2" ] || { echo "build the plugin first: just plugin"; exit 1; }
    mkdir -p "$HOME/.lv2" "$HOME/.clap" "$HOME/.vst3" "$data"
    links=("$bin/dussek.lv2:$HOME/.lv2/dussek.lv2"
           "$bin/dussek.clap:$HOME/.clap/dussek.clap"
           "$bin/dussek.vst3:$HOME/.vst3/dussek.vst3"
           "$PWD/instruments:$data/instruments")
    for l in "${links[@]}"; do
        t=${l#*:}
        if [ -e "$t" ] && [ ! -L "$t" ]; then echo "$t exists and is not a link; not touching it"; exit 1; fi
    done
    for l in "${links[@]}"; do ln -sfn "${l%%:*}" "${l#*:}"; done
    echo "installed (links): LV2, CLAP, VST3; instruments at $data/instruments"

# Remove the links install made
[group('run')]
uninstall:
    #!/usr/bin/env bash
    data="${XDG_DATA_HOME:-$HOME/.local/share}/dussek"
    for t in "$HOME/.lv2/dussek.lv2" "$HOME/.clap/dussek.clap" \
             "$HOME/.vst3/dussek.vst3" "$data/instruments"; do
        [ -L "$t" ] && rm "$t" && echo "removed $t"
    done
    true

# ---------------------------------------------------------------- instruments

# Each step of a project's chain: fresh, stale (and why) or never run
[group('instruments')]
status project:
    python3 proto/build.py status {{project}}

# Run a project's chain: a refit, so a sound change (options: --stale, --from STEP, --only STEPS)
[group('instruments')]
fit project *args:
    python3 proto/build.py build {{project}} {{args}}

# Export again without refitting: one project, or every one by default
[group('instruments')]
export project="--all":
    python3 proto/build.py build {{project}} --only export,export_bars

# Before a commit: nothing stale in a project built in this checkout (~6 s; exit 1 with the stale steps)
[group('instruments')]
check:
    python3 proto/build.py status --check --all

# Whether the committed exports are what an export makes today (~2 min, half the cores; one project or every one)
[group('instruments')]
reproduce *projects:
    python3 proto/reproduce.py {{projects}}

# ---------------------------------------------------------------- data

# Download a University of Iowa MIS set (piano or guitar) into data/iowa-mis-<set>/; re-runnable
[group('data')]
fetch-iowa set="piano":
    #!/usr/bin/env bash
    set -u
    cd data
    mkdir -p "iowa-mis-{{set}}"
    fail=0
    while read -r p; do
        f=iowa-mis-{{set}}/$(basename "$p")
        [ -s "$f" ] && continue
        url="https://theremin.music.uiowa.edu/$(printf %s "$p" | sed 's/ /%20/g')"
        if curl -sfL --retry 3 -o "$f.part" "$url"; then mv "$f.part" "$f"
        else echo "FAIL $f"; fail=$((fail + 1)); rm -f "$f.part"; fi
        sleep 0.5   # gentle on the university's server
    done < "iowa-mis-{{set}}-links.txt"
    echo "done, failures: $fail"
