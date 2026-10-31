# Compatibility

What a player saves has to keep working when Dussek is updated: a DAW session, a preset, a MIDI
map, an instrument made with the editor, a script that renders. This page says what stays stable,
what holds it in place, and how a change to it is made.

## What a player saves

| Saved | Keyed by | Kept stable by |
|---|---|---|
| a DAW session's parameters | the parameter's symbol (LV2, and DPF's VST3 and CLAP state); its index for automation (VST3, CLAP) | the parameter lock |
| the plugin's state | its keys (`_state_version`, `ui`, `instrument`, `velocity_curve`, `morph`, `felt`, `scale_file`) | the state's version and its migrations |
| the instrument chosen | the instrument's folder name | instrument aliases |
| an instrument's files | `instrument.json` (its `format`), `data.bin` | the frozen formats |
| presets, MIDI maps | their files | a version in each file |
| scripts | `dussek render`'s options and the keys of `--param` and `--tuning` | the command-line lock |
| the plugin itself | its LV2 URI, CLAP and VST3 IDs, the binaries' names | fixed since the rename (design log step 213) |

How an instrument sounds is not frozen: refits and engine changes improve it, and that is the
point of a release. A change a player would notice is named in the release notes; a large change
in character goes in as a switch (off by default) or as a new instrument. A player who needs the
old sound keeps the old release.

The editor's projects are experimental: their files may change between releases, and the release
notes say how to move a project along.

## The locks

- **The parameter table only grows** (`plugin/tests/params.lock`, checked by `ui_tests`). Each user
  parameter's index, symbol, range, default, unit and flags are recorded. A new parameter goes at
  the end of `Params.hpp`'s table and is recorded in the same commit
  (`PE_WRITE_PARAMS_LOCK=1 plugin/build/ui_tests` appends it). A parameter no longer wanted stays,
  hidden and inert. The order on screen is the layout's and stays free. The diagnostics after the
  table are not locked: hosts save them by symbol and never automate them.
- **The state carries its version** (`plugin/src/StateVersion.hpp`). `_state_version` is restored
  before the other keys: LV2 restores in declared order (it is declared first), DPF's VST3 and CLAP
  wrappers in key order (the leading `_` sorts it first; a test pins it). Every other key passes
  through `migrateState(version, key, value)`, in the plugin and in its UI (the host hands both
  the saved values). A change to a key's name or form raises `kStateVersion` and adds a migration,
  which must leave a value already in today's form unchanged (a fresh instance assumes today's
  version for a live change). A state from a newer release is read as far as it is understood.
- **Instruments keep their names** (`instruments/aliases.txt`). A renamed or removed instrument
  gets a line `old-id new-id` (or its nearest successor). A session naming an instrument that is
  neither found nor aliased plays nothing and shows it as missing; it is never replaced by another.
- **Released formats keep loading** (`engine/tests/formats/`: a bar instrument and a hammer piano
  of format 5, a registered harpsichord of format 6). Additions to `instrument.json` are optional
  fields with defaults. A change that cannot be optional raises `format`, keeps the old one
  readable, and adds a frozen export of the new format.
- **The files a player keeps carry a version**: a preset's `version = N` (the state version its
  states were saved at), a MIDI map's `version N`. A file without one is version 1; a newer one is
  read as far as it is understood.
- **`render` only grows** (`engine/tests/cli.lock`). Options and keys are added freely (and
  recorded: `PE_WRITE_CLI_LOCK=1`); removing or renaming one takes a release that deprecates it
  first.

## Changing a locked thing

The releases are 0.x for now: a break is still allowed, but deliberate. It is an edit of the lock
file (or the frozen format, or a migration) in a commit that says why, and a "Breaking" entry in
the release notes with what a player does about it. From 1.0 on, a break needs a major version.
