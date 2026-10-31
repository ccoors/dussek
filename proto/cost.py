#!/usr/bin/env python3
"""Real-time cost estimate of the model, per component, as a per-sample implementation
would run it (Phase 3 planning).

  strings     every eigenmode (partial × string × polarization) is a complex one-pole
              recursion after the hammer contact: 1 complex multiply-add per sample.
              Counted in full, and pruned: modes whose initial bridge-force amplitude
              is below PRUNE_DB re the note's strongest mode (at mf) are left out.
  contact     the hammer contact (1–5 ms) as real 2nd-order resonators, per note-on
  resonance   the sympathetic bank: one complex one-pole per partial (unisons merged,
              up to resonance.F_MAX) of every free string
  radiation   body (0.5 s) and room (2 s) impulse responses, 2 inputs (polarizations) ×
              2 outputs, partitioned FFT convolution
  extras      knock and release noise: precomputed per-key buffers, negligible

1 complex multiply-add (cMAC) = 4 mul + 4 add = 8 flops.

Usage: cost.py [--profile grand_a]
"""
import argparse

import numpy as np

import resonance
import scale

SR = 48000
PRUNE_DB = -80.0


def note_modes(prof, note, v):
    p = scale.note_params(prof, note)
    fm = scale.M.free_modes(p, v, SR)
    w = np.abs(fm["weight"]).ravel()
    full = w.size
    kept = int((20 * np.log10(w / w.max() + 1e-30) > PRUNE_DB).sum())
    k, wk, _ = scale.M.mode_table(p, SR)
    return full, kept, int(np.count_nonzero(fm["force"])), fm["K"] * fm["S"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True)
    args = ap.parse_args()
    prof = scale.Profile(args.profile)
    v = prof.hammer_speeds()["mf"]
    rows = {n: note_modes(prof, n, v) for n in range(21, 109)}
    print(f"{args.profile}: string eigenmodes per note (complex one-pole each), full / pruned at {PRUNE_DB:.0f} dB")
    for n in (21, 28, 36, 48, 60, 72, 84, 96, 108):
        full, kept, contact, real = rows[n]
        print(f"  note {n:3d}: {full:5d} / {kept:5d}   contact {contact:3d} samples × {real:4d} real resonators")
    full = np.array([r[0] for r in rows.values()])
    kept = np.array([r[1] for r in rows.values()])
    print(f"  all 88 notes: {full.sum()} / {kept.sum()}; mean per note {full.mean():.0f} / {kept.mean():.0f}")

    sym = sum(len(resonance.note_modes(prof, n)[0]) for n in range(21, 109))
    undamped = sum(len(resonance.note_modes(prof, n)[0]) for n in range(89, 109))
    print(f"sympathetic bank (≤ {resonance.F_MAX:.0f} Hz, unisons merged): {sym} resonators with the pedal down, "
          f"{undamped} for the undamped top strings alone")

    gflop = lambda cmacs: cmacs * 8 * SR / 1e9  # noqa: E731
    print("\nload at 48 kHz (1 cMAC/sample per resonator = 8 flops):")
    for name, voices in (("8 voices, middle register", [60, 64, 67, 72, 48, 55, 76, 79]),
                         ("16 voices incl. bass, pedal", [28, 33, 36, 40, 43, 48, 52, 55, 60, 64, 67, 72, 76, 79, 84, 88]),
                         ("32 voices, dense pedalled passage", list(range(24, 88, 2)))):
        f = sum(rows[n][0] for n in voices)
        k = sum(rows[n][1] for n in voices)
        print(f"  {name:34s} strings full {gflop(f):5.2f} GFLOP/s, pruned {gflop(k):5.2f} GFLOP/s")
    print(f"  sympathetic bank, pedal down      {gflop(sym):5.2f} GFLOP/s; pedal up {gflop(undamped):5.2f} GFLOP/s")
    taps = int(2.5 * SR)
    print(f"  radiation: 4 IRs × {taps} taps (body+room, 2 pol × 2 ch); direct FIR would be "
          f"{4 * taps * 2 * SR / 1e9:.0f} GFLOP/s, partitioned FFT convolution ≈ 0.1–0.3 GFLOP/s")


if __name__ == "__main__":
    main()
