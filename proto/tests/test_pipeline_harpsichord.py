import numpy as np

import export
import release
import scale
import string_model as M


def test_excitation_doc_hammer_keeps_the_values():
    p = scale.note_params(scale.Profile("grand_a"), 60)
    d = export.excitation_doc(p)
    assert d == {"type": "hammer", "mass": p.hammer_mass, "felt_K": p.felt_K, "felt_p": p.felt_p,
                 "balance": list(p.hammer_balance[:p.n_strings]), "strike_pos": p.strike_pos}


def test_excitation_doc_pluck():
    p = M.NoteParams(excitation="pluck", pluck_beta=0.25, pluck_fq=4000.0)
    assert export.excitation_doc(p) == {"type": "pluck", "beta": 0.25, "f_q": 4000.0, "amp": p.pluck_amp,
                                        "gain_db_per_step": 3.0, "fq_oct_per_step": 0.5}


def test_release_ratio_with_one_dynamic():
    speeds = {"pp": 0.5, "mf": 1.0, "ff": 2.0}
    r = [release.ratio_db(60, v, speeds, "harpsichord") for v in (0.5, 1.0, 2.0)]
    assert all(np.isfinite(r)) and r[0] == r[1] == r[2]
    assert release._data("harpsichord")[1].keys() == set(range(21, 109))


def test_piano_release_source_unchanged():
    # a piano without release samples of its own borrows Salamander's ([uses] release)
    assert release._data("grand_f")[1][60] == release._data("grand_a")[1][60]
