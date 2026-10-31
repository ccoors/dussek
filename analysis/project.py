"""Instrument projects (projects/<name>/project.toml): the index of projects, their parts and
sources, and their files: a part's fit document (fits/<part>.json), the caches
(cache/<id>/<name>.json) and a source's data directory.

Ids are global: a part's id (today's profile name) and a source's id (today's dataset name)
are unique over all projects, so the code addresses everything by id. While a step runs
under the driver (proto/build.py sets PE_STEP and PE_PROJECT), every read and write through
this module is checked against the step's entry in projects/steps.toml (`_read`, `_write`)."""
import json
import os
import tomllib
from functools import lru_cache

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))


def base():
    """The projects directory (PE_PROJECTS overrides it: the tests' own)."""
    return os.environ.get("PE_PROJECTS") or os.path.join(REPO, "projects")


@lru_cache(maxsize=None)
def _index(root):
    projects, parts, sources = {}, {}, {}
    owner = {}
    for name in sorted(os.listdir(root)):
        path = os.path.join(root, name, "project.toml")
        if not os.path.isfile(path):
            continue
        with open(path, "rb") as f:
            projects[name] = tomllib.load(f)
        ids = [(p["id"], p, None) for p in projects[name].get("part", [])]
        for p in projects[name].get("part", []):
            ids += [(s["id"], p, s) for s in p.get("source", [])]
        for id_, part_, src in ids:
            if owner.get(id_, name) != name:
                raise ValueError(f"id {id_!r} of {name} is already {owner[id_]}'s")
            owner[id_] = name
            if src is None:
                parts[id_] = (name, part_)
            else:
                sources[id_] = (name, part_["id"], src)
    return projects, parts, sources


def index():
    """(projects by name, parts by id: (project, table), sources by id: (project, part, table))."""
    return _index(base())


def config(name):
    """A project's whole project.toml (unchecked: the driver's)."""
    return index()[0][name]


def project_of(id_):
    projects, parts, sources = index()
    if id_ in projects:
        return id_
    if id_ in parts:
        return parts[id_][0]
    if id_ in sources:
        return sources[id_][0]
    raise KeyError(f"no project, part or source {id_!r}")


def part_ids(name):
    return [p["id"] for p in config(name).get("part", [])]


def source_ids(id_):
    """The sources of a project or of a part, in the file's order."""
    projects, parts, _ = index()
    tables = [parts[id_][1]] if id_ in parts else config(id_).get("part", [])
    return [s["id"] for p in tables for s in p.get("source", [])]


def part_of(sid):
    return index()[2][sid][1]


def main_part(name):
    """A project's first part (its main register)."""
    return part_ids(name)[0]


def part(pid):
    _read(pid, "project.part")
    return index()[1][pid][1]


def source(sid):
    _read(sid, "project.sources")
    return index()[2][sid][2]


def level_corrections(sid):
    """A source's level corrections (`[part.source.level_corrections]`; None without), a read of
    its own: the analyses and fits do not see them"""
    _read(sid, "project.level_corrections")
    return index()[2][sid][2].get("level_corrections")


# tables overridden from the command line (export.py --with), by (project, table)
_OVERRIDES = {}


def override(project, table, values):
    """Override keys of a project's top-level table for this process (a variant's export)."""
    _OVERRIDES.setdefault((project, table), {}).update(values)


_USED = set()


def clear_overrides():
    _OVERRIDES.clear()
    _USED.clear()


def unused_overrides():
    """(project, table, key) of the overrides no read took (a mistyped table, one this run never
    reads, or a key a part's own table shadows): a variant that would differ in nothing."""
    return sorted((p, t, k) for (p, t), v in _OVERRIDES.items() for k in v if (p, t, k) not in _USED)


