# AGENTS.md

If `private/` exists (the maintainer's private material, a repository of its own), read
`private/AGENTS.md` too.

A physically modeled piano (and harpsichords, harps, mallet instruments): measurements from
freely licensed recordings → a Python prototype that fits each instrument → exported
instruments → a real-time Rust engine → a DPF plugin (LV2, JACK standalone; VST3/CLAP built
but untested).

- `README.md`: the front page; `docs/*.md`: the user-facing description.
- `docs/architecture.md`: how the system works today.
- `docs/log/`: the design log (numbered steps, newest last; one line per step in its `README.md`).
- `ROADMAP.md`: what is open, by horizon (background in `docs/roadmap-notes.md`).
- `justfile`: the commands (`just` lists them).

## Layout

| Path | What |
|---|---|
| `projects/` | one project per instrument, **committed**: `<name>/project.toml` (every per-instrument choice: the sets' maps, the scale model, the fits' settings, the borrowings, the export and its menu entry), `fits/<part>.json` (the fitted values), `ledger.json` (what ran on which inputs); `cache/` (analysis results, intermediate files, logs) not in git, except what a lender lends (`git add -f`, listed by `proto/tests/test_fresh_clone.py`); `steps.toml`: the steps and each family's chain; `kinds.toml`: the wizard's answers to "How is it played?" and their donors; two projects are not in the menu: `harpsichord_buff` (the buff stop's source, no export), `registers_fixture` (a test fixture, `hidden` in its menu) |
| `analysis/` | measurements from the recordings (Python); `project.py` (the projects' files and the steps' checks), `mapping.py` (a set's files → notes and dynamics) |
| `proto/` | the offline model and its fits (Python); `build.py` runs a project's chain; renders in `proto/out/` (not in git) |
| `proto/export.py`, `proto/export_bars.py` | write `instruments/<project>/` (strings, bars) |
| `instruments/` | exported instruments, **committed**; re-export after changing a fit, an export or a menu entry |
| `engine/` | the real-time engine (Rust 2024); `src/bin/`: `render`, `bench`, `midi2wav`, `level` (an export's measured level, kept in its `instrument.json`), `roll` (a rolled piano's export) |
| `plugin/` | DPF wrapper (C++17): `src/Params.*` (the parameter table), `src/PianoPlugin.cpp` (DSP side), `src/ui/` (UI; pure logic in `pe_common`, tested by `tests/ui_tests.cpp`), `presets/` (the shipped presets), `tools/` (`run-standalone.sh`; `lv2render.c`, an offline LV2 host) |
| `editor/` | the instrument editor (Rust 2024, egui): `src/` the logic (project files, the mapper ported from `analysis/mapping.py`, measurements, the suggestion, `rule.rs` the rule the pages edit, `wizard.rs`, the driver client), `src/ui/` the window (`steps/` the pages the wizard and a source's tabs share) |
| `albums/demos/` | original demo pieces (CC0), the script that composes them and one that mixes the ensemble piece's stems; renders in `out/` (not in git) |
| `data/` | datasets, **not in git**; how to get each one: `docs/getting-data.md`, `data/SOURCES.md` |
| `docs/lab/` | the engine's ideas as six small experiments (a notebook) |

## Build and test

The `justfile` holds the commands (`just` lists them, `just --show <recipe>` prints one):

```sh
just test            # all four suites in turn, each to its end: ~2.5 min plus the builds, exit
                     # status non-zero if any failed (each suite's output in
                     # $TMPDIR/dussek-test-logs/<checkout>)
just test-engine     # engine: cargo nextest, every core (~2 min; a filter after it narrows)
just test-plugin     # plugin: builds it (DPF, a git submodule, fetched at its pinned commit), then ui_tests
                     # (last line "N tests, 0 failed checks", exit 0)
just test-proto      # prototype: pytest -n auto (~15 s)
just test-editor     # editor: cargo nextest (~10 s; the corpus test decodes the sets the first time)
just build           # engine, plugin, editor (release)

# by hand, minutes: a new project from Harp A's set, built (it is deleted again afterwards)
cargo test --release --manifest-path editor/Cargo.toml --test acceptance -- --ignored --nocapture
```

The engine's tests use the committed instruments; a test without its data returns early.

- Run all four suites before calling a change done: the layers test each other's data (an
  export key removed for the engine broke a `proto/tests` assertion once).
- Before committing a change under `proto/`, `engine/`, `projects/` or `instruments/`: `just check`
  (nothing stale). Before merging a change to `proto/` or `engine/`, and before committing an export:
  `just reproduce` (the committed exports are what an export makes today; a difference is a sound
  change, for the user's ear).
- Run the suites, exports and batch renders at the lowest priority: `nice -n 19 just test`,
  `nice -n 19 python3 proto/build.py …`. They fill every core for minutes while the machine stays
  in use; niced, it stays usable.
- `cargo nextest` runs every test of every binary in parallel (`cargo test` runs the binaries one
  after another: the engine took 5 min); its `.config/nextest.toml` (engine, editor) runs
  everything to the end (`fail-fast = false`: a flaky test once hid a real failure behind it). With
  plain `cargo test`, pass `--no-fail-fast`. Known flaky (timing): `buff.rs::switching_costs_little_on_the_audio_thread`
  (nextest runs it alone); rerun it alone before treating it as a failure.
- A test that renders many independent cases maps them over the cores (`engine/tests/common`:
  `par`, an instrument loaded once per process, engines built through a shared radiation cache);
  one single-threaded test set the engine suite's floor at 224 s.
- Decide on a test run by its exit status; do not pipe it through `grep` and then commit.
- After `git stash` / `git stash pop`, `sleep 1; touch` the touched files before cargo: same-second
  mtimes skip the rebuild and an A/B check runs the wrong binary. Never stash while a cargo run
  is going in the background.
- The UI's drawing has no unit tests. To look at it without a display:
  `Xvfb :97 -screen 0 1600x1000x24 &`, run `plugin/build/bin/dussek` with `DISPLAY=:97`,
  drive it with `xdotool`, capture with `import -window root`. Under PipeWire the standalone
  runs with `just standalone` (`plugin/tools/run-standalone.sh`: `pw-jack`, the outputs linked to the
  default sink, an ALSA MIDI port). The editor the same way
  (`editor/target/release/dussek-editor`), with `WAYLAND_DISPLAY` unset (winit prefers
  Wayland), `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` (else the folder picker opens on the
  desktop through its portal) and `xdotool search --name "Dussek editor" windowfocus`
  before typing.

## How things fit together

- Parameters: `plugin/src/Params.hpp` is the plugin's interface (indices, symbols, ranges).
  Every parameter has a `description` (the tooltip; `ui_tests` checks it).
- Instruments in the plugin: the catalog (`plugin/src/Catalog.*`) lists every instrument
  directory in the search paths; the choice is the `"instrument"` state (the directory's name,
  or an opened folder's path: step 163). What an instrument shows comes from the engine's report
  on its file (`engine/src/info.rs`, `pe_instrument_info`: rules over the data, never a name) and
  the export's menu block (`[menu]` in the project: name, group, labels, mallets' names,
  `reversed_keys`, `hidden`); `engine/tests/no_effect.rs` checks the report against renders on
  every exported instrument, `plugin/tests/visibility.txt` records what the UI shows per
  instrument (`PE_WRITE_UI_TABLE=1 plugin/build/ui_tests` rewrites it: read its diff).
  `plugin/src/ui/Instruments.cpp` `paramActive`/`paramVisible` read the entry.
- The rail for by-ear changes: `just snapshot <ref>` puts instruments from a git ref into the
  plugin's "Snapshots" group (outside the repository; the suites never read it), `just variant
  <project> <label> --with table.key=value` a project's export with a value overridden,
  `just report [--snapshot <ref>] [--uprights]` draws the grands' (or the uprights') traits and
  bass attack against their recordings' envelope (`docs/building-instruments.md`, "Listening,
  snapshots and the report").
- The UI layout is pure (`plugin/src/ui/Layout.cpp`: tabs, groups, compact rows); rows in the
  content area carry `scrolls` and live in layout coordinates (window + scroll).
- Fitting an instrument: `python3 proto/build.py build <project>` (the chains: `projects/steps.toml`;
  `status <project>` says what is stale and why; `just fit`, `just status`); `just export
  [<project>]` exports again without refitting (`--only export,export_bars`, every project by
  default); `docs/building-instruments.md`.
- The editor's mapper (`editor/src/mapping.rs`) gives the same items as `analysis/mapping.py`
  (`editor/tests/mapping_parity.rs`): change both together. Its suggestion is measured against the
  committed maps (`editor/tests/suggest_corpus.rs`, a floor of hits).
- The editor's pages edit `rule.rs`'s model of a source, never TOML directly; reading a committed
  source and applying it back must give it unchanged (`editor/tests/rule_corpus.rs`).
- No instrument is named in `analysis/`, `proto/`, `plugin/src` or `editor/src` (`proto/tests/test_no_names.py`):
  its choices go into its project file, with the reason as a comment. A step reads and writes only what its
  entry in `projects/steps.toml` declares (`analysis/project.py` checks it under the driver).

## Working on this repository

- Work on a branch. Merge (fast-forward) and push only when the maintainer says so.
- Commit subjects: `Area: what changed` (`Engine:`, `Plugin UI:`, `Vibraphone:`, `Docs:` …),
  a body that says why.
- A behavior change updates the docs where they describe it (`README.md`, `docs/*.md`,
  `docs/architecture.md`) and appends a numbered step to the last file of `docs/log/` (a heading
  with its anchor, then what changed, why, the measured numbers) with its line in
  `docs/log/README.md`.
- The ear outranks the metrics. A complaint about the sound becomes a measurable statistic,
  then a fix; a sound change is listened to before it is merged.
- Where the real instrument's mechanism plays badly from a keyboard, prefer the keyboard
  player's mapping (the vibraphone: a held key lifts its damper bar, step 48).
- What a player saves stays loadable (`docs/compatibility.md`): a new parameter goes at the end of
  `Params.hpp`'s table and into `plugin/tests/params.lock`; a state key renamed or reshaped raises
  `kStateVersion` with a migration; a renamed instrument gets a line in `instruments/aliases.txt`;
  `instrument.json` grows by optional fields (a change that cannot be optional raises `format` and
  freezes a copy in `engine/tests/formats/`); `render`'s options are added, not renamed. During 0.x
  a break is a deliberate edit of the lock in a commit that says why.
- LV2 and the JACK standalone are the targets; VST3/CLAP are not tested.

## Style

- Comments and docs: short, present tense, American spelling, the codebase's "the …" voice;
  numbers with units and a space (`12 dB`, `48 kHz`, `0.6 s`).
- Match the surrounding code (4-space indent, `pe` namespace in the plugin).
- No product names or trademarks in instrument names or presets (Grand A, Upright B, …).

## Shell

- Scope searches to the repository; never `find /`.
- Waiting on a job with `pgrep -f <its command>` matches the waiting loop itself; wait on a
  PID or a file instead.

## Legal

- Recordings are for analysis only: no third-party audio in git or in the engine. Every export
  carries `instruments/<dir>/LICENSE.md`, written from its project's `[license]` block and its
  lenders' (`proto/license.py`; CC BY 4.0, or CC BY-SA 4.0 where a ShareAlike set is reached).
