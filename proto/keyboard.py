#!/usr/bin/env python3
"""Render every measured note of a profile with the scale design curves and compare
with the recordings across the keyboard.

Metrics per note:
  - brightness change: rms error of ΔL_n − ΔL_1 (mf−pp and ff−pp), as in fit_hammer.py
  - spectral centroid (in partial numbers) at pp and ff, synth vs measured
  - decay: median ratio synth/measured of per-partial energy-decay-curve rates at ff
    (n ≥ 2, conf > 0.3); ff is rendered as long as the recording (max. 20 s) so that
    both decay fits see the same time window

Usage: keyboard.py [--profile grand_a] [-o proto/out]
"""
import argparse
import os
import sys
import tempfile
from multiprocessing import Pool

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import fit_hammer as FH  # noqa: E402
import scale  # noqa: E402
import soundboard as SB  # noqa: E402

_profile = None


def centroid(s, n_max=20):
    ps = [p for p in s["partials"] if p["n"] <= n_max]
    a = np.array([10 ** (p["level_db"] / 20) for p in ps])
    return float((a * [p["n"] for p in ps]).sum() / a.sum())


def run_note(args):
    dataset, note, ff_duration, use_sb = args
    global _profile
    if _profile is None:
        _profile = scale.Profile(dataset)
    params = scale.note_params(_profile, note)
    out = {}
    for dyn, v in _profile.hammer_speeds().items():
        dur = ff_duration if dyn == FH.D.loudest(dataset) else 4.0
        y, _ = SB.render(_profile, note, v, dur) if use_sb else scale.M.render(params, v, duration=dur)
        fd, path = tempfile.mkstemp(suffix=".wav")
        os.close(fd)
        sf.write(path, (y * 1e-3)[:, None], 48000, subtype="FLOAT")
        out[dyn] = A.analyze({"path": path, "note": note, "layer": 0, "layers": 0,
                              "dynamic": dyn, "tail_cut_s": 0.0})
        os.remove(path)
    return note, out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    ap.add_argument("-o", "--outdir", default=os.path.join(HERE, "out"))
    ap.add_argument("--soundboard", action="store_true", help="render through the identified soundboard")
    args = ap.parse_args()

    prof = scale.Profile(args.profile)
    keys = FH.D.dynamics(args.profile)
    dyns = [d for d in ("pp", "mf", "ff") if d in keys]
    soft = dyns[0]   # the softest recorded dynamic (pp; the upright: mf)
    top = FH.D.loudest(args.profile)   # the loudest (ff; the one-layer harpsichord: mf)
    pairs = [(d, soft) for d in dyns[1:]]
    meas = {}
    for s in prof.samples:
        for dyn, key in keys.items():
            if FH.D.layer_key(args.profile, s) == key:
                meas[(s["note"], dyn)] = s
    notes = sorted({n for n, _ in meas})
    with Pool() as pool:
        jobs = [(args.profile, n, min(meas[(n, top)]["duration_s"], 20.0) if (n, top) in meas else 4.0,
                 args.soundboard) for n in notes]
        synth = dict(pool.map(run_note, jobs))

    rows = []
    for n in notes:
        r = {"note": n}
        errs = []
        for hi, lo in pairs:
            usable = all((n, d) in meas and meas[(n, d)]["B_fit_resid_cents"] < 8   # detection ok
                         and meas[(n, d)]["n_partials_found"] >= 6 for d in (hi, lo))  # enough partials
            if usable and "error" not in synth[n][hi] and "error" not in synth[n][lo]:
                tgt = FH.rel_change(FH.partial_levels(meas[(n, hi)]), FH.partial_levels(meas[(n, lo)]))
                sc = FH.rel_change(FH.partial_levels(synth[n][hi], measured=False),
                                   FH.partial_levels(synth[n][lo], measured=False))
                common = sorted(set(sc) & set(tgt))
                if len(common) >= 2:
                    errs.append(np.sqrt(np.mean([(sc[k] - tgt[k]) ** 2 for k in common])))
        r["bright_err"] = float(np.mean(errs)) if errs else np.nan
        # absolute spectral shape: rms of measured − synth partial levels after removing the
        # per-note mean (recordings are partly normalized per note), averaged over pp/ff
        shape = []
        for dyn in (soft, top):
            if (n, dyn) in meas and "error" not in synth[n][dyn]:
                lm = FH.partial_levels(meas[(n, dyn)])
                ls = FH.partial_levels(synth[n][dyn], measured=False)
                d = np.array([lm[k] - ls[k] for k in sorted(set(lm) & set(ls))])
                if len(d) >= 3:
                    shape.append(np.sqrt(np.mean((d - d.mean()) ** 2)))
        r["shape_err"] = float(np.mean(shape)) if shape else np.nan
        for dyn in (soft, top):
            if (n, dyn) in meas and "error" not in synth[n][dyn]:
                r[f"centroid_{dyn}_meas"] = centroid(meas[(n, dyn)])
                r[f"centroid_{dyn}_synth"] = centroid(synth[n][dyn])
        if (n, top) in meas and "error" not in synth[n][top]:
            def edc(s):
                return {p["n"]: p["edc_decay_db_s"] for p in s["partials"]
                        if p["n"] >= 2 and p.get("edc_decay_db_s", 0) > 0 and p["conf"] > 0.3}
            dm, ds = edc(meas[(n, top)]), edc(synth[n][top])
            common = sorted(set(dm) & set(ds))
            r["decay_ratio"] = float(np.median([ds[k] / dm[k] for k in common])) if len(common) >= 2 else np.nan
        rows.append(r)

    def col(k):
        return np.array([r.get(k, np.nan) for r in rows])
    x = np.array(notes)
    fig, ax = plt.subplots(4, 1, figsize=(12, 13), sharex=True)
    tag = " (with soundboard)" if args.soundboard else " (bridge force)"
    ax[0].bar(x, col("bright_err"), width=2)
    ax[0].set(title=f"{args.profile}{tag}: brightness-change error (rms of ΔL_n − ΔL_1, mf/ff vs pp)", ylabel="dB")
    ax[3].bar(x, col("shape_err"), width=2, color="tab:purple")
    ax[3].set(title="absolute spectral shape error (rms of partial level differences, per-note mean removed; pp/ff)",
              ylabel="dB")
    for dyn, c in ((soft, "tab:blue"), (top, "tab:red")):
        ax[1].plot(x, col(f"centroid_{dyn}_meas"), "s--", color=c, ms=4, label=f"measured {dyn}")
        ax[1].plot(x, col(f"centroid_{dyn}_synth"), "o-", color=c, ms=4, label=f"synth {dyn}")
    ax[1].set(title="spectral centroid of partials 1–20 (amplitude-weighted)", ylabel="partial number")
    ax[1].legend(fontsize=8)
    ax[2].plot(x, col("decay_ratio"), "o-", color="k", ms=4)
    ax[2].axhline(1, color="gray", lw=0.8)
    ax[2].set(title="energy-decay rate synth / measured (ff, median over partials n ≥ 2)", ylabel="ratio", yscale="log")
    ticks = list(range(21, 109, 12))
    for a in ax:
        a.set_xticks(ticks, [f"A{(t - 21) // 12}" for t in ticks])
        a.grid(alpha=0.3)
    fig.tight_layout()
    out = os.path.join(args.outdir, f"keyboard_{args.profile}{'_sb' if args.soundboard else ''}.png")
    fig.savefig(out, dpi=100)
    be = col("bright_err")
    if not pairs:
        print("brightness-change error: n/a (one layer)")
    else:
        print(f"brightness-change error: median {np.nanmedian(be):.1f} dB, worst "
              + ", ".join(f"{notes[i]} ({be[i]:.1f})" for i in np.argsort(-np.nan_to_num(be))[:4]))
    print(f"absolute spectral shape error: median {np.nanmedian(col('shape_err')):.1f} dB")
    dr = col("decay_ratio")
    print(f"decay ratio synth/measured: median {np.nanmedian(dr):.2f}, range {np.nanmin(dr):.2f}–{np.nanmax(dr):.2f}")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
