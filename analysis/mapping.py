"""A sample set's items from its source's `map` (project.toml): which file is which note and
dynamic. Two kinds: `sfz` (an SFZ file's regions: layers ranked by velocity within a note, or
one file per layer) and `pattern` (file names: a glob and a regex with named groups), then the
source's own `relabel`, `skip`, `skip_files` (left out before the take choice), `tail_cut_s`, `cut_s`, `gain_db`, `extra` and `octave`. An
item is what analyze.py and modes.py read: path, relpath, note, layer, layers, dynamic,
tail_cut_s and the set's own fields."""
import glob
import os
import re

NOTE_OFFSETS = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
ACCIDENTALS = {"#": 1, "s": 1, "b": -1, None: 0}


def note_name(text):
    """'A0', 'D#1', 'Bb0', 'Cs4' (a sharp spelled s), 'g3' → MIDI (C4 = 60)."""
    m = re.fullmatch(r"([A-Ga-g])(#|s|b)?(-?\d)", text)
    if not m:
        raise ValueError(f"not a note name: {text!r}")
    return NOTE_OFFSETS[m[1].upper()] + ACCIDENTALS[m[2]] + 12 * (int(m[3]) + 1)


KEY_OPCODES = ("pitch_keycenter", "lokey", "hikey")


def _keys(r, offset):
    """A region's keys: key= stands for lokey, hikey and pitch_keycenter (where they are not
    given); a key by note name (c4 = 60) as its number; <control>'s octave_offset and
    note_offset move each key given as a number."""
    if "key" in r:
        for k in KEY_OPCODES:
            r.setdefault(k, r["key"])
    for k in (*KEY_OPCODES, "key"):
        if k in r and re.fullmatch(r"[A-Ga-g][#sb]?-?\d", r[k].strip()):
            r[k] = str(note_name(r[k].strip()))
    if offset:
        for k in (*KEY_OPCODES, "key"):
            if k in r and re.fullmatch(r"-?\d+", r[k].strip()):
                r[k] = str(int(r[k]) + offset)
    return r


def _read_sfz(path):
    return re.sub(r"//[^\n]*", "", open(path, encoding="utf-8", errors="replace").read())


def expand_sfz(path):
    """An SFZ file's text without comments, its #include files in their places (relative to the
    top file's folder) and each $NAME a #define names replaced by its value in what follows."""
    top = os.path.dirname(path)
    defines = {}

    def expand(text, depth):
        out, at = [], 0
        for m in re.finditer(r'#include\s+"([^"]*)"|#define\s+(\$\w+)\s+(\S+)', text):
            out.append(_substitute(text[at:m.start()], defines))
            at = m.end()
            if m[1] is not None:
                inc = os.path.join(top, m[1].replace("\\", "/"))
                if depth > 16 or not os.path.isfile(inc):
                    raise ValueError(f"{path}: #include {m[1]!r}: " + ("too deep" if depth > 16 else "no such file"))
                out.append("\n" + expand(_read_sfz(inc), depth + 1) + "\n")
            else:
                defines[m[2]] = m[3]
        out.append(_substitute(text[at:], defines))
        return "".join(out)

    return expand(_read_sfz(path), 0)


def _substitute(text, defines):
    # the longest name first ($EXT before $EX)
    for k in sorted(defines, key=len, reverse=True):
        text = text.replace(k, defines[k])
    return text


def parse_sfz(path):
    """Regions of an SFZ file (expand_sfz) as dicts of opcodes (global < group < region), their
    keys as _keys gives them."""
    text = expand_sfz(path)
    scopes = {"control": {}, "global": {}, "group": {}}
    regions, cur = [], None
    for tok in re.split(r"(<\w+>)", text):
        h = re.fullmatch(r"<(\w+)>", tok)
        if h:
            cur = h[1]
            if cur == "group":
                scopes["group"] = {}
            elif cur == "region":
                regions.append({**scopes["global"], **scopes["group"]})
            continue
        # opcodes: key=value, where a value (sample paths) may contain spaces
        for m in re.finditer(r"(\w+)=(.*?)(?=\s+\w+=|\s*$)", tok.strip(), flags=re.S):
            k, v = m[1], m[2].strip()
            if cur == "region":
                regions[-1][k] = v
            elif cur in scopes:
                scopes[cur][k] = v
    c = scopes["control"]
    offset = 12 * int(c.get("octave_offset", 0)) + int(c.get("note_offset", 0))
    return [_keys(r, offset) for r in regions]


