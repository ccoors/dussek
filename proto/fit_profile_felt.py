#!/usr/bin/env python3
"""Joint felt fit for one instrument profile: one set of hammer speeds (pp/mf/ff) for
all notes (same player, same session), felt stiffness per support note.

Per note and speed set, the error is the rms of ΔL_n − ΔL_1 (as in fit_hammer.py);
the speed set with the lowest total error over all support notes wins. The per-note
stiffness is then fitted by a smooth curve (log2 F05 linear in the note, plus a bass
factor below the bass break) and written to the part's fit document, which
scale.py reads.

Usage: fit_profile_felt.py --profile grand_b
"""
import argparse
import itertools
import json
import os
from multiprocessing import Pool

import numpy as np

import fit_hammer as FH
import scale

HERE = os.path.dirname(os.path.abspath(__file__))
SUPPORT_NOTES = [24, 30, 36, 42, 48, 54, 60, 66, 72, 78, 84, 90]
# sets that keep the steps between their dynamics: further support notes in the top treble,
# fitted to the level step
TREBLE_SUPPORT = [96, 102]
TREBLE_LEVEL_ONLY = 84
P_EXP = 2.3
F05_GRID = [2 ** (i / 2) for i in range(-2, 15)]          # 0.5 … 128 N
# ff up to 14 m/s: Grand K's top layer (velocity 107–127) is 25–40 dB above its pp, and
# the fit sat at 10 m/s
SPEEDS = {"pp": [0.2, 0.3, 0.4, 0.6], "mf": [0.5, 0.8, 1.1, 1.5, 2.1, 3.0], "ff": [2.5, 3.5, 5.0, 7.0, 10.0, 14.0]}
# MIDI velocities the export maps pp/mf/ff to (export.py "velocity")
VEL_MIDI = {"pp": 14, "mf": 60, "ff": 124}


def dynamics(profile):
    """The profile's recorded dynamics in order, and the level pairs compared (each
    against the softest)."""
    dyns = [d for d in ("pp", "mf", "ff") if d in FH.D.dynamics(profile)]
    return dyns, [(d, dyns[0]) for d in dyns[1:]]


def measured_targets(profile, note):
    samples = FH.P.load_cache(profile, "analysis")["samples"]
    keys = FH.D.dynamics(profile)
    meas = {}
    for s in samples:
        if s["note"] != note or "error" in s or "label_mismatch" in s:
            continue
        if s["B_fit_resid_cents"] >= 8 or s["n_partials_found"] < 6:
            continue
        for dyn, key in keys.items():
            if FH.D.layer_key(profile, s) == key:
                meas[dyn] = FH.partial_levels(s)
    return {(hi, lo): FH.rel_change(meas[hi], meas[lo])
            for hi, lo in dynamics(profile)[1] if hi in meas and lo in meas}


def level_targets(profile, notes, pairs):
    """Measured change of the fundamental's level between dynamics (dB) per note: running
    median over ±5 semitones (the player's scatter between notes is several dB)."""
    samples = FH.P.load_cache(profile, "analysis")["samples"]
    lv = {}
    for s in samples:
        if "error" in s or "label_mismatch" in s:
            continue
        d = FH.D.dynamic_of(profile, s)
        p1 = next((p for p in s["partials"] if p["n"] == 1 and p["range_db"] >= FH.MIN_RANGE_DB), None)
        if d and p1:
            lv.setdefault(s["note"], {})[d] = p1["level_db"] + s["peak_dbfs"]
    out = {}
    for n in notes:
        out[n] = {}
        for hi, lo in pairs:
            near = [v[hi] - v[lo] for m, v in lv.items() if abs(m - n) <= 5 and hi in v and lo in v]
            if near:
                out[n][(hi, lo)] = float(np.median(near))
    return out


CURVE_SMOOTH = 6.0     # semitones (Gaussian width) of the anchor curve
# sets that keep the straight line ([strings] line_only: Upright Y's support felts zig-zag)
def line_only(profile):
    return scale.strings(profile).get("line_only", False)


# … and they pick their speeds by the ff−mf level step among the sets within SPEED_TIE_DB of the
# best: Upright Y's error landscape is flat (5.7–6.2 dB from mf/ff 3.0/5.0 to 1.5/5.0),
# and the best set gave a 4 dB ff−mf step, recorded ~11 dB
SPEED_TIE_DB = 0.2
CURVE_MAX_ERR = 5.0    # dB: a support note whose best felt misses its target by more says nothing