def parse_with(item):
    """`table.key=value` (the value a TOML literal) → (table, key, value); ValueError naming the item."""
    head, eq, value = item.partition("=")
    table, dot, key = head.partition(".")
    if not (eq and dot and table and key and value):
        raise ValueError(f"--with {item!r}: expected table.key=value")
    try:
        v = tomllib.loads(f"v = {value}")["v"]
    except tomllib.TOMLDecodeError as e:
        raise ValueError(f"--with {item!r}: the value is no TOML literal ({e})") from e
    return table, key, v


def section(id_, name):
    """A top-level table of the project `id_` belongs to ({} where it has none), with its command
    line overrides; for a part or its source, the part's own table of that name over it (Harpsichord
    B's 4′: its own design)."""
    _read(id_, f"project.{name}")
    ov = _OVERRIDES.get((project_of(id_), name), {})
    table = {**config(project_of(id_)).get(name, {}), **ov}
    _, parts, sources = index()
    pid = id_ if id_ in parts else sources[id_][1] if id_ in sources else None
    own = parts[pid][1].get(name) if pid else None
    own = own if isinstance(own, dict) else {}
    _USED.update((project_of(id_), name, k) for k in ov if k not in own)
    return {**table, **own} if own else table


def uses(id_, role):
    """The project `id_`'s project borrows `role` from ([uses], with its command line overrides), or None."""
    _read(id_, "project.uses")
    return section(project_of(id_), "uses").get(role)


def provider(pid, role):
    """The part whose data stands for `role`: the borrowed project's main part, else pid."""
    other = uses(pid, role)
    return main_part(other) if other else pid


def project_dir(name):
    return os.path.join(base(), name)


def fit_path(pid):
    return os.path.join(project_dir(project_of(pid)), "fits", f"{pid}.json")


def cache_path(id_, name):
    return os.path.join(project_dir(project_of(id_)), "cache", id_, f"{name}.json")


def data_root(sid):
    return os.path.join(REPO, "data", source(sid)["root"])


def has_recordings(name):
    """Whether a project's recordings are here: every source's folder under data/. Without them (a
    fresh clone) the project lends what is committed of it, as it is."""
    return all(os.path.isdir(data_root(s)) for s in source_ids(name))


def write_json(path, obj, **dump):
    """json.dump to a temporary file, renamed into place: a reader never sees half a file."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = f"{path}.tmp{os.getpid()}"
    try:
        with open(tmp, "w") as f:
            json.dump(obj, f, **dump)
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.remove(tmp)


def has_fit(pid):
    return os.path.exists(fit_path(pid))


def _raw_fit(pid):
    path = fit_path(pid)
    if not os.path.exists(path):
        return {}
    with open(path) as f:
        return json.load(f)


def load_fit(pid):
    """The part's fit document ({} before its first fit)."""
    return _wrap(pid, _raw_fit(pid))


def write_fit(pid, doc):
    """Replace the part's fit document (a step that owns every key: bars.py, fit_buff.py)."""
    for k in doc:
        _write(pid, f"fit.{k}")
    write_json(fit_path(pid), doc, indent=1)


def update_fit(pid, values, drop=()):
    """Remove the keys `drop`, then set the top-level keys `values` (in place: a new key goes
    last, as {**old, **new} did)."""
    for k in (*drop, *values):
        _write(pid, f"fit.{k}")
    doc = _raw_fit(pid)
    for k in drop:
        doc.pop(k, None)
    doc.update(values)
    write_json(fit_path(pid), doc, indent=1)


def has_cache(id_, name):
    return os.path.exists(cache_path(id_, name))


def load_cache(id_, name):
    _read(id_, f"cache.{name}")
    with open(cache_path(id_, name)) as f:
        return json.load(f)


def cache_bytes(id_, name):
    _read(id_, f"cache.{name}")
    with open(cache_path(id_, name), "rb") as f:
        return f.read()


def write_cache(id_, name, obj, **dump):
    _write(id_, f"cache.{name}")
    write_json(cache_path(id_, name), obj, **dump)


class Undeclared(RuntimeError):
    """A step read or wrote what its entry in projects/steps.toml does not declare."""


@lru_cache(maxsize=None)
def registry():
    with open(os.path.join(base(), "steps.toml"), "rb") as f:
        return tomllib.load(f)


