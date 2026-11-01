#!/usr/bin/env python3
"""Export a profile as an instrument for the real-time engine (engine/).

The prototype stays the toolchain for measuring and fitting (scale design: profile →
physical parameters per note); the engine derives everything else from them
(engine/src/physics.rs: partials, bridge admittance, coupled unison modes, sympathetic
resonators, damper rates), so it can recompute them at any sample rate and for runtime
changes (tuning, unison width, condition). The soundboard response is exported as
sampled data at the reference rate (48 kHz); the noise parts (room response, knock and
release bursts) as the parameters of their generators, which the engine runs at load
(engine/src/noise.rs: same statistics, its own random realizations).

Output: instruments/<project>/instrument.json (structure, small values) and data.bin
(little-endian arrays). An array in the JSON is {"o": byte offset, "n": count,
"t": "f64" | "f32" | "c128"} (c128: complex as interleaved f64 pairs).

Per note:
  string: f0 (Hz), B, L (m), T (N), mass (kg, one string), S, detune_cents (S), loss
  b1 (1/s), b3 (s), bridge coupling kappa (1/s) and its reactive part kappa_reactive,
  horizontal polarization: horiz_gain, horiz_kappa, horiz_detune_cents; f_max (Hz);
  hammer: mass (kg), felt_K, felt_p, balance (S), strike_pos (re L), and where the part's
    [felt_core] gives the felt a core, core_K (N/m^p, before the balance) and core_depth (m);
  has_damper, damper_sigma (1/s at log2 f = damper_log2 start + i·step) or null;
  knock: knee factor or null; impact_db: the hammer impact's trim (dB, the project's [impact]
  trim_db; left out at 0); sustain: the key's ring re the fitted (×, the project's [sustain]; left
  out at 1);
  release: impacts (t0 s, level dB, decay s), band_db (1/3-octave levels), reference
  peaks at pp/mf/ff and ratio_db points.
Global: board (the synthetic soundboard's modes f, sigma, phi and the modal density:
the bridge admittance; r, r_h: each mode's radiation to the two channels, vertical and
horizontal polarization, 2×n), correction (C(f, note) dB on notes × grid_hz) or null,
damper_log2 (start, step, count), sympathetic (transfer, f_max), knock (amp, q,
decay_s, cutoff_hz, rise_s), release (bands_hz, length_s, rise_s), mic_spacing (AB
pair, m), radiation: per polarization the body response (2×n, f32, unit energy per
channel) and the room's seed; room: t60 (Hz, s) points, spectral envelope (grid_hz,
H_db), gain, length_s; pedal (or null): kick (amp, center, spread, rolloff: the strings'
displacement when the dampers lift, pedal.py) and its phase seed base, length_s,
bands_hz, and per take of down/up: impacts, band_db, ratio_db (re the median mf note
peak), seed.
Optional buff (harpsichords): register, notes, flat and gain (1/s), pos (re L) per key: the
buff stop's pads, σ_buff(k) = flat + gain·sin²(k·π·pos) (fit_buff.py).

Usage: export.py --project grand_a
"""
import argparse
import hashlib
import json
from dataclasses import replace
import os
import shutil
import subprocess
import tempfile

import numpy as np

import damper
import even_levels as EL
import fit_hammer as FH
import project as P  # noqa: E402  (on the path through fit_hammer)
import level_corrections as LC  # noqa: E402
import license as LIC
import pedal
import release
import resonance  # noqa: F401  (TRANSFER, F_MAX)
import scale
import snapshot
import soundboard as SB

HERE = os.path.dirname(os.path.abspath(__file__))
SR = 48000


def data_tie(data):
    """instrument.json's `data`: the data.bin it is written with (its size and SHA-256; the engine
    refuses another)."""
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def write_json(outdir, doc):
    with open(os.path.join(outdir, "instrument.json"), "w") as f:
        f.write(json.dumps(doc))


def write_instrument(outdir, snapdir, doc, blob):
    """The instrument's files, `doc` tied to its data (`data`, set here); the ones they replace, when
    they differ (their measured `level` aside), kept in `snapdir` (the editor's Compare page plays
    them as Original: the instrument before the last build)."""
    data = b"".join(blob.parts)
    doc["data"] = data_tie(data)
    text = json.dumps(doc)
    old_json, old_bin = os.path.join(outdir, "instrument.json"), os.path.join(outdir, "data.bin")
    if os.path.exists(old_json) and os.path.exists(old_bin):
        with open(old_json) as f, open(old_bin, "rb") as g:
            old = json.load(f)
            old.pop("level", None)
            same = old == json.loads(text) and g.read() == data
        if not same:
            os.makedirs(snapdir, exist_ok=True)
            os.replace(old_json, os.path.join(snapdir, "instrument.json"))
            os.replace(old_bin, os.path.join(snapdir, "data.bin"))
    os.makedirs(outdir, exist_ok=True)
    with open(old_json, "w") as f:
        f.write(text)
    with open(old_bin, "wb") as f:
        f.write(data)


def measure_level(outdir):
    """The exported instrument's level as the engine plays it (the engine's `level` tool: its ff
    level, which a morph reads instead of measuring the instrument, and a hammer piano's level
    table); `store_level` keeps it in instrument.json."""
    manifest = os.path.join(HERE, "..", "engine", "Cargo.toml")
    out = subprocess.run(["cargo", "run", "--release", "--quiet", "--manifest-path", manifest, "--bin", "level", "--", outdir],
                         check=True, stdout=subprocess.PIPE, text=True).stdout
    return json.loads(out)


def store_level(outdir, doc, level_doc):
    """instrument.json with its measured level (`level`, the last key)."""
    doc["level"] = level_doc
    write_json(outdir, doc)


