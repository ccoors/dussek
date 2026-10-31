# Contributing

Bug reports and pull requests are welcome. A report of how an instrument sounds is as useful as
one of a crash: which instrument, which notes, what you hear, and if you can, a MIDI file that
plays it.

## Before a pull request

- **Run all four suites:** `just test` (the engine, the plugin's UI logic, the prototype, the
  editor; a few minutes, every core). Run all four even for a change in one layer: the layers
  test each other's data. `just test` exits non-zero if any suite failed; each suite's output is
  in `${TMPDIR:-/tmp}/pianoengine-test-logs/<checkout>/`.
- **A change to `proto/`, `engine/`, `projects/` or `instruments/`:** also `just check` (no step
  is stale of any instrument built in your checkout from its recordings) and, before an export
  goes in, `just reproduce` (the committed exports are what an export makes today).
  [docs/building-instruments.md](docs/building-instruments.md) has the details.
- **The tests without the datasets:** a test whose recordings are not in `data/`, or whose
  project caches are not built, returns early or is skipped and names what it needs, so the suites
  pass in a fresh clone.
- **CI** runs the build, the plugin's UI tests, the prototype's and the editor's suites and
  `just check` on every push and pull request; the engine's suite, which takes longer, nightly.

## Commits

- Subjects read `Area: what changed`: `Engine: …`, `Plugin UI: …`, `Grand C: …`, `Docs: …`. The
  body says why.
- A change in behavior updates the docs that describe it (`README.md`, `docs/*.md`,
  `docs/architecture.md`) and adds a numbered step to the last file of [docs/log/](docs/log/README.md):
  what changed, why, and the measured numbers, with its line in `docs/log/README.md`.
- The style of comments and docs: short, present tense, American spelling, units with a space
  (`12 dB`, `48 kHz`). [AGENTS.md](AGENTS.md) has the rest of the conventions.

## Sound changes

A change that changes how an instrument sounds needs listening before it goes in. The ear
outranks the metrics: say in the pull request what to play and what to listen for, and how the
change measures against the recordings. A change that is only more physical goes in as a
switch, off by default, until people have played it.

## Instruments and their licenses

The instruments are CC BY 4.0, except Upright C, which is CC BY-SA 4.0. ShareAlike reaches
through borrowings: an instrument that borrows from Upright C, or a rolled piano with it among
its donors, is CC BY-SA 4.0 too. The export writes each instrument's `LICENSE.md` from its
project's `[license]` block (the recordings it was measured from) and those of the projects it
borrows from.

Recordings are for analysis only: no third-party audio goes into the repository or the engine.
No product names or trademarks go into instrument names or presets.

## License

By contributing, you agree that your contribution is licensed under the
[AGPL-3.0-or-later](LICENSE), and for instruments, under the instrument's license.