def step_entry(instance):
    """A chain instance's registry entry ("register#2" is the step "register")."""
    return registry()["step"][instance.split("#")[0]]


class _Rules:
    def __init__(self, project, entry):
        self.project = project
        # a memo (cache.release_peaks: a cache keyed by its own inputs) any step that needs it rebuilds
        memos = set(entry.get("memos", []))
        self.reads = set(entry.get("reads", [])) | set(entry.get("seeds", [])) | memos
        self.writes = set(entry.get("owns", [])) | set(entry.get("amends", [])) | memos


@lru_cache(maxsize=None)
def _rules_for(step, project):
    return _Rules(project, step_entry(step))


def _rules():
    step = os.environ.get("PE_STEP")
    return _rules_for(step, os.environ["PE_PROJECT"]) if step else None


def _as(id_, what, project):
    """`what` of id_ as the registry names it from `project`: its own ("fit.x"), or another
    project's under each role `project` borrows it as ("uses.dampers:fit.x")."""
    other = project_of(id_)
    if other == project:
        return [what]
    return [f"uses.{r}:{what}" for r, p in config(project).get("uses", {}).items() if p == other]


#: reads a wildcard does not cover: their fingerprints leave them out (proto/build.py), so a step
#: reading them names them
BY_NAME = ("project.level_corrections",)


def _declared(names, declared):
    for n in names:
        prefix, _, what = n.rpartition(":")
        kind = what.split(".")[0]
        lead = f"{prefix}:" if prefix else ""
        if n in declared:
            return True
        if what in BY_NAME:
            continue
        if f"{lead}{kind}.*" in declared or (prefix and f"{prefix}:*" in declared):
            return True
    return False


def _log(op, id_, what):
    project = os.environ.get("PE_PROJECT")
    row = {"step": os.environ.get("PE_STEP"), "project": project, "op": op, "id": id_, "what": what,
           "as": _as(id_, what, project) if project else [what]}
    with open(os.environ["PE_RECORD"], "a") as f:
        f.write(json.dumps(row) + "\n")


def _read(id_, what):
    if os.environ.get("PE_RECORD"):
        return _log("read", id_, what)
    rules = _rules()
    if rules and not _declared(_as(id_, what, rules.project), rules.reads | rules.writes):
        raise Undeclared(f"{os.environ['PE_STEP']} reads {what} of {id_} ({project_of(id_)}): "
                         f"not declared in projects/steps.toml")


def _write(id_, what):
    if os.environ.get("PE_RECORD"):
        return _log("write", id_, what)
    rules = _rules()
    if rules and (project_of(id_) != rules.project or what not in rules.writes):
        raise Undeclared(f"{os.environ['PE_STEP']} writes {what} of {id_} ({project_of(id_)}): "
                         f"not declared in projects/steps.toml")


class FitDoc(dict):
    """A fit document whose reads go through the checks (top-level keys)."""

    def __init__(self, pid, doc):
        super().__init__(doc)
        self.pid = pid

    def __getitem__(self, k):
        _read(self.pid, f"fit.{k}")
        return super().__getitem__(k)

    def get(self, k, default=None):
        _read(self.pid, f"fit.{k}")
        return super().get(k, default)

    def __contains__(self, k):
        _read(self.pid, f"fit.{k}")
        return super().__contains__(k)

    def _all(self):
        _read(self.pid, "fit.*")

    def keys(self):
        self._all()
        return super().keys()

    def items(self):
        self._all()
        return super().items()

    def values(self):
        self._all()
        return super().values()

    def __iter__(self):
        self._all()
        return super().__iter__()

    def __reduce__(self):
        # (dict.copy would go through the overridden keys(): a checked read of every key)
        return (FitDoc, (self.pid, {k: dict.__getitem__(self, k) for k in dict.keys(self)}))


def _wrap(pid, doc):
    return FitDoc(pid, doc) if os.environ.get("PE_STEP") or os.environ.get("PE_RECORD") else doc
