"""The brass chains in projects/steps.toml and proto/brass/steps.py."""
import hashlib
import importlib.util
import json
import os
import subprocess
import sys

import build
import project as P

FIVE = ("trumpet", "tenor_trombone", "horn", "bass_trombone", "tuba")
BRASS = os.path.join(P.REPO, "proto", "brass")


def test_the_chains_end_at_the_tables_the_report_and_the_export():
    for name in FIVE:
        assert build.chain(name)[-3:] == ["brass_tables", "brass_report", "export_brass"]


def test_only_the_lender_measures_the_mutes():
    for step in ("brass_mute_shapes", "brass_mute_residual"):
        assert step in build.chain("trumpet") and step not in build.chain("tuba")


def test_the_soft_slurs_are_repaired_before_the_levels_are_measured():
    """The repair moves lips and air, so a key's level comes after it; the landing's repair of slurs comes before it."""
    for name in FIVE:
        chain = build.chain(name)
        assert chain.index("brass_land") < chain.index("brass_slurs") < chain.index("brass_level")


def test_the_steps_exist():
    spec = importlib.util.spec_from_file_location("brass_steps", os.path.join(BRASS, "steps.py"))
    steps = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(steps)
    for name in FIVE:
        for step in build.chain(name)[:-1]:  # the export is a script of its own
            assert callable(getattr(steps, step.split("#")[0])), step


def test_brass_fit_access_is_checked():
    # under the driver's environment: brass_speak writing another step's key raises and leaves the file
    path = P.fit_path("tuba")
    before = hashlib.sha256(open(path, "rb").read()).hexdigest()
    env = {**os.environ, "BRASS_PROJECT": "tuba", "PE_STEP": "brass_speak", "PE_PROJECT": "tuba"}
    env.pop("PE_RECORD", None)
    r = subprocess.run([sys.executable, "-c", 'import spec; spec.save("level", {})'], cwd=BRASS, env=env,
                       capture_output=True, text=True)
    assert r.returncode != 0 and "Undeclared" in r.stderr
    assert hashlib.sha256(open(path, "rb").read()).hexdigest() == before


def test_a_mute_s_report_skips_a_note_the_instrument_lacks():
    """The mutes' one measurement is on F4, over the top of the tuba's table."""
    code = "import muted; muted.measured_note(['straight']); print('done')"
    env = {**os.environ, "BRASS_PROJECT": "tuba"}
    out = subprocess.run([sys.executable, "-c", code], cwd=BRASS, env=env, capture_output=True, text=True)
    assert out.returncode == 0 and "done" in out.stdout, out.stderr[-400:]