def apply_even_levels(doc, level_doc, table):
    """With a project's [even_levels] (None: none), each note's `level_db` gets its key's correction
    toward one level curve along the keyboard (even_levels.py; format 6: the first register's notes);
    True when it did."""
    if table is None:
        return False
    notes = doc["notes"] if "notes" in doc else doc["registers"][0]["notes"]
    try:
        rows = EL.corrections(level_doc, doc["velocity"]["speed"], table)
    except ValueError as e:
        raise ValueError(f"[even_levels]: {e}") from e
    for n in notes:
        n["level_db"] = EL.merge(n.get("level_db"), rows[n["note"] - 21])
    return True


def variant_target(project, label, menu, profile, dest):
    """A variant's directory, menu and profile, as a snapshot's (proto/snapshot.py): <project>@<label>
    in the Snapshots group."""
    if not label or "/" in label or any(c.isspace() for c in label):
        raise ValueError(f"a variant's label is one word without slashes, not {label!r}")
    name = (menu or {}).get("name") or project
    return os.path.join(dest, f"{project}@{label}"), {**(menu or {}), "name": f"{name} ({label})", "group": snapshot.GROUP}, f"{profile}@{label}"


class Blob:
    def __init__(self):
        self.parts, self.offset = [], 0

    def add(self, a, t="f64"):
        a = np.ascontiguousarray(a)
        if t == "c128":
            raw = np.ascontiguousarray(a.astype(np.complex128)).view(np.float64).astype("<f8").tobytes()
            n = a.size
        else:
            raw = a.astype("<f8" if t == "f64" else "<f4").ravel().tobytes()
            n = a.size
        ref = {"o": self.offset, "n": int(n), "t": t}
        pad = (-len(raw)) % 8
        self.parts.append(raw + b"\0" * pad)
        self.offset += len(raw) + pad
        return ref

    def replace(self, ref, a):
        """The block at `ref` (an f64 block from `add`) rewritten in place with `a`, of the same size."""
        a = np.ascontiguousarray(a)
        if ref.get("t") != "f64" or a.size != ref["n"]:
            raise ValueError(f"a block of {ref['n']} f64 values, not {a.size}")
        at = 0
        for i, part in enumerate(self.parts):
            if at == ref["o"]:
                raw = a.astype("<f8").ravel().tobytes()
                self.parts[i] = raw + part[len(raw):]
                return
            at += len(part)
        raise ValueError(f"no block at offset {ref['o']}")


# damper loss tables: per damped note at log2 f = DAMPER_LOG2[0] + i·DAMPER_LOG2[1]
DAMPER_LOG2 = (np.log2(20.0), 0.1, 100)


# the buff stop (fit_buff.py): the instruments that borrow one ([uses] buff), on their main register

# Harpsichord B's second 8′ choir and its lute row (no recordings: derived from the 8′ I fit
# as distances from the nut, β·L; starting values, tuned by ear)
D_ROW = 0.015          # m: the 8′ II jacks one row nearer the nut than the 8′ I's (a double's upper 8′)
BETA_MIN = 0.02        # the engine's pluck-point clamp
DETUNE_CENTS = 1.5     # the 8′ II choir against the 8′ I, per key uniform in ±: the two 8′s beat
DETUNE_SEED = 9208
ROW2_DB = 0.0          # the 8′ II's level re the 8′ I
D_LUTE = 0.025         # m: the lute row's distance from the nut
LUTE_FRAC = 0.6        # on short strings the lute stays this fraction of the 8′ II's point
LUTE_DB = 3.0          # the lute's level re the 8′ II row (by ear: at one quill force a bit quiet, step 226)


def choir_beta(beta_main, L):
    """The 8′ II's pluck point: one jack row (D_ROW) nearer the nut than the 8′ I's."""
    return max(beta_main - D_ROW / L, BETA_MIN)


def lute_beta(beta_ii, L):
    """The lute's pluck point: D_LUTE from the nut, nearer the nut than the 8′ II row."""
    return max(min(D_LUTE / L, LUTE_FRAC * beta_ii), BETA_MIN)


def choir_detune(note):
    """The 8′ II choir's tuning re the 8′ I (cents), seeded per key."""
    return float(np.random.default_rng(DETUNE_SEED + note).uniform(-DETUNE_CENTS, DETUNE_CENTS))


def choir_overrides(prof, note):
    """export_note's overrides for the 8′ II string of a key (from the 8′ I's fit)."""
    p = SB.note_params(prof, note)
    return {"pluck_beta": choir_beta(p.pluck_beta, p.L), "retune_cents": choir_detune(note)}


def lute_doc(prof, register):
    """The lute block: per key the lute row's pluck point on `register` (the 8′ II)."""
    betas = []
    for n in range(21, 109):
        p = SB.note_params(prof, n)
        betas.append(float(lute_beta(choir_beta(p.pluck_beta, p.L), p.L)))
    return {"register": register, "beta": betas, "gain_db": LUTE_DB}


def buff_doc(profile):
    """The buff block for this profile's instrument (register 0: its project's main part), or None."""
    if P.uses(profile, "buff") is None or P.main_part(P.project_of(profile)) != profile:
        return None
    src = P.provider(profile, "buff")
    if not P.has_fit(src):
        return None
    b = P.load_fit(src)
    return {"register": 0, "notes": [int(n) for n in b["notes"]], "flat": [float(a) for a in b["flat"]],
            "gain": [float(g) for g in b["gain"]], "pos": [float(p) for p in b["pos"]]}


ROOM_BANDS = (250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0)   # fit_room.BANDS


# a band's room level after a trim at most this over the room's unit level (dB)
ROOM_MAX_DB = 6.0