def _passes(r, f):
    v = r.get(f["opcode"], f.get("default"))
    if v is None:
        return False
    if "in" in f:
        return str(v) in f["in"]
    if "lt" in f:
        return float(v) < f["lt"]
    if "eq" in f:
        return float(v) == f["eq"]
    raise ValueError(f"filter {f}: needs in, lt or eq")


# the opcodes that number a key's takes (round robins): a filter on one is the take rule
TAKE_OPCODES = ("seq_position", "lorand")


def _takes(rs, takes, skip, rel_of):
    """The regions kept by the take rule `takes` (filters on TAKE_OPCODES), the left-out files
    `skip` dropped first: per key and velocity range the region passing the rule, or, where that one
    is left out, the next take not left out (the lowest value, then the sample); a group with no
    region passing the rule keeps none. Without a rule, only the left-out files go."""
    if not takes:
        return [r for r in rs if rel_of(r) not in skip]
    groups = {}
    for r in rs:
        key = (str(r.get("pitch_keycenter", r.get("lokey", 60))), str(r.get("lovel", 0)), str(r.get("hivel", 127)))
        groups.setdefault(key, []).append(r)
    kept = set()
    for g in groups.values():
        passing = [r for r in g if all(_passes(r, f) for f in takes)]
        keep = [r for r in passing if rel_of(r) not in skip]
        if not keep and passing:
            f = takes[0]
            rest = sorted((r for r in g if rel_of(r) not in skip),
                          key=lambda r: (float(r.get(f["opcode"], f.get("default", 0))), r["sample"]))
            keep = rest[:1]
        kept.update(id(r) for r in keep)
    return [r for r in rs if id(r) in kept]


def _sfz(root, m, skip=frozenset()):
    def rel_of(r):
        rel = m.get("prefix", "") + r["sample"].replace("\\", "/")
        return os.path.normpath(rel) if m.get("normpath") else rel

    filters = m.get("filter", [])
    takes = [f for f in filters if f["opcode"] in TAKE_OPCODES]
    per_file, whole = [], []
    for name in m["files"]:
        rs = [r for r in parse_sfz(os.path.join(root, name))
              if "sample" in r and all(_passes(r, f) for f in filters if f not in takes)]
        if m.get("folders"):
            rs = [r for r in rs if set(m["folders"]) & set(r["sample"].replace("\\", "/").split("/")[:-1])]
        pref = m.get("prefer_substring")
        if pref and any(pref in r["sample"] for r in rs):
            rs = [r for r in rs if pref in r["sample"]]
        per_file.append(_takes(rs, takes, skip, rel_of))
        whole.append(_takes(rs, takes, frozenset(), rel_of))

    if m.get("layers", "rank") == "file":
        for i, rs in enumerate(per_file, 1):
            seen = set()
            for r in rs:
                rel = rel_of(r)
                if m.get("unique") == "sample":
                    if rel in seen:
                        continue
                    seen.add(rel)
                yield {"path": os.path.join(root, rel), "relpath": rel,
                       "note": int(r.get("pitch_keycenter", r.get("lokey"))), "layer": i, "layers": len(per_file),
                       "dynamic": m["dynamic_format"].format(layer=i), "tail_cut_s": 0.0}
        return
    by_note, ranks = _by_note(per_file), _by_note(whole)
    dyn_from = m.get("dynamic_from")
    for note, rs in by_note.items():
        # the ranks of the key's takes before any is left out: a left-out layer stays empty
        slots = [_velocities(r) for r in ranks[note]]
        for r in rs:
            rank = slots.index(_velocities(r)) + 1
            slots[rank - 1] = None
            rel = rel_of(r)
            dyn = f"{rank}/{len(slots)}"
            if dyn_from:
                dyn = dyn_from["format"].format(re.search(dyn_from["regex"], os.path.basename(rel))[1])
            it = {"path": os.path.join(root, rel), "relpath": rel, "note": note, "layer": rank, "layers": len(slots),
                  "dynamic": dyn, "tail_cut_s": 0.0}
            if m.get("sfz_fields", True):
                it.update({"velocity": (int(r.get("lovel", 0)), int(r.get("hivel", 127))),
                           "sfz_volume_db": float(r.get("volume", 0)), "sfz_tune_cents": float(r.get("tune", 0))})
            yield it


def _by_note(per_file):
    """The regions per note (the SFZ default 60 where a region leaves it out), a sample seen again
    in its first place, ranked by velocity."""
    by_note = {}
    for r in (r for rs in per_file for r in rs):
        by_note.setdefault(int(r.get("pitch_keycenter", 60)), {})[r["sample"]] = r
    return {note: sorted(s.values(), key=lambda r: int(r.get("lovel", 0))) for note, s in sorted(by_note.items())}


