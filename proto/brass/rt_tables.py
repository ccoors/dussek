"""What the live engine needs per mute and note, built once and cached (out/rt_<mute>.pkl): the modes per
dynamics and the two filters (the bell with the microphone's correction; the impedance the modes
leave out). rt_tables.py [mute ...] builds them; live.py does so when they are missing or stale."""
import multiprocessing, os, pickle, sys
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import bore, sim, spec, muted, mutes, compare5, phrase3, fit4

HERE = os.path.dirname(os.path.abspath(__file__))
T5 = compare5.T5
RESIDUAL = spec.load("residual", {})
TABLE = sorted(int(m) for m in T5["table"])
NFFT = 8192
HLEN, RLEN = 2048, 512
CUP_HZ = (2000.0, 3500.0)
OS = fit4.OS


def path_of(mute):
    return spec.tables_path(f"tables_{mute}{plain()}")


def plain():
    return "" if os.environ.get("RESIDUAL", "1") != "0" else "_plain"


def memo_key(mute):
    """What a mute's tables are made of: a table's memo is good while this stays the same."""
    values = [{k: v for k, v in spec.S.items() if k != "gain_db"}, plain(), T5, spec.load("tuning"), spec.load("leveled"),
              None if plain() else spec.load("residual")]
    if mute != "open":
        values += [spec.load("tuning_mutes"), spec.load("cells_mutes"), spec.lender("mute_shapes"), spec.lender("mute_residual")]
    return spec.value_hash(values) + spec.code_hash(["rt_tables.py", "mutes.py", "muted.py", "bore.py", "compare5.py"])


def modes_of(modes):
    w = np.array([2 * np.pi * m[0] for m in modes])
    Q = np.array([m[1] for m in modes])
    Z = np.array([m[2] for m in modes])
    sp = -w / (2 * Q) + 1j * w * np.sqrt(1 - 1 / (4 * Q * Q))
    return sp, Z * (w / Q) * sp / (sp - np.conj(sp))


def fade_tail(h, n):
    h = h[:n].copy()
    k = n // 4
    h[-k:] *= np.cos(np.linspace(0, np.pi / 2, k)) ** 2
    return h


def residual_gain(freqs, m, d, mute):
    """The note's harmonics against the recording's (harm_fit.py), between them in log frequency, gone over the octave past the last: an edge there is heard;
    the lips' and the bore's, so a mute keeps it."""
    r = RESIDUAL.get(f"{m}_{d}") if os.environ.get("RESIDUAL", "1") != "0" else None
    if r is None:
        return 1.0
    f = 440 * 2 ** ((m - 69) / 12)
    at = f * np.arange(1, len(r) + 1)
    return 10 ** (np.interp(np.log(np.maximum(freqs, 1.0)), np.log(np.append(at, at[-1] * 2)), np.append(r, 0.0), left=0.0) / 20)


def lumped_cup(freqs, Z, H):
    """The cup is as wide as it is deep: it holds no waves along its depth, only a volume. The slices give it a quarter wave (5.4 kHz on the tuba: no pressure at the lips, so the
    pressure to the bell peaks, and the steepening's harmonics go through that peak) and a half wave. Takes the cup's slices back out and puts its volume in, above CUP_HZ,
    where only the filters read them: the modes end below and the lips see them only, so no note's playing changes."""
    L, a = spec.S["mouth"][0][:2]
    f = np.maximum(freqs, 1.0)
    w = 2 * np.pi * f
    kL = w / bore.C * L
    S = np.pi * a * a
    Zc = bore.RHO * bore.C / S
    co, si = np.cos(kL), np.sin(kL)
    down = co - 1j * Zc * si / Z  # the pressure past the cup per the pressure at the lips
    Yt = (co - 1j * Z * si / Zc) / (Z * co - 1j * Zc * si)
    Zl = 1 / (1j * w * S * L / (bore.RHO * bore.C ** 2) + Yt)
    g = np.clip(np.log(f / CUP_HZ[0]) / np.log(CUP_HZ[1] / CUP_HZ[0]), 0.0, 1.0)
    g = 0.5 - 0.5 * np.cos(np.pi * g)
    return Z + (Zl - Z) * g, H * (1 + (1 / down - 1) * g)


