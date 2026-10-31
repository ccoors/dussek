"""Release noise: the mechanical sound of a key coming back up (key return, damper
landing, hammer settling), synthesized from per-key statistics measured in the
Salamander release samples (analysis/release_noise.py), triggered at key-up as there.

Per key: 1–5 impacts (time after key-up, level, exponential decay), each a noise burst
with a 1 ms soft onset; the sum is shaped with the key's measured 1/3-octave spectrum
(minimum phase) and given an AB pair's coherence; a different noise per key (seeded),
so every key sounds different but the same each time. This is a resynthesis from
statistics, not a playback of the recording.

Level: re the note's peak, ratio_db per dynamic from the sampler's mix (pp −20,
mf −30, ff −38 dB in the median), interpolated over log hammer speed. The note's peak
is a reference table (radiated peak of the note at the profile's pp/mf/ff speeds,
peak_table), not the peak of the actual render: a real-time engine has to know the
level at key-up. Iowa has no release recordings and uses the same keys.
"""
import ast
import hashlib
import json
import os
from functools import lru_cache
from multiprocessing import Pool

import numpy as np
import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analysis"))
import project as P  # noqa: E402

import soundboard as SB  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
RISE_S = 0.001
LENGTH_S = 0.6


@lru_cache(maxsize=4)
def _data(dataset):
    # its own release samples where analyzed, else the set it borrows them from ([uses] release)
    d = P.load_cache(P.provider(dataset, "release"), "release")
    return np.array(d["bands_hz"]), {k["note"]: k for k in d["keys"]}


PEAK_RENDER_S = 1.0
PEAK_SLOTS = 4      # the release peaks' tables kept per part (the newest)


def _peak(args):
    import scale
    dataset, note, v, core, *rest = args       # (…, felt) where the export plays one
    os.environ["SOUNDBOARD_MODE"] = "full"
    y, _ = SB.render(scale.Profile(dataset), note, v, PEAK_RENDER_S, stereo=True, knock=False, core=core,
                     felt=rest[0] if rest else None)
    return float(np.abs(y).max())


@lru_cache(maxsize=1)
def _render_sources():
    """The repository's modules a peak render runs: soundboard and scale and what they import from
    proto/ and analysis/, followed through (the files, sorted)."""
    dirs = [HERE, os.path.join(HERE, "..", "analysis")]
    found, todo = set(), ["soundboard", "scale", "release"]
    while todo:
        name = todo.pop()
        path = next((os.path.normpath(os.path.join(d, name + ".py")) for d in dirs
                     if os.path.isfile(os.path.join(d, name + ".py"))), None)
        if path is None or path in found:
            continue
        found.add(path)
        for node in ast.walk(ast.parse(open(path).read())):
            if isinstance(node, ast.Import):
                todo += [a.name.split(".")[0] for a in node.names]
            elif isinstance(node, ast.ImportFrom) and node.module and not node.level:
                todo.append(node.module.split(".")[0])
    return sorted(found)


@lru_cache(maxsize=1)
def _code_digest():
    h = hashlib.sha1()
    for f in _render_sources():
        h.update(os.path.basename(f).encode() + open(f, "rb").read())
    return h.digest()


def _peak_key(dataset, core=None, felt=None):
    """Fingerprint of what the peaks depend on: the analysis and the body bands, the fitted profile
    (without the pedal block, which pedal.py writes after reading the table), the project's tables,
    the felt's core the export plays and the code that renders them. A cache without the fit went
    stale when a fit changed the profile; without the code and the project, when the model or a
    table the fit never sees changed (step 200)."""
    h = hashlib.sha1(P.cache_bytes(dataset, "analysis"))
    if P.has_cache(dataset, "body"):
        h.update(P.cache_bytes(dataset, "body"))
    h.update(_code_digest())
    h.update(json.dumps(P.config(P.project_of(dataset)), sort_keys=True, default=str).encode())
    if core:
        h.update(json.dumps(core, sort_keys=True).encode())
    if felt:
        h.update(json.dumps({"felt": felt}, sort_keys=True).encode())
    if P.has_fit(dataset):
        prof = {k: v for k, v in P.load_fit(dataset).items() if k != "pedal"}
        h.update(json.dumps(prof, sort_keys=True).encode())
    return h.hexdigest()


