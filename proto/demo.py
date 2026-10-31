#!/usr/bin/env python3
"""Render a listening demo: an ascending run over the keyboard at mf, chords at pp, mf
and ff, C2/C4/C6 held for 6 s at ff, then the instrument behavior: a silently held C3
under staccato C4 and G4 (sympathetic resonance), an arpeggio with the sustain pedal
and the same without.

Key releases engage the dampers (damper.py) and make the release noise (release.py);
with the pedal down or the key held the strings stay free, and all free strings
resonate sympathetically (resonance.py).

Usage: demo.py [--profile grand_a] [--soundboard full] [--no-knock] [--no-resonance]
"""
import argparse
import json
import os
from multiprocessing import Pool

import numpy as np
import soundfile as sf

import damper
import release
import resonance
import scale
import project as P  # noqa: E402  (on the path through scale)
import soundboard as SB

SR = 48000
HERE = os.path.dirname(os.path.abspath(__file__))
TAIL_S = 3.0          # rendered after the damper falls
UNDAMPED_S = 5.0      # rendered length for notes without dampers
_profile = None


def demo_events():
    """Key events [(start s, note, dynamic or None for a silent key press, hold s)],
    pedal intervals [(down s, up s)], total length."""
    events, pedal = [], []
    run = list(range(21, 109, 3))
    events += [(i * 0.28, n, "mf", 1.2) for i, n in enumerate(run)]
    t = len(run) * 0.28 + 1.0
    for dyn in ("pp", "mf", "ff"):                                   # C major chords
        events += [(t, n, dyn, 2.5) for n in (36, 48, 52, 55, 60, 64, 67, 72)]
        t += 3.0
    for n in (36, 60, 84):                                          # long notes: aftersound
        events.append((t, n, "ff", 6.0))
        t += 6.5
    events.append((t, 48, None, 4.0))                               # silent C3 …
    events += [(t + 0.5, 60, "ff", 0.15), (t + 2.0, 67, "ff", 0.15)]  # … under staccato C4, G4
    t += 5.0
    arp = [36, 43, 48, 52, 55, 60, 64, 67, 72]
    pedal.append((t - 0.05, t + 4.5))                               # arpeggio with pedal
    events += [(t + 0.25 * i, n, "mf", 0.2) for i, n in enumerate(arp)]
    t += 6.0
    events += [(t + 0.25 * i, n, "mf", 0.2) for i, n in enumerate(arp)]   # … and without
    t += 4.0
    return events, pedal, t + 1.0


def free_intervals(note, events, pedal):
    """Intervals in which the strings of a note are free (key down or pedal down)."""
    if not damper.has_damper(note):
        return [(0.0, np.inf)]
    iv = sorted([(t, t + h) for t, n, _, h in events if n == note] + list(pedal))
    merged = []
    for a, b in iv:
        if merged and a <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], b))
        else:
            merged.append((a, b))
    return merged


def damper_schedule(start, free):
    """[(t from the strike, on)] for a note struck at start: the damper falls at the end
    of the free interval (key or pedal down) that contains the strike; returns that time
    too (inf without damper). Later liftings are ignored: by then the note has decayed
    by ≥ TAIL_S of damping and its render has ended."""
    for a, b in free:
        if a <= start < b:
            return ([(b - start, True)], b - start) if np.isfinite(b) else ([], np.inf)
    return [(0.0, True)], 0.0


def render_note(args):
    dataset, note, velocity, dampers, duration, sb_mode, knock = args
    global _profile
    if _profile is None:
        _profile = scale.Profile(dataset)
    if sb_mode == "none":
        y, info = scale.M.render(scale.note_params(_profile, note), velocity, sr=SR, duration=duration,
                                 dampers=dampers, damper_sigma=lambda f: damper.sigma(note, f, dataset))
        return np.stack([y, y]), y
    os.environ["SOUNDBOARD_MODE"] = sb_mode
    y, info = SB.render(_profile, note, velocity, duration, sr=SR, stereo=True, knock=knock, dampers=dampers)
    return y, info["bridge_force"]


