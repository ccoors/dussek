"""The sustain, measured alike on recordings and renders: frame-by-frame level, pitch and
brightness and how much they wander, the noise between the harmonics, the harmonic balance."""
import os
import numpy as np
import soundfile as sf
from scipy.signal import butter, sosfiltfilt
import player

HERE = os.path.dirname(os.path.abspath(__file__))
FR = 0.05  # frame, s


def frames(x, sr, f0):
    n = int(FR * sr)
    out = []
    # a window of at least 16 periods, and each harmonic's band at least as wide as the window's main lobe: narrower (3 % of a
    # low harmonic against +-10 Hz), a perfectly periodic tone read as noise below about 400 Hz
    win = max(4 * n, int(16 / f0 * sr))
    w = np.hanning(win)
    lobe = 3 * sr / win
    for i in range(0, len(x) - win, n):
        s = x[i : i + win]
        sp = np.abs(np.fft.rfft(s * w)) ** 2
        f = np.fft.rfftfreq(len(s), 1 / sr)
        # refine f0 on this frame from the strongest of the first harmonics
        k = np.arange(1, 13)
        hs, he = [], 0.0
        for kk in k:
            hw = min(max(0.03 * kk * f0, lobe), 0.45 * f0)
            band = (f > kk * f0 - hw) & (f < kk * f0 + hw)
            if kk * f0 > sr / 2 - 1000 or not band.any():
                hs.append(0.0)
                continue
            hs.append(sp[band].sum())
            he += sp[band].sum()
        tot = sp[(f > f0 * 0.5) & (f < min(12.5 * f0, sr / 2 - 1000))].sum()
        hs = np.array(hs)
        out.append({
            "level": 10 * np.log10(sp.sum() + 1e-20),
            "centroid": float((hs * k).sum() / (hs.sum() + 1e-20)),
            "noise_db": 10 * np.log10(max(tot - he, 1e-20) / (he + 1e-20)),
            "pitch": player.pitch(s[: max(2 * n, int(6 / f0 * sr))], sr, f0) if np.std(s) > 0 else 0,
            "harm": 10 * np.log10(hs + 1e-20),
        })
    return out


def stats(x, sr, f_note):
    fr = frames(x, sr, f_note)
    lv = np.array([q["level"] for q in fr])
    t = np.arange(len(lv))
    lv_d = lv - np.polyval(np.polyfit(t, lv, 1), t)  # the slow trend out
    pc = np.array([1200 * np.log2(q["pitch"] / f_note) for q in fr])
    pc = (pc + 600) % 1200 - 600
    ce = np.array([q["centroid"] for q in fr])
    h = np.mean([q["harm"] for q in fr], axis=0)
    # wobble: the strongest level modulation between 2 and 30 Hz
    spec = np.abs(np.fft.rfft(lv_d * np.hanning(len(lv_d))))
    fm = np.fft.rfftfreq(len(lv_d), FR)
    band = (fm > 2) & (fm < 10)
    return {
        "level_sd_db": float(np.std(lv_d)),
        "pitch_sd_c": float(np.std(pc - np.median(pc))),
        "pitch_med_c": float(np.median(pc)),
        "centroid": float(np.median(ce)),
        "centroid_sd": float(np.std(ce)),
        "noise_db": float(np.median([q["noise_db"] for q in fr])),
        "am_hz": float(fm[band][np.argmax(spec[band])]) if band.any() else 0.0,
        "harm": (h - h.max()).tolist(),
    }


HP = butter(4, 70, 'highpass', fs=44100, output='sos')


def highpass(x):
    return sosfiltfilt(HP, x)


BANDS = [(0, 1000), (1000, 2000), (2000, 4000), (4000, 8000), (8000, 16000), (16000, 22000)]


def bands(x, sr):
    """The sustain's energy per band, dB re the whole."""
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / sr)
    tot = sp[f > 50].sum()
    return [float(10 * np.log10(sp[(f >= a) & (f < b)].sum() / tot + 1e-20)) for a, b in BANDS]


def onset(x, sr, f_note):
    """The attack: ms from −40 dB (re the sustain) to the tone locked (harmonics 20 dB over
    the noise between them), and the energy above 4 kHz in the first 100 ms re the sustain's."""
    n = int(0.005 * sr)
    e = np.array([np.sqrt(np.mean(x[i:i + n] ** 2)) for i in range(0, len(x) - n, n)])
    ref = np.median(e[int(0.6 / 0.005):int(1.2 / 0.005)])
    start = int(np.argmax(e > ref * 0.01))
    lock = None
    for i in range(start, min(len(e) - 8, start + 200)):
        s = x[i * n:(i + 8) * n]
        if np.std(s) < ref * 0.1:
            continue
        fr = frames_quick(s, sr, f_note)
        if fr < -20:
            lock = i
            break
    from scipy.signal import butter, sosfilt
    hp = sosfilt(butter(4, 4000, "highpass", fs=sr, output="sos"), x)
    a = start * n
    hi_on = np.sqrt(np.mean(hp[a:a + int(0.1 * sr)] ** 2))
    hi_sus = np.sqrt(np.mean(hp[int(0.6 * sr):int(1.2 * sr)] ** 2))
    return {"lock_ms": (lock - start) * 5 if lock is not None else 999,
            "hf_attack_db": float(20 * np.log10(hi_on / (hi_sus + 1e-12) + 1e-12))}