def _velocities(r):
    return (int(r.get("lovel", 0)), int(r.get("hivel", 127)))


def _note(g, spec):
    v = g[spec.get("group", "note")]
    style = spec.get("style", "name")
    if style == "name":
        return note_name(v) + spec.get("shift", 0)
    if style == "midi":
        return int(v) + spec.get("shift", 0)
    if style == "index":
        return int(v) + spec.get("offset", 0)
    raise ValueError(f"note style {style!r}")


ORDERS = {"note": lambda it: (it["note"], it["layer"], it["path"]),
          "layer": lambda it: (it["layer"], it["note"], it["path"]),
          "path": lambda it: it["path"]}


def _pattern(root, m, relabel, skip=frozenset()):
    rx = re.compile(m["regex"])
    layers, names, fmt = m.get("layers"), m.get("dynamic_names", {}), m.get("dynamic_format", "{}")
    prefer, fields = m.get("prefer", []), m.get("fields", {})
    best, rows = {}, []
    # the root is a folder, not a pattern (a set named "Set [2]")
    for path in glob.glob(os.path.join(glob.escape(root), m["glob"]), recursive=True):
        rel = os.path.relpath(path, root)
        if rel in skip:
            continue
        g = rx.fullmatch(rel if m.get("match") == "relpath" else os.path.basename(path))
        if not g:
            continue
        g = g.groupdict()
        note = _note(g, m["note"])
        note = relabel.get(note, note)
        if layers is None:
            layer, n, dyn = 1, 1, m["dynamic"]
        else:
            if g["dynamic"] not in layers:
                continue
            layer, n = layers.index(g["dynamic"]) + 1, len(layers)
            dyn = names.get(g["dynamic"], fmt.format(g["dynamic"]))
        it = {"path": path, "relpath": rel, "note": note, "layer": layer, "layers": n, "dynamic": dyn, "tail_cut_s": 0.0}
        for k, t in fields.items():
            it[k] = int(g[k]) if t == "int" else g[k]
        if not prefer:
            rows.append(it)
            continue
        rank = (*(int(g[k]) for k in prefer), path)
        if (note, layer) not in best or rank < best[(note, layer)][0]:
            best[(note, layer)] = (rank, it)
    rows += [it for _, it in best.values()]
    return sorted(rows, key=ORDERS[m.get("order", "note")])


def items(src, root):
    """The items of the source table `src` under its data directory `root`."""
    m = src["map"]
    relabel = {int(k): v for k, v in src.get("relabel", {}).items()}
    skip_files = set(src.get("skip_files", []))
    if m["kind"] == "sfz":
        rows = [{**it, "note": relabel.get(it["note"], it["note"])} for it in _sfz(root, m, skip_files)]
    elif m["kind"] == "pattern":
        rows = _pattern(root, m, relabel, skip_files)
    else:
        raise ValueError(f"source {src['id']}: unknown map kind {m['kind']!r}")
    skip = set(src.get("skip", []))
    cut = {int(k): v for k, v in src.get("cut_s", {}).items()}
    gain = src.get("gain_db", {})
    out = []
    for it in rows:
        if it["note"] in skip:
            continue
        it["tail_cut_s"] = cut.get(it["note"], src.get("tail_cut_s", 0.0))
        if it["dynamic"] in gain:
            it["gain_db"] = gain[it["dynamic"]]
        it.update(src.get("extra", {}))
        if "octave" in src:
            it = {**it, "key": it["note"], "note": it["note"] + src["octave"]}
        out.append(it)
    return out


def _dump(ids):
    """One JSON line per source whose data directory exists: its items, or the error."""
    import json
    import project as P
    for sid in ids or sorted(P.index()[2]):
        root = P.data_root(sid)
        if not os.path.isdir(root):
            continue
        try:
            print(json.dumps({"id": sid, "root": root, "items": items(P.source(sid), root)}))
        except Exception as e:  # noqa: BLE001  (the reference's error is part of the comparison)
            print(json.dumps({"id": sid, "root": root, "error": f"{type(e).__name__}: {e}"}))


if __name__ == "__main__":
    import argparse
    ap = argparse.ArgumentParser(description="the items of sources, as JSON lines (the editor's parity test)")
    ap.add_argument("--dump", nargs="*", metavar="SOURCE", required=True)
    _dump(ap.parse_args().dump)