def anchor_curve(per_note):
    """Felt curve through the support notes: log2 F05 smoothed over the keyboard (local
    linear, CURVE_SMOOTH), held beyond the outermost support notes. Used where the felt is
    not one straight line: the upright's mid felt is flat (~4 N), its top treble hard
    (22 N at F6, the grid's 128 N at B6); the Theil–Sen line through all of them still
    fell towards the treble. Grid-edge values stand for the edge (a bound, conservative);
    notes with an error above CURVE_MAX_ERR (an unreachable target) are left out."""
    pts = {n: f for n, (e, f) in per_note.items() if e <= CURVE_MAX_ERR}
    ns = sorted(pts)
    at = scale._smooth(ns, [np.log2(pts[n]) for n in ns], np.ones(len(ns)), frac=CURVE_SMOOTH)
    grid = list(range(ns[0], ns[-1] + 1, 3))
    return {"notes": grid, "f05": [float(2 ** at(n)) for n in grid]}


def smooth_curve(f05_by_note):
    """Robust smooth curve: log2 F05 linear in the note (Theil–Sen slope, median
    intercept) above the bass break, plus a bass factor below it. Values at the edge of
    the grid are excluded: they only say "beyond the grid", not where."""
    edge = (min(F05_GRID), max(F05_GRID))
    pts = {n: np.log2(f) for n, f in f05_by_note.items() if f not in edge}
    ns = np.array([n for n in pts if n >= 30], float)
    lf = np.array([pts[int(n)] for n in ns])
    slopes = [(lf[j] - lf[i]) / (ns[j] - ns[i]) for i in range(len(ns)) for j in range(i + 1, len(ns))]
    a = float(np.median(slopes))
    b = float(np.median(lf - a * (ns - 60)))
    below = [2 ** (pts[n] - (a * (n - 60) + b)) for n in pts if n < 30]
    return {"felt_f05_at_c4": float(2 ** b), "felt_log2_slope_per_semitone": a,
            "felt_bass_factor": float(np.median(below)) if below else 1.0,
            "felt_excluded_at_grid_edge": sorted(int(n) for n, f in f05_by_note.items() if f in edge)}


def print_curve(c):
    a = c["felt_log2_slope_per_semitone"]
    print(f"curve: F05 = {c['felt_f05_at_c4']:.2f} N · 2^({a:.4f}·(note−60)), doubling every "
          f"{(1 / a if a > 0 else float('inf')):.1f} semitones; bass factor {c['felt_bass_factor']:.2f}; "
          f"excluded (grid edge): {c['felt_excluded_at_grid_edge']}")


def speed_sets(profile, dyns):
    """The hammer speed sets to search: all combinations of SPEEDS, or for a normalized set
    with a reference ([uses] levels) the reference's fitted speeds alone.
    Without levels the fit trades speed against felt hardness: Grand E's ff fell to
    3.5 m/s (Iowa, the same model: 7.0), its soundboard tilted 35 dB to make up for it, and
    whatever adds highs of its own (phantom partials, hammer wear) came out ~10 dB too
    strong. Fixed at Iowa's, the felt error fell (6.7 → 5.4 dB) and H matched Iowa's."""
    ref = FH.D.level_reference(profile)
    if ref:
        speeds = FH.P.load_fit(ref)["hammer_speeds"]
        return [tuple(speeds[d] for d in dyns)]
    return list(itertools.product(*(SPEEDS[d] for d in dyns)))


FELT_KEYS = ("felt_p", "felt_f05_at_c4", "felt_log2_slope_per_semitone", "felt_bass_factor",
             "felt_excluded_at_grid_edge", "felt_curve")


def reference_felts(profile, notes):
    """With a felt reference ([uses] felt): per note the grid felt nearest the
    reference profile's fitted one (the only felt the fit then tries), else None."""
    ref = FH.D.felt_reference(profile)
    if ref is None:
        return None
    prof = scale.Profile(ref, board=False)
    return {n: min(F05_GRID, key=lambda f: abs(np.log(f) - np.log(prof.felt(n)[1]))) for n in notes}