def frames_quick(s, sr, f0):
    sp = np.abs(np.fft.rfft(s * np.hanning(len(s)))) ** 2
    f = np.fft.rfftfreq(len(s), 1 / sr)
    he = 0.0
    for k in range(1, 13):
        he += sp[(f > k * f0 * 0.94) & (f < k * f0 * 1.06)].sum()
    tot = sp[(f > f0 * 0.5) & (f < 12.5 * f0)].sum()
    return 10 * np.log10(max(tot - he, 1e-20) / (he + 1e-20))


def click(x, sr):
    """The attack's click: the peak energy above 8 kHz in 5 ms frames over the first 50 ms,
    dB re the sustain's."""
    from scipy.signal import butter, sosfilt
    hp = sosfilt(butter(4, 8000, "highpass", fs=sr, output="sos"), x)
    n = int(0.005 * sr)
    e = np.array([np.sqrt(np.mean(x[i:i + n] ** 2)) for i in range(0, len(x) - n, n)])
    ref = np.median(e[int(0.6 / 0.005):int(1.2 / 0.005)])
    start = int(np.argmax(e > ref * 0.01)) * n
    h = [np.sqrt(np.mean(hp[i:i + n] ** 2)) for i in range(start, start + int(0.05 * sr), n)]
    sus = np.sqrt(np.mean(hp[int(0.6 * sr):int(1.2 * sr)] ** 2))
    return float(20 * np.log10(max(h) / (sus + 1e-12) + 1e-12))


def rises(x, sr):
    """ms from the attack's start (−40 dB) until the level, and the treble (>4 kHz), reach
    within 3 dB of their sustain."""
    from scipy.signal import butter, sosfilt
    hp = sosfilt(butter(4, 4000, "highpass", fs=sr, output="sos"), x)
    n = int(0.005 * sr)
    env = lambda s: np.array([np.sqrt(np.mean(s[i:i + n] ** 2)) for i in range(0, len(s) - n, n)])
    e, h = env(x), env(hp)
    a, b = int(0.6 / 0.005), int(1.2 / 0.005)
    re, rh = np.median(e[a:b]), np.median(h[a:b])
    start = int(np.argmax(e > re * 0.01))
    return (int(np.argmax(e[start:] > re * 0.707)) * 5, int(np.argmax(h[start:] > rh * 0.707)) * 5)


def track(x, sr, f_note, win=0.03, hop=0.005):
    """Pitch in cents re the note every hop: the harmonics' peaks (1..6, within ±250 cents of
    their place) weighted by their strength, so a strong upper harmonic does not slip it."""
    n, h = int(win * sr), int(hop * sr)
    w = np.hanning(4 * n)[: n] if False else np.hanning(n)
    out = []
    for i in range(0, len(x) - n, h):
        s = x[i:i + n] * w
        sp = np.abs(np.fft.rfft(s, 8 * n))
        f = np.fft.rfftfreq(8 * n, 1 / sr)
        num = den = 0.0
        for k in range(1, 7):
            band = (f > k * f_note * 2 ** (-250 / 1200)) & (f < k * f_note * 2 ** (250 / 1200))
            if not band.any() or k * f_note > sr / 2 - 2000:
                continue
            j = np.argmax(sp[band])
            a = sp[band][j]
            num += a * 1200 * np.log2(f[band][j] / (k * f_note))
            den += a
        out.append(num / den if den > 0 else np.nan)
    return np.array(out)


def attack_curve(x, sr, f_note):
    """The first 200 ms's pitch, every 10 ms from the attack's start (−30 dB), cents."""
    n = int(0.005 * sr)
    e = np.array([np.sqrt(np.mean(x[i:i + n] ** 2)) for i in range(0, len(x) - n, n)])
    ref = np.median(e[int(0.6 / 0.005):int(1.2 / 0.005)])
    start = int(np.argmax(e > ref * 0.03)) * n
    tr = track(x[start:start + int(0.25 * sr)], sr, f_note)
    return tr[::2][:20]


def jumps(x, sr, f_note):
    """The sustain's pitch track: the share of time more than 30 cents off its median, and the
    largest excursion (cents)."""
    tr = track(x, sr, f_note)
    tr = tr[np.isfinite(tr)]
    d = tr - np.median(tr)
    return float(np.mean(np.abs(d) > 30)), float(np.max(np.abs(d)))


def noise_band(x, sr, f0, lo=2000, hi=8000):
    """Energy between the harmonics re on them, within lo..hi (dB)."""
    import player
    f0 = player.pitch(x[: int(0.2 * sr)], sr) * (1 if abs(np.log2(player.pitch(x[: int(0.2 * sr)], sr) / f0)) < 0.1 else 0) or f0
    # refine on the strongest harmonic below 3 kHz (the measured pitch, not the nominal one)
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / sr)
    band = (f >= lo) & (f < hi)
    k = f / f0
    on = band & (np.abs(k - np.round(k)) * f0 < max(10.0, 0.02 * f0))
    return float(10 * np.log10(sp[band & ~on].sum() / (sp[on].sum() + 1e-20) + 1e-20))