def peak_table(profile, core=None, felt=None):
    """{note: {dyn: radiated peak}} for the profile's pp/mf/ff speeds, with the felt's core of a
    [felt_core] table and the felt's exponent of a [felt] table where the export plays them (cached in the part's release_peaks cache,
    recomputed when the profile or the core changed)."""
    key = _peak_key(profile.dataset, core, felt)
    # a table per key: the export asks with the core, the pedal fit and the tests without, and one
    # slot made each redo the other's 264 renders
    tables = {}
    if P.has_cache(profile.dataset, "release_peaks"):
        cached = P.load_cache(profile.dataset, "release_peaks")
        tables = cached.get("tables", {cached["key"]: cached["table"]} if "key" in cached else {})
        if key in tables:
            return {int(n): v for n, v in tables[key].items()}
    speeds = profile.hammer_speeds()
    jobs = [(profile.dataset, n, speeds[d], core, felt) for n in range(21, 109) for d in ("pp", "mf", "ff")]
    with Pool() as pool:
        peaks = pool.map(_peak, jobs)
    table = {}
    for (_, n, _, _, _), (d, pk) in zip(jobs, zip(("pp", "mf", "ff") * 88, peaks)):
        table.setdefault(n, {})[d] = pk
    tables = {**{k: v for k, v in list(tables.items())[-(PEAK_SLOTS - 1):]}, key: table}
    P.write_cache(profile.dataset, "release_peaks", {"tables": tables})
    return table


def level(profile, table, note, speed):
    """Peak level of the release noise for a note struck at speed (m/s)."""
    speeds = profile.hammer_speeds()
    pts = sorted((np.log(speeds[d]), np.log(table[note][d])) for d in ("pp", "mf", "ff"))
    peak = np.exp(np.interp(np.log(speed), [p for p, _ in pts], [q for _, q in pts]))
    return float(peak * 10 ** (ratio_db(note, speed, speeds, profile.dataset) / 20))


def ratio_db(note, speed, speeds, dataset):
    """Release peak re the note's peak (dB) at hammer speed (m/s), given the profile's
    pp/mf/ff speeds (the dynamics the release data has: one point is held)."""
    key = _data(dataset)[1][note]
    pts = sorted((np.log(speeds[d]), r) for d, r in key["ratio_db"].items())
    return float(np.interp(np.log(speed), [p for p, _ in pts], [r for _, r in pts]))


def burst(note, sr, dataset):
    """Release noise of one key, stereo (2, n), peak 1."""
    bands_hz, keys = _data(dataset)
    key = keys[note]
    return noise_burst(key["impacts"], key["band_db"], bands_hz, 5000 + note, LENGTH_S, sr)


def noise_burst(impacts, band_db, bands_hz, seed, length_s, sr=48000):
    """Mechanical noise resynthesized from its statistics: impacts (t0 s, level dB,
    decay s) shape the envelope of stereo noise with an AB pair's coherence, filtered to
    the 1/3-octave band levels band_db; (2, n), peak 1. Also used for the pedal noise."""
    bands_hz = np.asarray(bands_hz)
    n = int(length_s * sr)
    rng = np.random.default_rng(seed)
    t = np.arange(n) / sr
    env = np.zeros(n)
    for t0, lv, tau in impacts:
        dt = t - t0
        env += np.where(dt >= 0, 10 ** (lv / 20) * np.sin(0.5 * np.pi * np.clip(dt / RISE_S, 0, 1)) ** 2
                        * np.exp(-np.maximum(dt, 0) / max(tau, 1e-3)), 0)
    noise = rng.standard_normal((2, n))
    f = np.fft.rfftfreq(n, 1 / sr)
    coh = np.sinc(2 * f * SB.MIC_SPACING / 343.0)
    spec = np.fft.rfft(noise, axis=1)
    noise = np.fft.irfft(np.stack([spec[0], coh * spec[0] + np.sqrt(1 - coh ** 2) * spec[1]]), n, axis=1)
    x = noise * env
    nf = 4096
    # 1/3-octave band powers → power density (bandwidth ∝ f); its dB value is the
    # filter magnitude in dB
    psd_db = np.array(band_db) - 10 * np.log10(bands_hz / bands_hz[0])
    shape_db = np.interp(np.log2(np.clip(np.fft.rfftfreq(nf, 1 / sr), 1, None)), np.log2(bands_hz), psd_db)
    h = SB.min_phase(shape_db, nf)
    y = np.stack([np.convolve(c, h)[:n] for c in x])
    return y / np.abs(y).max()