def resonance_schedules(events, pedal, t_end, sounding):
    """Per note: [(start sample, damped, active)] over the whole demo; active is False
    while the note sounds by its own render (from a strike until its damper rests)."""
    out = {}
    for note in range(21, 109):
        free = free_intervals(note, events, pedal)
        own = sounding.get(note, [])
        times = sorted({0.0, *[x for iv in free + own for x in iv if np.isfinite(x)]})
        seg = []
        for t in times:
            if t >= t_end:
                continue
            is_free = any(a <= t < b for a, b in free)
            is_own = any(a <= t < b for a, b in own)
            seg.append((int(t * SR), not is_free, not is_own))
        out[note] = seg
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True)
    ap.add_argument("-o", "--out")
    ap.add_argument("--soundboard", default="full", choices=["none", "smooth", "body", "room", "full"])
    ap.add_argument("--no-knock", action="store_true")
    ap.add_argument("--no-resonance", action="store_true")
    ap.add_argument("--no-release", action="store_true")
    ap.add_argument("--undamped-s", type=float, default=UNDAMPED_S,
                    help="rendered length of notes without dampers (s)")
    args = ap.parse_args()

    profile = scale.Profile(args.profile)
    speeds = profile.hammer_speeds()
    events, pedal, t_end = demo_events()
    jobs, placed, sounding, releases = [], [], {}, []
    for start, note, dyn, hold in events:
        if dyn is None:
            continue
        sched, last_on = damper_schedule(start, free_intervals(note, events, pedal))
        duration = (last_on + TAIL_S) if np.isfinite(last_on) else args.undamped_s
        duration = min(duration, t_end - start)
        jobs.append((args.profile, note, speeds[dyn], sched, duration, args.soundboard, not args.no_knock))
        placed.append(start)
        sounding.setdefault(note, []).append((start, start + min(last_on, duration)))
        releases.append((start + hold, note, speeds[dyn]))
    with Pool() as pool:
        notes = pool.map(render_note, jobs)
    peaks = release.peak_table(profile) if not args.no_release and args.soundboard != "none" else None

    n_total = int(t_end * SR)
    mix = np.zeros((2, n_total))
    drive = np.zeros(n_total)
    for start, (y, force), (t_up, note, v) in zip(placed, notes, releases):
        i = int(start * SR)
        mix[:, i:i + y.shape[1]] += y[:, : n_total - i]
        drive[i:i + len(force)] += force[: n_total - i]
        if not args.no_release and args.soundboard != "none":     # key-up noise
            r = release.burst(note, SR, profile.dataset) * release.level(profile, peaks, note, v)
            j = int(t_up * SR)
            if j < n_total:
                mix[:, j:j + r.shape[1]] += r[:, : n_total - j]
    if not args.no_resonance and args.soundboard != "none":
        res = resonance.sympathetic(profile, drive, resonance_schedules(events, pedal, t_end, sounding), SR)
        mix += SB.radiate(profile, res, np.zeros_like(res), SR)
    tag = args.soundboard + ("_noknock" if args.no_knock else "") + ("_nores" if args.no_resonance else "") \
        + ("_norel" if args.no_release else "")
    out = args.out or os.path.join(HERE, "out", f"demo_{args.profile}_{tag}.wav")
    # reference for the real-time engine: unnormalized mix and the event list
    sf.write(out.replace(".wav", "_raw.wav"), mix.T.astype(np.float32), SR, subtype="FLOAT")
    P.write_cache(args.profile, "demo_events", {"profile": args.profile, "sr": SR, "t_end": t_end, "pedal": pedal,
                                                "events": [{"t": t, "note": n, "speed": (speeds[d] if d else None), "hold": h}
                                                           for t, n, d, h in events]}, indent=1)
    write_mix(out, mix)


def write_mix(out, mix):
    mix = mix * 0.9 / np.abs(mix).max()
    fade = int(0.5 * SR)
    mix[:, -fade:] *= np.linspace(1, 0, fade)
    sf.write(out, mix.T, SR, subtype="PCM_24")
    print(f"wrote {out} ({mix.shape[1] / SR:.1f} s)")


if __name__ == "__main__":
    main()