def plain_chamber(freqs, H, Ho):
    """A mute is as wide as the bell: past the bell's first mode across (1.84 c / 2 pi a) its chamber holds no waves along its length alone. The tube network still gives it
    them, as notches at one spacing up to 20 kHz, which a low note's harmonics draw as lines. Past that the mute keeps what it does on average against the open bell, in
    power over 3 times that frequency, and the open bell's phase. Filters only: no note's playing changes."""
    fc = 1.84 * bore.C / (2 * np.pi * mutes.BELL)
    f = np.maximum(freqs, 1.0)
    G = H / np.where(np.abs(Ho) > 0, Ho, 1.0)
    P = np.abs(G) ** 2 + 1e-30
    n = max(3, int(3 * fc / (freqs[1] - freqs[0])) | 1)
    w = np.hanning(n + 2)[1:-1]
    avg = np.convolve(P, w, "same") / np.convolve(np.ones_like(P), w, "same")
    g = np.clip(np.log2(f / fc), 0.0, 1.0)
    g = 0.5 - 0.5 * np.cos(np.pi * g)
    return Ho * np.exp((1 - g) * 0.5 * np.log(P) + g * 0.5 * np.log(avg) + 1j * (1 - g) * np.unwrap(np.angle(G)))


def build_note(a):
    m, mute = a
    entry = {"modes": {}, "f": 440 * 2 ** ((m - 69) / 12)}
    for d in ("pp", "mf", "ff"):
        n = muted.note(m, d, mute)
        sp, rc = modes_of(n.modes)
        entry["modes"][d] = (sp, rc)
        entry.setdefault("settings", {})[d] = muted.setting(m, d, mute)
    n = muted.note(m, "mf", mute)
    freqs = np.fft.rfftfreq(NFFT, 1 / sim.SR)
    Z, H = lumped_cup(freqs, *n.transfer(np.maximum(freqs, 1.0)))
    if spec.S.get("plain_chamber") and mute not in ("open", "plunger"):
        H = plain_chamber(freqs, H, lumped_cup(freqs, *muted.note(m, "mf", "open").transfer(np.maximum(freqs, 1.0)))[1])
    H[0] = 0
    sp, rc = entry["modes"]["mf"]
    s = 2j * np.pi * np.maximum(freqs, 1.0)
    Zm = (rc[None, :] / (s[:, None] - sp[None, :]) + np.conj(rc)[None, :] / (s[:, None] - np.conj(sp)[None, :])).sum(axis=1)
    R = Z - Zm
    R[0] = 0
    entry["h"] = {d: fade_tail(np.fft.irfft(H * muted.eq_gain(freqs, mute, d) * residual_gain(freqs, m, d, mute), NFFT), HLEN) for d in ("pp", "mf", "ff")}
    entry["r"] = fade_tail(np.fft.irfft(R, NFFT), RLEN)
    if mute == "plunger":  # what leaves: "h" shut, "open" the open bell; the live player blends them by hand
        f1 = np.maximum(freqs, 1.0)
        n.mute = mutes.MUTES[mute]
        _, Hs = lumped_cup(freqs, *n.transfer(f1))
        Hs[0] = 0
        away = 10 ** ((mutes.piston_gain_db(f1, n.mute.SHUT_R * mutes.BELL / 0.0615) - mutes.piston_gain_db(f1, mutes.BELL)) / 20)
        entry["open"], entry["h"] = entry["h"], {d: fade_tail(np.fft.irfft(Hs * away * muted.eq_gain(freqs, "open", d) * residual_gain(freqs, m, d, mute), NFFT), HLEN) for d in ("pp", "mf", "ff")}
    return m, entry


def levels_at():
    """Per key the air's log and the lips' center per dynamics, and the air at the top: from the fits alone."""
    lev = spec.load("leveled")
    return {m: ([np.log(lev[str(m)][d]) for d in ("pp", "mf", "ff")], [T5["table"][str(m)][d]["center"] for d in ("pp", "mf", "ff")], phrase3.top_of(m)) for m in TABLE}


def build(mute):
    muted.know_splits([mute])
    with ProcessPoolExecutor(8, mp_context=multiprocessing.get_context("fork")) as ex:
        notes = dict(ex.map(build_note, [(m, mute) for m in TABLE]))
    path = spec.tables_path(f"tables_{mute}{plain()}", writing=True)
    tmp = path + f".{os.getpid()}"
    with open(tmp, "wb") as f:
        pickle.dump({"notes": notes, "at": levels_at(), "key": memo_key(mute)}, f)
    os.replace(tmp, path)


_LOADED = {}
CELLS = spec.load("cells", {})  # "mute_key_dyn": [lip center +, air x], the live player's repairs


def load(mute):
    if mute in _LOADED:
        return _LOADED[mute]
    t = None
    if os.path.exists(path_of(mute)):
        with open(path_of(mute), "rb") as f:
            t = pickle.load(f)
    if t is None or t.get("key") != memo_key(mute):
        print(f"building the {mute} tables (once, about a minute)...", flush=True)
        build(mute)
        with open(path_of(mute), "rb") as f:
            t = pickle.load(f)
    _LOADED[mute] = t
    return _LOADED[mute]


if __name__ == "__main__":
    for m in sys.argv[1:] or list(mutes.MUTES):
        load(m)