def room_eq(rows, table):
    """The room's own level per band (eq_db rows, Hz and dB) with a part's [room] trim_db by ear added
    (rows of band and dB: a cut, or a lift up to ROOM_MAX_DB over the unit level; the fits never see
    it). The partials' compensation for the room stays as fitted. A room without cuts gets every band,
    as the engine holds a row's value beyond it."""
    if not table:
        return rows
    trim = table.get("trim_db")
    if set(table) != {"trim_db"} or not isinstance(trim, list):
        raise ValueError(f"[room]: trim_db, rows of [band Hz, dB]: {table}")
    out = [[float(b), float(v)] for b, v in rows] or [[b, 0.0] for b in ROOM_BANDS]
    for row in trim:
        b, db = float(row[0]), float(row[1])
        if b not in [r[0] for r in out]:
            raise ValueError(f"[room] trim_db: a band of {ROOM_BANDS} Hz and dB: {row}")
        out = [[r[0], round(r[1] + db, 2) if r[0] == b else r[1]] for r in out]
        if max(r[1] for r in out) > ROOM_MAX_DB:
            raise ValueError(f"[room] trim_db: a band at most {ROOM_MAX_DB:+.0f} dB over the room's unit level: {row}")
    return out


BOARD_TRIM_DB = 12.0
BOARD_TRANSIENT_DB = 24.0
BOARD_KEYS = {"trim_db", "transient_db", "low_eta"}
LOW_ETA = (0.005, 0.2)


# the radiation's committed draw is kept on a ringing board unless a fundamental falls this deep in a
# notch of it (the soundboard step picked it on the fitted overlap: the deepest about −12 dB)
NOTCH_KEEP_DB = -14.0


def ringing_draw(prof, board, grid_hz, h_db, low_eta):
    """The board to radiate through with [board] low_eta: the committed draw, or where a fundamental
    lies deeper than NOTCH_KEEP_DB in a notch of its response (narrower low modes, deeper notches
    between them: Grand E's C2 −23.7 dB), the draw of the NOTCH_TRIES whose deepest is least deep
    (board.pick_rad_seed). Unchanged without low_eta."""
    if low_eta is None:
        return board
    import board as Bd
    f1 = [p.f0 * np.sqrt(1 + p.B) for p in (scale.note_params(prof, n) for n in range(21, 109))]
    worst = float(Bd.notch_depths(board.impulse_response(grid_hz, h_db, SR, polarization="v"), SR, f1).min())
    if worst >= NOTCH_KEEP_DB:
        return board
    seed, best, _ = Bd.pick_rad_seed(lambda k: scale.make_board(prof.dataset, rad_seed=k, eta_low=low_eta),
                                     grid_hz, h_db, f1, SR)
    print(f"radiation draw {seed} on the ringing board: its deepest notch at a fundamental {best:+.1f} dB "
          f"(the committed draw {board.rad_seed}: {worst:+.1f})")
    return scale.make_board(prof.dataset, rad_seed=seed, eta_low=low_eta)


def board_low_eta(table):
    """A part's [board] low_eta (the fits never see it): the board's low modes ring as with this loss
    factor where that is longer than the fitted overlap gives (board.Board eta_low); None without."""
    v = (table or {}).get("low_eta")
    if v is None:
        return None
    if not isinstance(v, (int, float)) or not LOW_ETA[0] <= v <= LOW_ETA[1]:
        raise ValueError(f"[board] low_eta: a loss factor {LOW_ETA[0]} … {LOW_ETA[1]}: {v}")
    return float(v)


def board_trim_db(freqs, table, note=None):
    """A part's [board] trim_db by ear (the fits never see it) at freqs (dB) for a key (note): per row
    a band center (Hz) and dB over the octave around it, linear in log frequency to the next octave,
    so adjacent rows join flat; 0 elsewhere. A row's optional two keys fade it in, linear from 0 at
    the first to full at the second (either way round, so a row can hold the treble or the bass),
    and keep it off the key's fundamental: 0 up to √2·f0, full from 2·f0, linear in log frequency
    (the treble's row lies on the top octave's fundamentals; a register's tone, not its loudness)."""
    trim = (table or {}).get("trim_db", [])
    if table and (not set(table) <= BOARD_KEYS or not isinstance(trim, list)):
        raise ValueError(f"[board]: trim_db, rows of [band Hz, dB] or [band Hz, dB, key at 0, key at full]; "
                         f"transient_db, rows of [band Hz, dB]; low_eta, a loss factor: {table}")
    lf = np.log2(np.maximum(np.asarray(freqs, float), 1e-9))
    out = np.zeros(len(lf))
    for row in trim:
        c, db = float(row[0]), float(row[1])
        if c <= 0 or abs(db) > BOARD_TRIM_DB or len(row) not in (2, 4) or (len(row) == 4 and row[2] == row[3]):
            raise ValueError(f"[board] trim_db: a band above 0 Hz, at most ±{BOARD_TRIM_DB:.0f} dB, "
                             f"two different keys or none: {row}")
        w = 1.0
        if len(row) == 4 and note is not None:
            above = np.clip(2 * (lf - np.log2(440.0) - (note - 69) / 12) - 1, 0, 1)
            w = float(np.clip((note - row[2]) / (row[3] - row[2]), 0, 1)) * above
        out += w * db * np.clip(1 - np.abs(lf - np.log2(c)), 0, 1)
    return out


