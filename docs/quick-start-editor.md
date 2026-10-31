# Quick start: a new instrument with the editor

**Experimental.** The editor and the fitting pipeline work, and they made every instrument here,
but they are tools for building instruments, not yet a product: expect rough edges, long builds
and pages that assume you know the model. [editor.md](editor.md) describes every page;
[building-instruments.md](building-instruments.md) the files and commands underneath.

This walks from a sample set to a playable instrument, with VCSL's concert harp (CC0) as the
example: the set Harp A was made from.

## What you need

- The repository, built: `just build` (the README's [Build from source](../README.md#build-from-source)).
- Python 3.11 or newer with the packages of `requirements.txt` (`pip install -r
  requirements.txt`). The editor runs the pipeline (`proto/build.py`) through it.
- The sample set, under `data/`: VCSL, cloned into `data/vcsl/`
  ([getting-data.md](getting-data.md) lists every set and where it goes). A set elsewhere on
  disk works too: the editor links it in.

Nothing else: a new instrument starts from the closest one of its kind, its donor, and borrows
what its own recordings cannot give (a harp's dampers are Grand E's, measured from its release
recordings; its release noise Grand A's). What the lenders lend is in the repository, and a
lender you have not built yourself is used as it is.

## Steps

1. **Start the editor:** `just run-editor`. The projects are listed on the left.
2. **New instrument…** opens the wizard. Each page says what it is for.
3. **Folder:** choose `data/vcsl/Chordophones/Composite Chordophones/Concert Harp` (Choose…,
   or type the path), then **Use this folder**.
4. **How to read the files:** the set has no SFZ map, so the editor reads the file names. It
   proposes the reading; keep it.
5. **The file names:** each part of an example name has its role (the note, the dynamic, the
   take). Keep the proposal; check that the note and the dynamic are where they belong.
6. **Dynamics:** one row per recorded layer with its measured level. The concert harp has four
   (p, mp, mf, f), but p and mp hold only a few files: keep mf and f alone, mf as mf and f as ff,
   as Harp A does.
7. **How is it played?** Choose **harp**. The donor is the harp with the closest key range;
   the collapsed list offers every donor, so pick Harp A there if another is shown.
8. **Check and listen:** the grid of keys and layers, with suspect takes marked. A click on a key
   lists its files, and a row plays its file; a wrong one (a mislabeled note, a flawed take) can
   be left out here.
9. **Name and menu:** a name in lower case (`my_harp`), its name and group in the plugin's menu
   (`My harp`, `Harps`). Leave **Build it now** ticked and press **Finish**.

The build runs every step of the chain (the analysis, the fits, the export) and shows each one's
progress and log. A harp takes a few minutes.

## Where it lands

- `projects/my_harp/`: the project. `project.toml` holds every choice, its first line naming the
  donor and why; `fits/` the fitted values.
- `instruments/my_harp/`: the export. After `just install`, the plugin lists it in its menu
  without a restart; a copy of the folder anywhere plays through the instrument menu's
  "Open instrument folder…".

## Its license

The export writes `instruments/my_harp/LICENSE.md`. A new project names no recordings of its own
yet, so the file lists what its borrowed parts carry and says that the instrument's own license
is not stated. Add the set's credit to `project.toml`:

```toml
[license]
set = "VCSL: Concert Harp"
author = "Versilian Studios"
url = "https://github.com/sgossner/VCSL"
terms = "The Versilian Community Sample Library."
source_license = "CC0"   # CC0, public-domain, free-use, CC-BY-4.0 or CC-BY-SA-4.0
by = "Your Name"         # who made the instrument
```

then write the file again: `python3 proto/license.py my_harp`. The instrument is CC BY 4.0, or
CC BY-SA 4.0 when any recordings it reaches, its own or a lender's, are ShareAlike (Upright C's).
Music made with it is the player's.

## Next

- **Compare** sets the instrument beside its recordings, note by note, and corrects its levels.
- A project's pages edit any choice; **Build** runs the steps a change made stale.
