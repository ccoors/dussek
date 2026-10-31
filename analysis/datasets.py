"""The sample sets' facts for the analysis and the fits, from the projects
(projects/<name>/project.toml, analysis/project.py): a source's items (analysis/mapping.py),
its data directory, its dynamics and whether its levels are the recorded ones, and the
projects a part borrows from."""
import os

import mapping
import project as P
from mapping import parse_sfz  # noqa: F401  (release_noise.py)


def ids():
    """Every source id (the scripts' choices)."""
    return sorted(P.index()[2])


def items(sid):
    """The source's recordings, as the analysis reads them."""
    return mapping.items(P.source(sid), P.data_root(sid))


def root(sid):
    return P.data_root(sid)


def dynamics(sid):
    """The recorded dynamics: name (pp/mf/ff) → the samples' layer key (`layer_key`), softest first."""
    return P.source(sid)["dynamics"]


def levels(sid):
    """The set's levels: "recorded" (as played), "per_key" (each key scaled by one gain: the steps
    between its dynamics recorded, its level along the keys not) or "normalized" (each file)."""
    return P.source(sid).get("levels")


def levels_recorded(sid):
    """Whether the set keeps the recorded levels along the keys (the fits follow them)."""
    return levels(sid) == "recorded"


def level_steps_recorded(sid):
    """Whether the steps between a key's dynamics are the recorded ones (the felt fit follows them)."""
    return levels(sid) in ("recorded", "per_key")


def _borrowed(pid, role):
    other = P.uses(pid, role)
    return P.main_part(other) if other else None


def level_reference(pid):
    """The part whose recorded levels a normalized set follows ([uses] levels), or None."""
    return _borrowed(pid, "levels")


def felt_reference(pid):
    """The part whose fitted felt the felt fit takes ([uses] felt), or None."""
    return _borrowed(pid, "felt")


def output_gain_db(pid):
    return P.section(pid, "export").get("output_gain_db", 0.0)


def tail_cut(sid):
    return P.source(sid).get("tail_cut_s", 0.0)


def release_sfz(sid):
    """The SFZ file whose release regions are the set's own release samples, or None."""
    r = P.source(sid).get("release") or {}
    return r.get("file") if r.get("kind") == "sfz" else None


def layer_key(sid, s):
    """The key of an analysis sample that `dynamics(sid)` refers to."""
    return s["layer"] if P.source(sid).get("layer_key") == "layer" else s["dynamic"]


def dynamic_of(sid, s):
    """The dynamic name (pp/mf/ff) of an analysis sample, or None."""
    k = layer_key(sid, s)
    return next((d for d, v in dynamics(sid).items() if v == k), None)


def loudest(sid):
    """The loudest recorded dynamic ("ff"; a one-layer set: "mf")."""
    return list(dynamics(sid))[-1]


def sample_path(sid, s):
    """The recording of an analysis sample."""
    return os.path.join(root(sid), s.get("relpath", s["file"]))
