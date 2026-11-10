import glob
import json
import os

import numpy as np

import board

ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "instruments")


def exported():
    for path in sorted(glob.glob(os.path.join(ROOT, "*", "instrument.json"))):
        if os.path.basename(os.path.dirname(path)) == "rolled_preview":
            continue  # the editor's preview (git-ignored): not an exported instrument
        d = json.load(open(path))
        if "brass" in d:  # its notes are silent: a one-sample body, the sound is the players'
            continue
        yield os.path.basename(os.path.dirname(path)), d, os.path.join(os.path.dirname(path), "data.bin")


def test_no_fundamental_lies_in_a_notch_of_the_radiation():
    # Grand F's D#7 had its fundamental 31 dB under the response around it in the left channel
    # (its second partial took over on that side); the picked draw keeps the deepest at about
    # -12 dB (board.py pick_rad_seed, at the main register's keys); the bars' mono radiation
    # (export_bars.py) lies at -6 dB at most
    seen = 0
    for name, d, blob in exported():
        ir = d["radiation"]["v"]["body"]
        raw = open(blob, "rb").read()
        h = np.frombuffer(raw, dtype="<f4", count=ir["ir"]["n"], offset=ir["ir"]["o"]).reshape(ir["channels"], -1)
        notes = d["notes"] if "notes" in d else d["registers"][0]["notes"]
        f1 = [n["string"]["f0"] * np.sqrt(1 + n["string"]["B"]) for n in notes]
        depth = board.notch_depths(h, d["reference_sr"], f1)
        worst = depth.min()
        key = notes[int(np.argmin(depth.min(axis=0)))]["note"]
        assert worst > -15.0, f"{name}: key {key}'s fundamental {worst:.1f} dB under the response around it"
        seen += 1
    if seen == 0:
        import pytest
        pytest.skip("no exported instruments")


def test_a_bars_compensation_follows_the_response_around_a_notch():
    # the bars' board has a notch 39 dB deep and a few cents wide at 6167 Hz (Glockenspiel B's C#6:
    # its mode at 6168 Hz was raised 37 dB to undo it, and the engine, playing the key in tune,
    # sounded it beside the notch: 30 dB over its neighbors); a mode moves by tens of cents with
    # the tuning, so the compensation takes the response around it
    import export_bars as E

    b = board.Board(seed=E.BOARD_SEED, density=board.DENSITY, overlap=E.BOARD_OVERLAP)
    ir = np.repeat(b.impulse_response(E.H_GRID_HZ, [0.0, 0.0], E.SR, polarization="v")[:1], 2, axis=0)
    g = 20 * np.log10(E.body_gain(ir, np.array([6167.0])))[0]
    # the response's level is 0 dB (a flat envelope), its random coloring ±4 dB
    assert g > -6.0, g


def test_no_bars_mode_is_raised_past_the_response_around_it():
    # the export divides each mode's weight by the body's gain there; within the recorded keys no
    # audible mode (30 dB of its key's strongest, up to 20 kHz) is raised by more than 6 dB past
    # what the response over the band the tuning moves it asks (±50 cents: concert pitch, A4, the
    # temperament; a notch a few cents wide is no gain the mode is played at)
    import export_bars as E

    b = board.Board(seed=E.BOARD_SEED, density=board.DENSITY, overlap=E.BOARD_OVERLAP)
    ir = b.impulse_response(E.H_GRID_HZ, [0.0, 0.0], E.SR, polarization="v")[:1]
    nfft = 1 << 20
    cum = np.concatenate([[0.0], np.cumsum(np.abs(np.fft.rfft(ir, nfft, axis=1))[0] ** 2)])
    grid = np.fft.rfftfreq(nfft, 1 / E.SR)

    def around_db(f):
        a, z = np.searchsorted(grid, f / 2 ** (1 / 24)), np.searchsorted(grid, f * 2 ** (1 / 24))
        return 10 * np.log10((cum[z] - cum[a]) / (z - a))

    proj = os.path.join(os.path.dirname(__file__), "..", "..", "projects")
    seen = 0
    for name, d, _ in exported():
        fit_path = os.path.join(proj, name, "fits", f"{d.get('profile', name)}.json")
        if "notes" not in d or not os.path.exists(fit_path):
            continue
        fit = json.load(open(fit_path))
        if "keys" not in fit or "ratios" not in fit["keys"][0]:
            continue
        lo, hi = fit["recorded"]
        # a flat body (`[export] flat_body`: the tines, the bars recorded dry) raises no mode
        import project as P
        flat = bool(P.section(name, "export").get("flat_body"))
        by_note = {n["note"]: n for n in d["notes"]}
        bad = []
        for k in fit["keys"]:
            n = by_note.get(k["note"])
            if not lo <= k["note"] <= hi or n is None:
                continue
            od = np.array(k["out_db"], float)
            f = k["f1"] * np.array(k["ratios"], float)
            # the fitted modes first (the balafon's other recorded lines follow them)
            out = np.array(n["string"]["modes"]["out"], float)[: len(od)]
            raised = 20 * np.log10(np.maximum(out, 1e-12)) - od
            for i in np.where((od > od.max() - 30) & (f <= 20000.0))[0]:
                if raised[i] > 6.0 - (0.0 if flat else around_db(f[i])):
                    bad.append(f"{k['note']}: mode {i} at {f[i]:.0f} Hz +{raised[i]:.0f} dB")
        assert not bad, f"{name}: {', '.join(bad)}"
        seen += 1
    assert seen > 0
