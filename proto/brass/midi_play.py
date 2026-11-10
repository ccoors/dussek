"""Plays a MIDI track on the model: the mapping a DAW would give it.

CC11 air | velocity the tongue's strength | overlap = slur | pitch bend, by scaling the whole
instrument (a keyboard player's slide) | CC1, CC19 vibrato depth, rate | CC21 growl | CC22 how hard
the slurs are | CC23 flutter tongue | CC25 accent | CC20, CC26 the attack's slide, depth and time
| CC27 release time. Which mute is a choice per track, as in the files seen so far.

usage: midi_play.py file.mid out.wav [track=mute ...]   (no mapping: every track, open)
"""
import sys
import numpy as np
import soundfile as sf
from scipy.signal import resample_poly
import sim, player, fit4, fit5, compare5, muted, mutes, phrase3

SR = sim.SR
T5 = compare5.T5
KEYSWITCH_BELOW = 52  # the trumpet's lowest note is E3
PB_RANGE = 2.0  # semitones at full bend
TABLE = sorted(int(m) for m in T5["table"])
AT = None
SLUR_SLACK, SLUR_SLACK_T = 200.0, 0.02  # cents, seconds
GROWL, FLUTTER_SHAPE, FLUTTER_DEPTH = 0.3, 2, 0.6  # the air's swing at full growl; how long each tongue beat stops the air


class Track:
    """Notes as (start s, end s, midi, velocity), controllers as (time s, value) lists."""

    def __init__(self, notes, cc=None, pb=None):
        self.notes = sorted(notes)
        self.cc = cc or {}
        self.pb = pb or []


def read(path):
    import mido
    m = mido.MidiFile(path)
    tmap = sorted((t, msg.tempo) for tr in m.tracks for t, msg in _ticks(tr) if msg.type == "set_tempo")

    def sec(tick):
        s, last, tempo = 0.0, 0, 500000
        for tk, te in tmap:
            if tk >= tick:
                break
            s += (tk - last) * tempo / 1e6 / m.ticks_per_beat
            last, tempo = tk, te
        return s + (tick - last) * tempo / 1e6 / m.ticks_per_beat
    out = {}
    for tr in m.tracks:
        on, notes, cc, pb = {}, [], {}, []
        for tick, msg in _ticks(tr):
            if msg.is_meta:
                continue
            t = sec(tick)
            if msg.type == "note_on" and msg.velocity > 0:
                on[msg.note] = (t, msg.velocity)
            elif msg.type in ("note_on", "note_off") and msg.note in on:
                s, v = on.pop(msg.note)
                if msg.note >= KEYSWITCH_BELOW:
                    notes.append((s, t, msg.note, v))
            elif msg.type == "control_change":
                cc.setdefault(msg.control, []).append((t, msg.value))
            elif msg.type == "pitchwheel":
                pb.append((t, msg.pitch))
        if notes:
            out[tr.name] = Track(notes, cc, pb)
    return out


def _ticks(tr):
    t = 0
    for msg in tr:
        t += msg.time
        yield t, msg


def curve(events, default, t):
    """A controller over times t: holds its last value, eased over 5 ms."""
    if not events:
        return np.full(len(t), float(default))
    ts = np.array([e[0] for e in events])
    vs = np.array([e[1] for e in events], float)
    i = np.searchsorted(ts, t, side="right") - 1
    y = np.where(i >= 0, vs[np.maximum(i, 0)], vs[0])
    n = max(1, int(0.005 * SR))
    c = np.concatenate([[0.0], np.cumsum(y)])
    return np.concatenate([y[:n - 1], (c[n:] - c[:-n]) / n])


def nearest_table(midi):
    """A note the fit has, and the semitones the scaling adds to reach the note asked for."""
    if midi in TABLE:
        return midi, 0.0
    b = min(TABLE, key=lambda m: abs(m - midi))
    return b, float(midi - b)


def phrases(notes, gap=0.35):
    out, cur = [], [notes[0]]
    for n in notes[1:]:
        if n[0] - max(c[1] for c in cur) > gap:
            out.append(cur)
            cur = [n]
        else:
            cur.append(n)
    return out + [cur]


