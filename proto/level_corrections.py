"""A source's level corrections (`[part.source.level_corrections]`, from the editor's Compare page):
dB per recorded key at pp, mf and ff, interpolated linearly to every key and held beyond the
recorded ones; the exports write them per note as `level_db`, one [pp, mf, ff] per mallet."""
import numpy as np

import project as P

DYNAMICS = ("pp", "mf", "ff")


def check(table, sid):
    """Raise ValueError naming the source when the table cannot be read."""
    keys = table.get("keys")
    if not isinstance(keys, list) or not keys or not all(isinstance(k, int) for k in keys):
        raise ValueError(f"{sid}: level_corrections needs keys, a list of MIDI keys")
    if any(b <= a for a, b in zip(keys, keys[1:])):
        raise ValueError(f"{sid}: level_corrections keys must be ascending")
    for d in DYNAMICS:
        v = table.get(d)
        if v is None:
            continue
        if not isinstance(v, list) or len(v) != len(keys):
            raise ValueError(f"{sid}: level_corrections {d} needs one value per key ({len(keys)})")
        if not all(isinstance(x, (int, float)) and not isinstance(x, bool) and np.isfinite(x) for x in v):
            raise ValueError(f"{sid}: level_corrections {d} must be numbers")


def at(table, key):
    """[pp, mf, ff] dB at a key: linear between the recorded keys, the ends held."""
    keys = table["keys"]
    return [float(np.interp(key, keys, table[d])) if d in table else 0.0 for d in DYNAMICS]


def tables(part_id, roles):
    """The part's sources of these roles in order, each one's table or None (checked)."""
    out = []
    for sid in P.source_ids(part_id):
        s = P.source(sid)
        if s.get("role", "main") not in roles:
            continue
        t = P.level_corrections(sid)
        if t is not None:
            check(t, sid)
        out.append(t)
    return out


def note_levels(tabs, key):
    """A note's level_db: one [pp, mf, ff] per table (none: 0 dB); None when there is no table."""
    if all(t is None for t in tabs):
        return None
    return [at(t, key) if t is not None else [0.0, 0.0, 0.0] for t in tabs]