def board_transient_db(freqs, table):
    """A part's [board] transient_db at freqs (dB): rows of a band center (Hz) and dB, the octave
    around it as trim_db's. It moves part of the board's response out of the radiation and into
    every key's correction: the radiation (the body's response, the room's level) gets it, the
    correction its opposite, so each partial keeps its gain and only the sound between the partials
    (a strike's onset, the hammer's impact, the room's input) changes. A fitted response that tilts
    far over the keyboard's middle, its treble lifted back key by key, sends a treble note's onset out
    through the hump (Grand G: 100–300 Hz 38 dB over 1–1.6 kHz, design log step 181)."""
    rows = (table or {}).get("transient_db", [])
    if not isinstance(rows, list):
        raise ValueError(f"[board] transient_db: rows of [band Hz, dB]: {rows}")
    for row in rows:
        if len(row) != 2 or float(row[0]) <= 0 or abs(float(row[1])) > BOARD_TRANSIENT_DB:
            raise ValueError(f"[board] transient_db: a band above 0 Hz and at most ±{BOARD_TRANSIENT_DB:.0f} dB: {row}")
    lf = np.log2(np.maximum(np.atleast_1d(np.asarray(freqs, float)), 1e-9))
    return sum((float(db) * np.clip(1 - np.abs(lf - np.log2(float(c))), 0, 1) for c, db in rows), np.zeros(len(lf)))


def shift_room_eq(rows, table):
    """The room's own level (eq_db rows) with the [board] transient shift added: the room scales to
    unit energy after its spectrum H, so the shift goes on its level, not on H (there it would come
    back as a lift everywhere). Both are linear in log frequency between their rows (the engine holds a
    row's value beyond the ends): the rows at both sets' breakpoints carry their sum exactly."""
    shift_rows = (table or {}).get("transient_db", [])
    if not shift_rows:
        return rows
    base = [[float(b), float(v)] for b, v in rows]
    pts = sorted({b for b, _ in base} | {float(c) * k for c, _ in shift_rows for k in (0.5, 1.0, 2.0)})
    lf = np.log2(pts)
    held = (np.interp(lf, np.log2([b for b, _ in base]), [v for _, v in base]) if base else np.zeros(len(pts)))
    return [[f, round(float(v), 2)] for f, v in zip(pts, held + board_transient_db(np.array(pts), table))]


def transient_lift_db(before, after, shift_db, band=(1000.0, 8000.0)):
    """How much the unit-energy body response rises where the [board] transient shift leaves it alone
    (dB, the median over `band` of the two responses' level ratio; 0 without a shift)."""
    if not np.any(shift_db):
        return 0.0
    f = np.fft.rfftfreq(before.shape[1], 1 / SR)
    sel = (f >= band[0]) & (f < band[1])
    ratio = 10 * np.log10((np.abs(np.fft.rfft(after, axis=1)) ** 2).sum(axis=0)[sel]
                          / (np.abs(np.fft.rfft(before, axis=1)) ** 2).sum(axis=0)[sel])
    return float(np.median(ratio))


def shift_correction(corr, shift_db):
    """The per-key correction with `shift_db` (on the board's grid) taken off every row, the radiation
    carrying it instead (board_transient_db); a part without a correction gets one carrying only that."""
    if not np.any(shift_db):
        return corr
    if corr is None:
        corr = list(SB.CORR_NOTES), np.zeros((len(SB.CORR_NOTES), len(shift_db)))
    return corr[0], np.asarray(corr[1], float) - shift_db[None, :]


def trim_correction(corr, table, grid_hz):
    """The per-key correction (anchor notes, rows on the board's grid) with the [board] trim added to
    every row; a part without a correction gets one carrying only the trim."""
    if not (table or {}).get("trim_db"):
        return corr
    if corr is None:
        corr = list(SB.CORR_NOTES), np.zeros((len(SB.CORR_NOTES), len(grid_hz)))
    curves = np.array([board_trim_db(grid_hz, table, n) for n in corr[0]])
    return corr[0], np.asarray(corr[1], float) + curves


def excitation_doc(p):
    """The note's excitation for the engine (format 5): hammer or pluck parameters."""
    if p.excitation in ("pluck", "tangent"):
        # the tangent: the pluck's limit β → 0 (physics::PluckSpec::tangent; beta unused there)
        return {"type": "pluck", "beta": p.pluck_beta, "f_q": p.pluck_fq, "amp": p.pluck_amp,
                "gain_db_per_step": p.pluck_gain_db_per_step, "fq_oct_per_step": p.pluck_fq_oct_per_step,
                **({"tangent": True} if p.excitation == "tangent" else {})}
    return {"type": "hammer", "mass": p.hammer_mass, "felt_K": p.felt_K, "felt_p": p.felt_p,
            "balance": list(p.hammer_balance[:p.n_strings]), "strike_pos": p.strike_pos,
            **({"core_K": p.core_K, "core_depth": p.core_depth} if p.core_K else {})}


def with_levels(notes, part):
    """The part's main source's level corrections on its notes (`level_db`; none: left out)."""
    tabs = LC.tables(part, ("main",))
    for n in notes:
        lv = LC.note_levels(tabs, n["note"])
        if lv is not None:
            n["level_db"] = lv
    return notes


def impact_trim_db(table, note):
    """The hammer impact's trim at a key (dB) from a project's [impact] table: trim_db, (key, dB)
    anchors set by ear, linear between and held beyond; 0 without."""
    at = sorted(table.get("trim_db", []))
    if not at:
        return 0.0
    return float(np.interp(note, [k for k, _ in at], [d for _, d in at]))



# the engine's Sustain range (physics::MIN_SUSTAIN … MAX_SUSTAIN), in percent
SUSTAIN_PERCENT = (25.0, 800.0)


# the plugin's Sympathetic resonance range, in percent
SYMPATHETIC_PERCENT = (0.0, 300.0)