def render_phrase(ph, tr, mute, seed, hand=None):
    global AT
    if AT is None:
        AT = phrase3.player_map()
    t0 = ph[0][0] - 0.05
    rel_tail = 0.9
    N = int((max(n[1] for n in ph) + rel_tail - t0) * SR)
    t = t0 + np.arange(N) / SR
    d = curve(tr.cc.get(11), 100, t) / 127.0
    c = {k: curve(tr.cc.get(k), v, t) for k, v in {1: 0, 19: 64, 20: 102, 21: 0, 22: 100, 23: 0, 25: 0, 26: 64, 27: 40}.items()}
    semis = curve(tr.pb, 0, t) / 8192.0 * PB_RANGE
    pm, lo_f, seg, burst = np.zeros(N), np.zeros(N), np.zeros(N, int), np.ones(N)
    kicks = []
    notes, index = [], {}

    def note_for(m, dyn):
        key = (m, dyn)
        if key not in index:
            index[key] = len(notes)
            notes.append(muted.note(m, dyn, mute))
        return index[key]
    near = lambda dd: min(compare5.DYN, key=lambda k: abs(compare5.DYN[k] - np.clip((dd - 0.2) / 0.7, 0, 1)))
    for i, (s, e, m, v) in enumerate(ph):
        nxt = ph[i + 1] if i + 1 < len(ph) else None
        slur_next = nxt is not None and nxt[0] <= e + 0.001
        if slur_next:
            e = nxt[0]
        k0, k1 = int((s - t0) * SR), min(N, int((e - t0) * SR))
        if k1 <= k0:
            continue
        slur = i > 0 and ph[i - 1][1] >= s - 0.001
        base, warp = nearest_table(m)
        di = d[k0:k1]
        dm = float(np.median(di))
        idx = note_for(base, near(dm))
        note = notes[idx]
        p, cen = AT(base, np.maximum(di, 0.2))
        dc, air = muted.setting(base, near(dm), mute)
        cen = cen + dc
        p = p * air * np.minimum(1.0, di / 0.2)  # below pp the air simply fades
        n = k1 - k0
        tn = np.arange(n) / SR
        vel = v / 100.0
        scoop_c = 0.0
        env = np.ones(n)
        if not slur:
            a = compare5.attack_for(float(np.clip((dm - 0.2) / 0.7, 0, 1)))
            env = sim.blow(n, release=0.0, first=a["first"])
            depth = c[20][k0] / 102.0
            st = 0.04 * 2 ** ((c[26][k0] - 64) / 16)
            scoop_c = a["scoop"] * depth * np.exp(-tn / st)
            kicks.append((s + 0.002, a["kick"] * (0.4 + 0.6 * min(vel, 1.2))))
            burst[k0:k1] += a["burst"] * min(vel, 1.2) * np.exp(-tn / 0.03)
            if c[25][k0] > 64 and v > 90:  # accent: a swell of air at the start
                env = env * (1 + 0.3 * (vel - 0.9) / 0.37 * np.exp(-tn / 0.12))
        pm[k0:k1] = p * env
        lo_f[k0:k1] = note.f * (cen - 0.1) * 2 ** (-scoop_c / 1200)
        seg[k0:k1] = idx
        semis[k0:k1] += warp
        if slur:  # the lips glide, the air eases for a moment, a legato tongue nudges them
            g = min(n, int(0.025 * SR))
            if k0 > 0:
                lo_f[k0:k0 + g] = np.linspace(lo_f[k0 - 1], lo_f[k0 + g - 1], g)
                if m < ph[i - 1][2]:  # going down the lips ease past the note, or they stay on their own frequency
                    lo_f[k0:k1] *= 2 ** (-SLUR_SLACK * np.exp(-tn / SLUR_SLACK_T) / 1200)
                hard = c[22][k0] / 127.0
                h = min(2 * int(0.02 * SR), 2 * k0, 2 * n)
                pm[k0 - h // 2:k0 + h // 2] *= 1 - (0.5 + 0.35 * hard) * np.hanning(h)
                kicks.append((s, phrase3.SLUR_KICK * (0.6 + 0.4 * hard)))
        if not slur_next:
            r = int((0.02 + 0.6 * (c[27][min(k1, N - 1)] / 127.0) ** 2) * SR)
            lim = N - k1
            if nxt is not None:
                lim = max(0, int((nxt[0] - t0) * SR) - k1)
            r = min(r, lim)
            if r > 0:
                pm[k1:k1 + r] = pm[k1 - 1] * np.linspace(1, 0, r)
                lo_f[k1:k1 + r] = lo_f[k1 - 1]
                seg[k1:k1 + r] = idx
                semis[k1:k1 + r] += warp
            if nxt is not None and r == 0:
                g = min(int(0.02 * SR), n)
                pm[k1 - g:k1] *= np.linspace(1, 0, g)
        # lips stay set between notes
        nx = int((nxt[0] - t0) * SR) if nxt is not None else N
        hold = k1 + (r if not slur_next else 0)
        lo_f[hold:nx] = lo_f[hold - 1]
        seg[hold:nx] = idx
        semis[hold:nx] += warp
    rng = np.random.default_rng(seed)

    # vibrato: the lips move and the note follows; the rest of the depth in the scaling
    depth = c[1] / 127.0
    rate = 3.5 + 4.5 * c[19] / 127.0
    phase = 2 * np.pi * np.cumsum(rate) / SR
    wob = np.sin(phase)
    lo_f = lo_f * (1 + 0.012 * depth * wob)
    semis = semis + depth * 0.35 * wob
    pm = pm * (1 + 0.10 * depth * wob)

    # growl: the throat's voice in the air, rough and a little irregular
    gr = c[21] / 127.0
    if gr.max() > 0:
        from scipy.signal import butter, sosfilt
        band = sosfilt(butter(2, [55, 130], "bandpass", fs=SR, output="sos"), rng.standard_normal(N))
        band /= np.std(band) + 1e-9
        pm = pm * np.maximum(0.3, 1 + GROWL * gr * band)
        burst = burst + 4 * gr * (band > 0.3)

    # flutter: the tongue rolls at about 25 Hz, each beat takes 60 % of the air (all of it, and the
    # note breaks up: +170 cents on D4, jumps of 600 on D5)
    fl = c[23] / 127.0
    if fl.max() > 0:
        xs = np.arange(0, N, 2205)
        ph_t = 2 * np.pi * np.cumsum(25 * (1 + 0.1 * np.interp(np.arange(N), xs, rng.standard_normal(len(xs))))) / SR
        shut = (0.5 + 0.5 * np.cos(ph_t)) ** FLUTTER_SHAPE
        pm = pm * (1 - FLUTTER_DEPTH * np.minimum(1, 1.6 * fl) * shut)
        burst = burst + 6 * fl * shut
        semis = semis - 0.1 * np.minimum(1, 1.6 * fl)  # with less air the note rides about 10 cents sharp

    # the bend scales the instrument: the model runs in its own time
    n = int(0.01 * SR)
    cs = np.concatenate([[0.0], np.cumsum(semis)])
    lo = np.clip(np.arange(N) - n // 2, 0, N); hi = np.clip(lo + n, 0, N)
    semis = (cs[hi] - cs[lo]) / np.maximum(hi - lo, 1)
    ratio = 2 ** (semis / 12)
    tau = np.concatenate([[0.0], np.cumsum(ratio)[:-1]])
    M = int(tau[-1])
    grid = np.arange(M)
    warp_in = lambda a: np.interp(grid, tau, a)
    OS = fit4.OS
    pm2, lf2, bu2 = warp_in(pm), warp_in(lo_f), warp_in(burst)
    seg2 = seg[np.minimum(np.searchsorted(tau, grid), N - 1)]
    k2 = np.zeros(M * OS)
    for ts, a in kicks:
        i = int(np.interp((ts - t0) * SR, np.arange(N), tau)) * OS
        if 0 <= i < len(k2):
            k2[i] = max(k2[i], a)
    up = lambda a: np.repeat(a, OS)
    _, lp = fit4.lips(1.0, fit5.Q)
    u, _ = sim.simulate(notes, up(seg2), up(pm2), up(lf2), kick=k2, sr=SR * OS, soft=1e-4, lips=lp, firm=fit5.FIRM,
                        burst=up(bu2), seed=seed, **compare5.LIFE)
    x = resample_poly(sim.radiate(u, notes, up(seg2), nl=player.NL * T5["nl"], sr=SR * OS), 1, OS)
    if hand is not None:  # a hand on the stem's cup: the same flow through the closed end, blended in time
        closed = [note_copy(n, 1.0) for n in notes]
        y = resample_poly(sim.radiate(u, closed, up(seg2), nl=player.NL * T5["nl"], sr=SR * OS), 1, OS)
        w = np.interp(np.arange(M), tau, hand(t))
        x = x * (1 - w) + y * w
    x = np.interp(tau, np.arange(len(x)), x)
    return int(t0 * SR), muted.eq(x, mute)


def note_copy(note, hand):
    import copy
    n = copy.copy(note)
    n.mute = mutes.Wah(stem=True, hand=hand)
    for k in ("stem_r", "stem_L", "cup_r", "cup_L", "cut", "neck_r", "neck_L", "chamber_r", "chamber_L", "hole_r", "hole_L"):
        setattr(n.mute, k, getattr(note.mute, k))
    return n


def job(a):
    i, ph, tr, mute = a
    return render_phrase(ph, tr, mute, seed=1 + i)


def render_track(tr, mute, workers=8):
    from concurrent.futures import ProcessPoolExecutor
    phs = phrases(tr.notes)
    with ProcessPoolExecutor(workers) as ex:
        res = list(ex.map(job, [(i, ph, tr, mute) for i, ph in enumerate(phs)]))
    end = max(k + len(x) for k, x in res)
    y = np.zeros(end)
    for k, x in res:
        y[k:k + len(x)] += x
    return y


# about half of a mute's real loss stays: a muted part stays audible in a mix, and still sounds further off
MAKEUP = {"open": 0.0, "straight": 5.5, "cup": 0.0, "wah": 0.0, "wah_stem": 7.0, "bucket": 3.5}


def main(path, out, mapping):
    tracks = read(path)
    parts = []
    for name, tr in tracks.items():
        mute = mapping.get(name, "open")
        y = render_track(tr, mute) * 10 ** (MAKEUP[mute] / 20)
        print(f"{name}: {len(tr.notes)} notes on {mute}, {len(y) / SR:.1f} s, level {20 * np.log10(np.sqrt(np.mean(y ** 2)) + 1e-12):.1f} dB")
        parts.append(y)
    y = np.zeros(max(map(len, parts)))
    for p in parts:
        y[:len(p)] += p
    sf.write(out, y * 0.9 / np.abs(y).max(), SR)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], dict(a.split("=") for a in sys.argv[3:]))