def level_steps_from(profile):
    """The set whose recorded steps between dynamics the felt fit follows: its own where it keeps
    them, else its levels reference's ([uses] levels: a normalized set follows the same model's
    recordings), else None. Grand E's own fit, without them, put its treble pp 8–24 dB further
    under its mf than Grand B's recordings."""
    if FH.D.level_steps_recorded(profile):
        return profile
    return FH.D.level_reference(profile)


def wanted_support(profile):
    """The support keys: every 6th, and the top treble's where there are recorded steps between
    the dynamics to follow (fitted to the level step)."""
    return SUPPORT_NOTES + (TREBLE_SUPPORT if level_steps_from(profile) else [])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    ap.add_argument("--curve-only", action="store_true",
                    help="recompute the smooth curve from the stored per-note values")
    args = ap.parse_args()
    if args.curve_only:
        out = dict(FH.P.load_fit(args.profile))
        out.update(smooth_curve({int(n): v["f05"] for n, v in out["support"].items()}))
        out.pop("felt_curve", None)
        if level_steps_from(args.profile) and not line_only(args.profile):
            out["felt_curve"] = anchor_curve({int(n): (v["err_db"], v["f05"]) for n, v in out["support"].items()})
        FH.P.update_fit(args.profile, out, drop=("felt_curve",))
        print_curve(out)
        return

    dyns, pairs = dynamics(args.profile)
    v_sets = speed_sets(args.profile, dyns)
    # the support notes, or the nearest recorded ones (sets sampled in whole tones)
    recorded = sorted({s["note"] for s in FH.P.load_cache(args.profile, "analysis")["samples"]})
    wanted = wanted_support(args.profile)
    support = sorted({min(recorded, key=lambda r: abs(r - n)) for n in wanted})
    targets = {n: measured_targets(args.profile, n) for n in support}
    # without pp in the top treble (Grand K above C6), the level step ff−mf
    extra = [(dyns[-1], dyns[-2])] if len(dyns) == 3 else []
    steps_from = level_steps_from(args.profile)
    levels_db = level_targets(steps_from, support, pairs + extra) if steps_from else {}

    def level_pairs(n):
        have = levels_db.get(n, {})
        return [p for p in pairs if p in have] or [p for p in extra if p in have]
    # notes with the spectral targets; in the top treble (too few partials for them) the
    # level step alone where the set keeps its levels: without it the upright's treble
    # felt was extrapolated soft enough (~1 N) that mf barely excited the strings (the
    # ff−mf step 20+ dB instead of the recorded ~12, a dull, weak treble at mf)
    # (a note with some of the dynamics pairs counts: Grand K's bass lacks a clean pp)
    notes = [n for n in support if targets[n] or (n >= TREBLE_LEVEL_ONLY and level_pairs(n))]
    speeds = sorted({v for vs in v_sets for v in vs})
    fixed = reference_felts(args.profile, notes)      # a felt reference: its felt alone
    grid = (lambda n: [fixed[n]]) if fixed else (lambda n: F05_GRID)
    jobs = [(args.profile, n, P_EXP, f, v) for n in notes for f in grid(n) for v in speeds]
    print(f"{args.profile}: {len(notes)} support notes, {len(jobs)} renders")
    with Pool() as pool:
        levels = dict(zip([(n, f, v) for _, n, _, f, v in jobs],
                          (lv for _, lv in pool.map(FH.render_levels, jobs))))

    def note_error(n, f, vs):
        synth = dict(zip(dyns, (levels[(n, f, v)] for v in vs)))
        errs = []
        for hi, lo in pairs:
            tgt = targets[n].get((hi, lo), {})
            sc = FH.rel_change(synth[hi], synth[lo])
            common = sorted(set(sc) & set(tgt))
            if len(common) >= 2:
                errs.append(np.sqrt(np.mean([(sc[k] - tgt[k]) ** 2 for k in common])))
        # the fundamental's own level change, where the recorded levels are kept
        for hi, lo in level_pairs(n):
            m = levels_db.get(n, {}).get((hi, lo))
            if m is not None and 1 in synth[hi] and 1 in synth[lo]:
                errs.append(abs(synth[hi][1] - synth[lo][1] - m))
        return np.mean(errs) if errs else np.inf

    best = None
    cands = []
    for vs in v_sets:
        spectral = [n for n in notes if targets[n]]
        per_note = {n: min((note_error(n, f, vs), f) for f in grid(n)) for n in spectral}
        # level-only treble notes: a felt soft enough to choke both dynamics can give the
        # same step as a hard one (Iowa C7: 1 N); treble hammers are not softer than the
        # middle's, so only felts at least the median of the spectral fits from C4 up
        floor = np.median([f for n, (_, f) in per_note.items() if n >= 60] or [min(F05_GRID)])
        for n in notes:
            if n not in per_note:
                per_note[n] = min((note_error(n, f, vs), f) for f in grid(n) if fixed or f >= floor)
        if not all(np.isfinite(e) for e, _ in per_note.values()):
            raise RuntimeError(f"no comparable partials for some notes: {per_note}")
        total = np.mean([e for e, _ in per_note.values()])
        cands.append((total, vs, per_note))
        if best is None or total < best[0]:
            best = (total, vs, per_note)
    if line_only(args.profile) and len(dyns) == 3:
        hi, lo = dyns[-1], dyns[-2]
        rec = np.median([v[(hi, lo)] for v in levels_db.values() if (hi, lo) in v])

        def step(c):
            sp = dict(zip(dyns, c[1]))
            return np.median([levels[(n, f, sp[hi])][1] - levels[(n, f, sp[lo])][1]
                              for n, (_, f) in c[2].items()
                              if 1 in levels[(n, f, sp[hi])] and 1 in levels[(n, f, sp[lo])]])
        ties = [c for c in cands if c[0] <= best[0] + SPEED_TIE_DB]
        best = min(ties, key=lambda c: abs(step(c) - rec))
        print(f"{len(ties)} speed sets within {SPEED_TIE_DB} dB; {hi}−{lo} step recorded {rec:.1f} dB, "
              f"chosen {step(best):.1f} dB")
    total, vs, per_note = best
    speeds = dict(zip(dyns, vs))
    extrapolated = [d for d in VEL_MIDI if d not in speeds]
    for d in extrapolated:
        # log-linear in the MIDI velocity through the two recorded dynamics
        (d0, v0), (d1, v1) = sorted(speeds.items(), key=lambda kv: VEL_MIDI[kv[0]])[:2]
        t = (VEL_MIDI[d] - VEL_MIDI[d0]) / (VEL_MIDI[d1] - VEL_MIDI[d0])
        speeds[d] = float(np.exp(np.log(v0) + t * (np.log(v1) - np.log(v0))))
    print(f"best speeds " + ", ".join(f"{d} {speeds[d]:.2f}" + (" (extrapolated)" if d in extrapolated else "")
                                    for d in VEL_MIDI) + f" m/s, mean error {total:.1f} dB")
    for n, (e, f) in per_note.items():
        print(f"  note {n}: F05 {f:6.1f} N  ({e:.1f} dB)")

    out_curve = smooth_curve({n: f for n, (_, f) in per_note.items()})
    if fixed:            # the reference's felt itself, not a curve through its grid steps
        ref = FH.P.load_fit(FH.D.felt_reference(args.profile))
        out_curve = {k: ref[k] for k in FELT_KEYS if k in ref}
    elif steps_from and not line_only(args.profile):
        # the treble's level-step notes make the felt bend: the anchor curve (scale.py
        # Profile.felt prefers it)
        out_curve["felt_curve"] = anchor_curve(per_note)
    out = {"profile": args.profile, "hammer_speeds": {d: speeds[d] for d in VEL_MIDI},
           **({"hammer_speeds_extrapolated": extrapolated} if extrapolated else {}),
           "felt_p": P_EXP, **out_curve,
           "support": {str(n): {"f05": f, "err_db": float(e)} for n, (e, f) in per_note.items()},
           "mean_error_db": float(total)}
    # keep the other blocks (soundboard, coupling, knock, …); they depend on the felt,
    # so rerun them afterwards
    FH.P.update_fit(args.profile, out, drop=("hammer_speeds_extrapolated", "felt_excluded_at_grid_edge", "felt_curve"))
    print_curve(out)
    print(f"wrote {FH.P.fit_path(args.profile)}")


if __name__ == "__main__":
    main()
