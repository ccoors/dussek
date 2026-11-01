# Changelog

What each release changes for a player. From 1.0 on a break needs a major version; before it, a
break is allowed but listed under "Breaking" (`docs/compatibility.md`).

## Unreleased

### New

- A tongue drum (Percussion): a steel tongue drum's 15 tongues E3–E5, from new recordings at three
  dynamics; the other tongues ring along ("Ringing tongues").

### Changed

- Upright D: its own key release noise, from new recordings of its keys released without a
  note (it borrowed Grand A's), at its recordings' level: about 10 dB louder and brighter. Its A5
  at ff from a new, unclipped recording.

### Fixed

- Linux: instruments installed with `install.sh --system` are found when `$XDG_DATA_DIRS`
  leaves out `/usr/local/share` (NixOS, some desktops).
- Linux: a failed `install.sh` lists what it copied, so `uninstall.sh` or the next install removes
  it; a relative `$XDG_DATA_HOME` is ignored, as the XDG spec says.
- Windows: the installer keeps Dussek in its fixed folder (a plugin's Copy CLI looks for the
  standalone there) and removes the Start menu entry when an update leaves the standalone out.

## 0.3.0 — 2026-11-01

The first release.

### New

- Nine grands, four uprights, six harpsichords, a clavichord, harps, guitars, zithers and
  percussion, each physically modeled from freely licensed recordings, shipped as numbers.
- A standalone and LV2, CLAP and VST3 plugins for Linux, Windows and macOS (Apple silicon).
- Installers: a setup program on Windows, a package on macOS, `install.sh` on Linux; the
  instruments installed once for every plugin format.