def picks(project, mute, key):
    """What each of a key's dynamics plays on under a mute: the cents from its open mode to the mode it takes."""
    code = ("import json, muted; "
            f"print(json.dumps({{d: muted.mode_cents({key}, d, {mute!r}) for d in ('pp', 'mf', 'ff')}}))")
    env = {**os.environ, "BRASS_PROJECT": project}
    out = subprocess.run([sys.executable, "-c", code], cwd=BRASS, env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr[-400:]
    return json.loads(out.stdout.splitlines()[-1])


def test_a_split_mode_is_one_mode_for_all_of_a_key_s_dynamics():
    """A mute splits a mode, the open note between the halves: the lower half does not hold, and a few cents
    between the dynamics must not send one of them there."""
    got = picks("tenor_trombone", "straight", 45)  # the straight mute's split key since the mutes' shapes were refitted (step 242)
    assert all(c > 100 for c in got.values()), got


def test_a_key_s_dynamics_that_agree_keep_their_mode():
    got = picks("tenor_trombone", "straight", 50)
    assert max(got.values()) - min(got.values()) < 60 and max(abs(c) for c in got.values()) < 100, got


def test_a_mute_s_steps_end_by_checking_that_its_notes_hold(monkeypatch):
    """The mode fix moves slides and lips: what it leaves is what a player gets."""
    spec = importlib.util.spec_from_file_location("brass_steps", os.path.join(BRASS, "steps.py"))
    steps = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(steps)
    calls = []
    monkeypatch.setattr(steps, "run", lambda *a, **k: calls.append(a), raising=True)
    monkeypatch.setattr(steps.P, "update_fit", lambda *a, **k: None)  # the step starts from none: not the committed fit
    steps.NAME = "tuba"
    steps.brass_mutes()
    assert calls[-1] == ("rt_stability", "straight", "repair")
    assert calls.index(("mode_fix", "lips", "straight")) < len(calls) - 1


def test_the_slur_repair_covers_what_goes_in_the_bell(monkeypatch):
    spec = importlib.util.spec_from_file_location("brass_steps", os.path.join(BRASS, "steps.py"))
    steps = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(steps)
    calls = []
    monkeypatch.setattr(steps, "run", lambda *a, **k: calls.append(a), raising=True)
    steps.NAME = "tuba"
    steps.brass_slurs()
    assert calls == [("slur_fix", "repair", "open"), ("slur_fix", "repair", "straight")]


def test_the_report_counts_what_can_be_played():
    """Keys outside the range stay quiet: a count that holds them says nothing about the instrument."""
    code = "import json, rt_stability; print(json.dumps(rt_stability.in_range({34: 9, 40: 3, 72: 6, 74: 9})))"
    env = {**os.environ, "BRASS_PROJECT": "tenor_trombone"}
    out = subprocess.run([sys.executable, "-c", code], cwd=BRASS, env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr[-400:]
    assert json.loads(out.stdout.splitlines()[-1]) == {"40": 3, "72": 6}


def landing(project, key, before, seed, vel=None):
    v = "" if vel is None else f", vel={vel}"
    code = f"import slur_fix; print(slur_fix.landing({key}, {before}, {seed}{v}))"
    env = {**os.environ, "BRASS_PROJECT": project}
    out = subprocess.run([sys.executable, "-c", code], cwd=BRASS, env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr[-400:]
    return float(out.stdout.splitlines()[-1])


def test_a_soft_falling_line_lands_on_its_notes():
    """The horn's G4 and F#4, a stronger mode close under them: a soft slur stayed up to 60 c flat for as long as it sounded."""
    for seed in (1, 2):
        assert abs(landing("horn", 67, [69], seed)) < 25
        assert abs(landing("horn", 66, [69, 67], seed)) < 25


def test_a_loud_falling_line_lands_on_its_notes():
    """The trumpet's G-sharp 5 slurred from above at ff: it sat 115 c flat, near G5, for up to a second (heard in the plugin)."""
    for before, seed in (([82], 1), ([81], 2), ([83], 3)):
        assert abs(landing("trumpet", 80, before, seed, vel=127)) < 25


def test_a_slur_repair_that_falls_short_leaves_the_tongued_note_be():
    """Most playing is tongued: a move that only comes closer may not take the tongued note further off."""
    code = ("import slur_fix as f; print(f.may_take(-60, -30, 20, 10), f.may_take(-60, -30, 40, 10), "
            "f.may_take(-60, -30, 40, 45), f.may_take(-60, -55, 10, 10))")
    env = {**os.environ, "BRASS_PROJECT": "tuba"}
    out = subprocess.run([sys.executable, "-c", code], cwd=BRASS, env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr[-400:]
    assert out.stdout.split() == ["True", "False", "True", "False"]


def test_the_prototype_plays_the_export_s_keys():
    """The player and the stability check stop where the export does: at the table's top unless `above` says more."""
    code = ("import json, live, rt_stability, rt_tables; t = rt_tables.TABLE; "
            "print(json.dumps([live.plays(t[-1]), live.plays(t[-1] + 1), live.plays(t[0] - 6), live.plays(t[0] - 7), "
            "rt_stability.KEYS[-1] == t[-1]]))")
    env = {**os.environ, "BRASS_PROJECT": "horn"}
    out = subprocess.run([sys.executable, "-c", code], cwd=BRASS, env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr[-400:]
    assert json.loads(out.stdout.splitlines()[-1]) == [True, False, True, False, True]


def test_an_instrument_without_mutes_still_writes_the_mutes_outputs(monkeypatch):
    """The driver checks a step's outputs: the horn has no mutes, and its fresh fit had none, so the step stayed stale."""
    spec = importlib.util.spec_from_file_location("brass_steps", os.path.join(BRASS, "steps.py"))
    steps = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(steps)
    calls, saved = [], []
    monkeypatch.setattr(steps, "run", lambda *a, **k: calls.append(a), raising=True)
    monkeypatch.setattr(steps.P, "update_fit", lambda part, values: saved.append((part, values)), raising=True)
    steps.NAME = "horn"
    steps.brass_mutes()
    assert calls == [] and saved == [(P.main_part("horn"), {"cells_mutes": {}, "tuning_mutes": {}})]


def test_a_slur_repair_moves_only_the_lips():
    """The air sets a note's level: 0.8 of it left the tenor's wah E-flat 4 5.6 dB under its line on a soft slur, mostly
    the bell's noise; 1.25 put others 0.7 to 4 dB over theirs."""
    code = "import json, slur_fix; m = slur_fix.candidates('open', 60); print(json.dumps([len(m), sorted({a[3] for a in m})]))"
    env = {**os.environ, "BRASS_PROJECT": "tenor_trombone"}
    out = subprocess.run([sys.executable, "-c", code], cwd=BRASS, env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr[-400:]
    count, airs = json.loads(out.stdout.splitlines()[-1])
    assert count > 0 and airs == [1.0]


def test_each_export_carries_its_project_s_output_gain():
    """The brass sit at the grands' loudness (step 243): a gain per project in [export], written into its export."""
    for name in FIVE:
        gain = P.section(name, "export").get("output_gain_db")
        assert gain is not None, name
        with open(os.path.join(P.REPO, "instruments", name, "instrument.json")) as f:
            assert json.load(f).get("output_gain_db") == gain, name