def sympathetic_factor(table):
    """The strings' ringing along re the fitted coupling (×) from a project's [sympathetic] percent, set
    by ear: the sympathetic strings' drive (the export's transfer) scaled as the plugin's Sympathetic
    resonance control scales it on a string instrument, so its 100 % is the instrument's own; 1 without."""
    if not table:
        return 1.0
    v = table.get("percent")
    if set(table) != {"percent"} or not isinstance(v, (int, float)) or not SYMPATHETIC_PERCENT[0] <= v <= SYMPATHETIC_PERCENT[1]:
        raise ValueError(f"[sympathetic]: percent, {SYMPATHETIC_PERCENT[0]:.0f} … {SYMPATHETIC_PERCENT[1]:.0f}: {table}")
    return v / 100.0


def sustain_factor(table, note):
    """A key's ring re the fitted one (×) from a project's [sustain] table: percent, (key, %) anchors set
    by ear, linear between and held beyond (as the Sustain control: every mode's decay rate divided by
    it); 1 without."""
    rows = (table or {}).get("percent", [])
    if table and (set(table) != {"percent"} or not all(
            isinstance(r, list) and len(r) == 2 and 21 <= r[0] <= 108 and SUSTAIN_PERCENT[0] <= r[1] <= SUSTAIN_PERCENT[1]
            for r in rows) or [r[0] for r in rows] != sorted({r[0] for r in rows})):
        raise ValueError(f"[sustain]: percent, rows of [key 21 … 108, rising, % {SUSTAIN_PERCENT[0]:.0f} … "
                         f"{SUSTAIN_PERCENT[1]:.0f}]: {table}")
    if not rows:
        return 1.0
    return float(np.interp(note, [k for k, _ in rows], [v for _, v in rows])) / 100.0


def export_note(prof, note, blob, speeds, peaks, kn, sounding=None, peak_gain=1.0, pluck_beta=None, retune_cents=0.0,
                core=None, felt=None, unison=None):
    """Physical parameters of a note (the engine derives the modes: engine/src/physics.rs).
    `note` is the key played; an added register's string sounds at `sounding` (its
    analysis and peaks are at the sounding note), its level scaled by `peak_gain`; a derived
    choir (8′ II) overrides the pluck point and retunes the string by `retune_cents`."""
    p = scale.with_felt(SB.note_params(prof, sounding or note), felt)   # before the core: its f05 is under the felt's exponent
    p = scale.with_unison(p, unison)
    if core:
        p = scale.with_core(p, core)
    if pluck_beta is not None:
        p = replace(p, pluck_beta=pluck_beta)
    if retune_cents:
        p = replace(p, f0=p.f0 * 2 ** (retune_cents / 1200))
    S = p.n_strings
    # a jack on every key carries a damper, the harp a hand; the clavichord's listing cloth
    # damps every string once its tangent leaves it
    plucked = p.excitation in ("pluck", "tangent")
    grid = 2 ** (DAMPER_LOG2[0] + DAMPER_LOG2[1] * np.arange(DAMPER_LOG2[2]))
    sig_d = damper.hand_sigma(grid) if "fingered" in prof.traits else damper.sigma(note, grid, prof.dataset, plucked)
    out = {
        "note": note,
        # the recorded string's own pitch re the curve (the engine's "As recorded")
        "recorded_cents": round(prof.recorded_cents(sounding or note), 2),
        "string": {"f0": p.f0, "B": p.B, "L": p.L, "T": p.T, "mass": p.string_mass, "S": S,
                   "detune_cents": list(p.detune_cents[:S]), "b1": p.b1, "b3": p.b3, "kappa": p.kappa,
                   **({"loss_q": p.loss_q} if p.loss_q != 2.0 else {}),
                   "kappa_reactive": p.kappa_reactive, "horiz_gain": p.horiz_gain,
                   "horiz_kappa": p.horiz_kappa, "horiz_detune_cents": p.horiz_detune_cents,
                   "f_max": p.f_max, "scatter_cents": p.scatter_cents},
        "excitation": excitation_doc(p),
        "has_damper": bool(damper.has_damper(note, plucked)),
        "damper_sigma": blob.add(sig_d) if damper.has_damper(note, plucked) else None,
    }
    # noise parts as parameters; the engine synthesizes them (engine/src/noise.rs)
    knee = SB.knock_knee(note, kn["knee_note"]) if kn else 0.0
    out["knock"] = {"knee": knee} if knee > 0 else None
    trim = impact_trim_db(P.section(prof.dataset, "impact"), note)
    if trim:
        out["impact_db"] = trim
    ring = sustain_factor(P.section(prof.dataset, "sustain"), note)
    if ring != 1.0:
        out["sustain"] = round(ring, 4)
    rkey = release._data(prof.dataset)[1][note]
    # the harp: no mechanism comes back up (the hand's touch is not modeled): no impacts
    out["release"] = {"impacts": [] if "fingered" in prof.traits else rkey["impacts"], "band_db": rkey["band_db"],
                      "peak": [peak_gain * peaks[min(sounding or note, 108)][d] for d in ("pp", "mf", "ff")],
                      "ratio_db": sorted([float(np.log(speeds[d])), r] for d, r in rkey["ratio_db"].items())}
    return out


def register_correction(sb_main, sb_add, octave, balance_db, anchors, room=None):
    """An added register's correction on the main register's body (dB, anchors × grid):
    its own C at the sounding note (key + octave), its transfer re the main one's
    (H_add − H_main: it plays through the main register's body), its level balance, and the
    main register's room compensation (`room`, fit_room.py: it plays through that room too, whose
    cuts take from its partials)."""
    grid = np.array(sb_add["grid_hz"])
    dh = np.array(sb_add["H_db"]) - np.array(sb_main["H_db"])
    comp = np.array(room["comp_db"]) if room and "comp_db" in room else 0.0
    return np.array([SB.correction_db(sb_add, a + octave, grid) + dh + comp + balance_db[a] for a in anchors])


