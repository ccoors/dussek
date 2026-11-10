"""Which instrument the modules work on (BRASS_PROJECT, set by steps.py): its [brass] table, its fit keys and
its caches, all through analysis/project.py so the driver's checks hold. The modules keep the instrument in
module state; forked workers inherit it."""
import contextlib
import hashlib
import json
import os
import subprocess
import sys


HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "analysis"))
import mapping  # noqa: E402
import project as P  # noqa: E402
import split_brass  # noqa: E402

NAME = os.environ["BRASS_PROJECT"]
PART = NAME
OUT = os.path.join(HERE, "out")  # what a step leaves for a person to read: not in git, no step's input
os.makedirs(OUT, exist_ok=True)


def load(key, default=None):
    """A fit key."""
    return P.load_fit(PART).get(key, default)


def save(key, value):
    P.update_fit(PART, {key: value})


def load_cache(name, default=None):
    return P.load_cache(PART, name) if P.has_cache(PART, name) else default


def save_cache(name, value):
    P.write_cache(PART, name, value)


def lender(key, default=None):
    """A fit key of the instrument the mutes are borrowed from, or this one's where it has its own."""
    own = load(key)
    if own is not None:
        return own
    other = P.uses(NAME, "mutes")
    return P.load_fit(P.main_part(other)).get(key, default) if other else default


def notes():
    """{"midi_dyn": {midi, dyn, file, start, end}}: the cut notes in their runs (analysis/split_brass.py): the fits
    measure the background in the silence around a note, which its own file leaves out."""
    out = {}
    for sid in P.source_ids(NAME):
        root = P.data_root(sid)
        for it in mapping.items(P.source(sid), root):
            out[f"{it['note']}_{it['dynamic']}"] = dict(midi=it["note"], dyn=it["dynamic"], **split_brass.where(it["path"], root))
    return out


def tables_path(name, writing=False):
    """A table memo's file (cache.tables: rebuilt by whichever step needs it, from what its key covers)."""
    (P._write if writing else P._read)(PART, "cache.tables")
    d = os.path.dirname(P.cache_path(PART, "tables"))
    os.makedirs(d, exist_ok=True)
    return os.path.join(d, f"{name}.pkl")


_GONE = object()


@contextlib.contextmanager
def trying(table, key, value=_GONE):
    """A trial value in a module's table (`value` left out: the key taken out) for one measurement, the table as it was
    afterwards. A pool worker measures many trials: one left behind changed what the next one measured, so the order
    the pool handed them out decided the fit."""
    had, old = key in table, table.get(key)
    if value is _GONE:
        table.pop(key, None)
    else:
        table[key] = value
    try:
        yield
    finally:
        if had:
            table[key] = old
        else:
            table.pop(key, None)


def code_hash(names):
    """The hash of these modules' sources (a memo's key)."""
    h = hashlib.sha256()
    for n in names:
        with open(os.path.join(HERE, n), "rb") as f:
            h.update(f.read())
    return h.hexdigest()


def value_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def build_c(src, flags, so=None):
    """The library of a C file beside this module, built when older than its source, renamed into place: two
    processes building at once each load a whole one. Returns its path."""
    src = os.path.join(HERE, src)
    so = so or os.path.join(HERE, "_" + os.path.basename(src)[:-2] + ".so")
    if not os.path.exists(so) or os.path.getmtime(so) < os.path.getmtime(src):
        tmp = f"{so}.{os.getpid()}"
        subprocess.run(["cc", *flags, "-shared", "-fPIC", "-o", tmp, src, "-lm"], check=True)
        os.replace(tmp, so)
    return so


S = dict(P.section(NAME, "brass"))
S.setdefault("cone", None)
if load("gain_db") is not None:  # what the chain measures rather than chooses: the output's gain
    S["gain_db"] = load("gain_db")


def lip_mu(f):
    """The lips' mass per area for a note of f Hz. Fixed, a low note's lips would swing further than any lips do: the swing
    goes as 1 / (mass x f^2). A player's lips are heavier and looser on low notes (mu_p, re mu at mu_f)."""
    return S["mu"] * (S.get("mu_f", 262.0) / f) ** S.get("mu_p", 0.0)


def side(midi):
    return next(s for s in S["sides"] if midi <= s[0])