def calibration_gain_db(peaks, recorded):
    """dB that moves the engine's output calibration (engine::calibration: the median ff
    peak over all 88 keys, sorted element 44) onto the median over the recorded keys
    `recorded` = (lowest, highest): extrapolated keys outside them (a harpsichord's range,
    the French's ends at C6) are no reference for the level."""
    all_keys = sorted(peaks[n]["ff"] for n in range(21, 109))[44]
    rec = sorted(peaks[n]["ff"] for n in range(recorded[0], recorded[1] + 1))
    return float(20 * np.log10(all_keys / rec[len(rec) // 2]))


def register_balance(prof_main, prof_add, octave, peaks_main, peaks_add):
    """Per key (21…108) the added register's level re the main one (dB): the recorded
    peak difference (+ SFZ volume; the loudest layer; same microphones) minus the models'
    peak difference, smoothed over the keys and held beyond the recorded ones."""
    def recorded(prof):
        top = FH.D.loudest(prof.dataset)
        key = FH.D.dynamics(prof.dataset)[top]
        rows = sorted((s.get("key", s["note"]), s["peak_dbfs"] + s.get("sfz_volume_db", 0.0))
                      for s in prof.samples if FH.D.layer_key(prof.dataset, s) == key)
        k, v = np.array(rows, float).T
        return lambda n: float(np.interp(n, k, v)), top
    rm, top = recorded(prof_main)
    ra, _ = recorded(prof_add)
    keys = list(range(21, 109))
    diff = [ra(n) - rm(n) - 20 * np.log10(peaks_add[min(n + octave, 108)][top] / peaks_main[n][top])
            for n in keys]
    at = scale._smooth(keys, diff, np.ones(len(keys)), frac=4.0)
    return {n: float(at(n)) for n in keys}


def phantom_doc(section):
    """The phantom partials' level for the engine ([export] phantom, the Wear control's units), None
    when absent or 0."""
    v = float(section.get("phantom", 0.0))
    return v if v > 0 else None


def mobility_doc(cp):
    """The bridge's mobility for the engine's board block (physics.rs Mobility): the coupling's
    anchors, their factors and the boost's corner; None without anchors."""
    at = scale.mobility_anchors(cp)
    if not at:
        return None
    return {"anchors": [a for a, _ in at], "factors": [v for _, v in at], "fc_hz": scale.MOBILITY_FC_HZ}


def pedal_doc(prof):
    """Pedal noise: thump takes (impacts, band levels, level re the median mf note peak)
    for pedal down/up, their burst length and seeds, and the strings' kick (pedal.py)."""
    pd = prof.felt_fit.get("pedal")
    if not pd:
        return None
    d = pedal.data(prof.dataset)
    takes = lambda kind: [{"impacts": t["impacts"], "band_db": t["band_db"], "ratio_db": t["ratio_db"],  # noqa: E731
                           "seed": pedal.SEED[kind] + i} for i, t in enumerate(d[kind])]
    return {"kick": {x: pd[x] for x in ("amp", "center", "spread", "rolloff")}, "kick_seed": pedal.KICK_SEED,
            "length_s": pedal.LENGTH_S, "bands_hz": d["bands_hz"], "down": takes("down"), "up": takes("up")}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--project", required=True, help="the instrument's project (projects/<name>): its directory")
    ap.add_argument("--with", dest="overrides", action="append", default=[], metavar="TABLE.KEY=VALUE",
                    help="override a key of the project's top-level table (a TOML literal), for this export")
    ap.add_argument("--variant", metavar="LABEL",
                    help="write into the snapshots folder as <project>@LABEL (menu group Snapshots), not instruments/")
    ap.add_argument("--out", metavar="DIR",
                    help="write the export into DIR instead of instruments/<project> (proto/reproduce.py's)")
    args = ap.parse_args()
    for item in args.overrides:
        try:
            table, key, value = P.parse_with(item)
        except ValueError as e:
            ap.error(str(e))
        P.override(args.project, table, {key: value})
    even = P.section(args.project, "even_levels") if "even_levels" in P.config(args.project) or any(
        o.startswith("even_levels.") for o in args.overrides) else None
    if even is not None:
        try:
            EL.settings(even)
        except ValueError as e:
            ap.error(str(e))
    # [export]: the main register's part (the project's first), a second register played by the
    # same keys ({part, octave}: Harpsichord B's 4′), the derived 8′ II choir with its lute row
    ex = P.section(args.project, "export")
    main = ex.get("main") or P.main_part(args.project)
    regs = ex.get("registers", [])
    if len(regs) > 1:
        ap.error("one added register at most")
    add_register, octave = (regs[0]["part"], regs[0]["octave"]) if regs else (None, 12)
    # its name and the stop that plays it (Params::stops' bit: 1 the 4′, 2 the 8′ II): a measured
    # second 8′ ({part, octave = 0, name = "8′ II", stop = 2}: Harpsichords E and F)
    add_name, add_stop = (regs[0].get("name", "4′"), regs[0].get("stop")) if regs else ("4′", None)
    add_choir = ex.get("choir", False)
    os.environ["SOUNDBOARD_MODE"] = "full"
    low_eta = board_low_eta(P.section(main, "board"))
    prof = scale.Profile(main, eta_low=low_eta)
    speeds = prof.hammer_speeds()
    core = P.section(main, "felt_core")
    felt = P.section(main, "felt")
    unison = P.section(main, "unison")    # the release peaks without it: a peak over 1 s barely sees the detune
    peaks = release.peak_table(prof, core, felt)
    kn = prof.felt_fit.get("knock")
    blob = Blob()
    notes = with_levels([export_note(prof, n, blob, speeds, peaks, kn, core=core, felt=felt, unison=unison) for n in range(21, 109)], main)
    # radiation: the body response as data (unit energy per channel), the room as the
    # parameters of SB.room_ir; the engine builds body + room as SB.make_filter("full")
    sb = prof.felt_fit["soundboard"]
    board = prof.board or scale.make_board(prof.dataset, eta_low=low_eta)
    ir = lambda h: {"channels": int(h.shape[0]), "len": int(h.shape[1]), "ir": blob.add(h, "f32")}  # noqa: E731
    # the radiation with the [board] transient shift (the correction takes its opposite below)
    shift = board_transient_db(np.array(sb["grid_hz"]), P.section(main, "board"))
    h_db = (np.asarray(sb["H_db"], float) + shift).tolist()
    board = ringing_draw(prof, board, sb["grid_hz"], h_db, low_eta)
    bodies = {pol: board.impulse_response(sb["grid_hz"], h_db, SR, polarization=pol) for pol in ("v", "h")}
    radiation = {pol: {"body": ir(bodies[pol]), "room_seed": seed} for pol, seed in (("v", 11), ("h", 21))}
    rp = SB.room_params(prof)
    # the body's response comes at unit energy: without the shifted part it is lifted everywhere else
    # (lift_db); the correction takes that off too, and the room's gain gets it, so the body's and the
    # room's partials keep their gains
    lift = transient_lift_db(board.impulse_response(sb["grid_hz"], sb["H_db"], SR, polarization="v"), bodies["v"], shift)
    if lift:
        rp = dict(rp, gain=rp["gain"] * 10 ** (lift / 10))
    eq = shift_room_eq(room_eq(rp["eq_db"], P.section(main, "room")), P.section(main, "board"))
    radiation["room"] = {"t60": np.asarray(rp["t60"]).tolist(), "grid_hz": list(sb["grid_hz"]),
                         "H_db": list(sb["H_db"]), "gain": rp["gain"], "length_s": 2.0,
                         **({"eq_db": eq} if eq else {}),
                         # the body at the microphone positions: the shifted response, as the exported body's
                         **({"body_H_db": h_db} if np.any(shift) else {})}
    bands_hz = release._data(prof.dataset)[0]
    # C(f, note) plus the fitted room's compensation (the partials keep the identified H)
    plain = shift_correction(SB.correction_rows(sb, prof.felt_fit.get("room")), shift + lift)
    corr = trim_correction(plain, P.section(main, "board"), np.array(sb["grid_hz"]))
    # a [board] trim plays after the even levels (measured without it): else they lift a trimmed bass
    # back to the curve and undo most of it
    trimmed = None if corr is plain else (np.zeros_like(corr[1]) if plain is None else np.asarray(plain[1], float))
    # the level: the calibration on the recorded keys, plus the set's own offset (a quiet piano)
    keys = sorted({s.get("key", s["note"]) for s in prof.samples if "error" not in s})
    gain = round(ex.get("output_gain_db", 0.0) + calibration_gain_db(peaks, (keys[0], keys[-1])), 2)
    doc = {
        # the lowest and highest recorded key (the plugin's strip marks the keys beyond as extrapolated)
        "format": 5, "profile": main, "recorded": [int(keys[0]), int(keys[-1])], "body": P.section(args.project, "project")["body"], "reference_sr": SR,
        # the plugin's menu entry (the project's [menu]: name, group, labels; the engine ignores it)
        **({"menu": P.section(args.project, "menu")} if P.section(args.project, "menu") else {}),
        **({"bend": True} if "bends" in prof.traits else {}),
        "mic_spacing": SB.MIC_SPACING,
        **({"output_gain_db": gain} if abs(gain) >= 0.05 else {}),
        # the phantom partials' level, part of the piano's own sound (the Wear control adds on top)
        **({"phantom": ph} if (ph := phantom_doc(ex)) else {}),
        "velocity": {"midi": [14, 60, 124], "speed": [speeds[d] for d in ("pp", "mf", "ff")]},
        "contact": {"max_s": 0.02, "separation_s": 0.002},
        "knock": {"amp": kn["amp"], "q": kn["q"], "decay_s": kn["decay_s"], "cutoff_hz": kn["cutoff_hz"],
                  "rise_s": SB.KNOCK_RISE_S} if kn else None,
        "release": {"bands_hz": bands_hz.tolist(), "length_s": release.LENGTH_S, "rise_s": release.RISE_S},
        "pedal": pedal_doc(prof),
        "sympathetic": {"transfer": resonance.TRANSFER * sympathetic_factor(P.section(prof.dataset, "sympathetic")),
                        "f_max": resonance.F_MAX},
        # the synthetic soundboard's modes: the bridge admittance (board.py)
        "board": {"f": blob.add(board.f), "sigma": blob.add(board.sigma), "phi": blob.add(board.phi),
                  "density": board.density,
                  # radiation of each mode towards the two channels, per polarization
                  # (board.impulse_response): the engine re-synthesizes the body response
                  # for other excitation points (microphone positions, mic.rs)
                  "r": blob.add(board.r), "r_h": blob.add(board.r_h),
                  # the measured modes (fit_body_modes.py) from this index on, and the share of
                  # their admittance the horizontal polarization feels
                  **({"measured_from": int(board.n_random), "h_share": float(prof.h_share)} if board.modes else {}),
                  # the bridge's mobility along the keyboard (fit_coupling.fit_mobility)
                  **({"mobility": mob} if (mob := mobility_doc(prof.coupling())) else {})},
        # per-note transfer correction C(f, note), dB: per partial a gain (soundboard.py)
        "correction": {"notes": list(map(float, corr[0])), "grid_hz": list(map(float, sb["grid_hz"])),
                       "C_db": (c_ref := blob.add(np.array(corr[1]) if trimmed is None else trimmed))} if corr else None,
        "damper_log2": list(DAMPER_LOG2),
        "radiation": radiation,
        "notes": notes,
    }
    im = prof.felt_fit.get("impact")
    if im:
        # the hammer's impact on the board (fit_knock.py): the felt force × gain × (speed / the ff
        # anchor's)^growth through a 2nd-order lowpass at cutoff_hz (null: open), on the bridge
        doc["impact"] = {"gain": im["gain"], "cutoff_hz": im["cutoff_hz"], "growth": im["growth"]}
    if add_register:
        # format 6: registers sharing the main one's body, room and knock (one key, two jacks)
        prof2 = scale.Profile(add_register, eta_low=low_eta)     # its strings on the main body
        core2 = P.section(add_register, "felt_core")     # its strings' felt, wherever they play
        felt2 = P.section(add_register, "felt")
        unison2 = P.section(add_register, "unison")
        peaks2 = release.peak_table(prof2, core2, felt2)
        bal = register_balance(prof, prof2, octave, peaks, peaks2)
        notes2 = with_levels([export_note(prof2, n, blob, prof2.hammer_speeds(), peaks2, None, sounding=n + octave,
                                          peak_gain=10 ** (bal[n] / 20), core=core2, felt=felt2, unison=unison2) for n in range(21, 109)], add_register)
        sb2 = prof2.felt_fit["soundboard"]
        anchors = [float(a) for a in corr[0]]
        C2 = register_correction(sb, sb2, octave, {a: bal[min(max(int(round(a)), 21), 108)] for a in anchors},
                                 anchors, prof.felt_fit.get("room"))
        # it plays through the main register's body: that body's shift and trim, the trim as the main
        # one's after the even levels
        C2 = shift_correction((anchors, C2), shift + lift)[1]
        C2_trimmed = trim_correction((anchors, C2), P.section(main, "board"), np.array(sb["grid_hz"]))[1]
        del doc["notes"]
        doc["format"] = 6
        doc["registers"] = [{"name": "8′", "notes": notes},
                            {"name": add_name, **({"stop": int(add_stop)} if add_stop is not None else {}), "notes": notes2,
                             "correction": {"notes": anchors, "grid_hz": list(map(float, sb["grid_hz"])),
                                            "C_db": (c2_ref := blob.add(np.asarray(C2, float)))}}]
        print(f"balance {min(bal.values()):+.1f} … {max(bal.values()):+.1f} dB")
    if add_choir:
        if not add_register or add_stop is not None:
            ap.error("a choir needs an added register on the 4′ (format 6)")
        # the second row: the main register's strings, its corrections
        notes3 = with_levels([export_note(prof, n, blob, speeds, peaks, None, peak_gain=10 ** (ROW2_DB / 20),
                                          **choir_overrides(prof, n)) for n in range(21, 109)], main)
        doc["registers"].append({"name": "8′ II", "notes": notes3})
        doc["lute"] = lute_doc(prof, len(doc["registers"]) - 1)
    buff = buff_doc(main)
    if buff:
        doc["buff"] = buff
    unused = P.unused_overrides()
    if unused:      # a variant that would differ in nothing
        ap.error("--with read by nothing in this export: " + ", ".join(f"{t}.{k}" for _, t, k in unused))
    if args.variant:
        final, doc["menu"], doc["profile"] = variant_target(args.project, args.variant, doc.get("menu"), doc["profile"],
                                                            snapshot.snapshots_dir())
        os.makedirs(os.path.dirname(final), exist_ok=True)
        outdir = tempfile.mkdtemp(prefix=f".tmp-{os.path.basename(final)}-", dir=os.path.dirname(final))
        snapdir = os.path.join(outdir, ".previous")       # a fresh directory: nothing is replaced
    elif args.out:
        outdir = args.out
        snapdir = os.path.join(outdir, ".previous")
    else:
        outdir = os.path.join(HERE, "..", "instruments", args.project)
        snapdir = os.path.join(P.project_dir(args.project), "cache", "previous-export")
    try:
        write_instrument(outdir, snapdir, doc, blob)
        print(f"wrote {outdir}: {blob.offset / 1e6:.2f} MB data")
        level_doc = measure_level(outdir)
        if apply_even_levels(doc, level_doc, even):
            # the corrected instrument.json only (the data is the same): write_instrument would keep
            # the uncorrected one as the previous export
            write_json(outdir, doc)
            rows = np.array(EL.corrections(level_doc, doc["velocity"]["speed"], even))
            print(f"even levels: per key {rows.min():+.1f} … {rows.max():+.1f} dB, rms "
                  + " ".join(f"{d} {np.sqrt(np.mean(rows[:, i] ** 2)):.1f}" for i, d in enumerate(EL.DYNAMICS)) + " dB")
            level_doc = measure_level(outdir)
        if trimmed is not None:
            blob.replace(c_ref, np.array(corr[1]))
            if add_register:
                blob.replace(c2_ref, np.asarray(C2_trimmed, float))
            with open(os.path.join(outdir, "data.bin"), "wb") as g:
                g.write(b"".join(blob.parts))
            doc["data"] = data_tie(b"".join(blob.parts))
            write_json(outdir, doc)
            print("board trim: " + ", ".join(f"{r[0]:g} Hz {r[1]:+.1f} dB" + (f" (keys {r[2]:g} → {r[3]:g})" if len(r) == 4 else "")
                                             for r in P.section(main, "board")["trim_db"]))
            level_doc = measure_level(outdir)
        store_level(outdir, doc, level_doc)
        LIC.write(args.project, outdir)
        if args.variant:
            if os.path.exists(final):
                shutil.rmtree(final)
            os.chmod(outdir, 0o755)
            os.rename(outdir, final)
            print(f"variant {final}")
    finally:
        if args.variant and os.path.exists(outdir):
            shutil.rmtree(outdir)


if __name__ == "__main__":
    main()
