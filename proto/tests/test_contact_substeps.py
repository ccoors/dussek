import numpy as np

import string_model as M


def stiff_treble(hard=1.0):
    """A C7 grand string with a hard treble felt (Grand D's fit: 68 N at 0.5 mm)."""
    f0 = 440 * 2 ** ((100 - 69) / 12)
    return M.NoteParams(f0=f0, B=4e-3, n_strings=3, detune_cents=(0.0, 0.5, -0.4), L=0.06,
                        string_mass=0.06 * 7.85e3 * np.pi * (0.4e-3) ** 2, T=600.0,
                        hammer_mass=4e-3, felt_K=hard * 68.0 / 5e-4 ** 2.3, felt_p=2.3, strike_pos=0.1)


def test_a_stiff_contact_stays_bounded():
    # explicit at the audio rate the contact blew up for stiff contacts (Iowa C7 at ff
    # 330 N where its neighbors peak at 100–150, Grand D's C7 NaN): sub-steps keep it stable
    p = stiff_treble()
    assert M.contact_substeps(p, 7.0, 48000) > 1
    y, info = M.render(p, 7.0, duration=0.1)
    assert np.isfinite(y).all() and info["peak_force_N"] < 200


def test_substeps_converge():
    p = stiff_treble()
    ref, _ = M.render(p, 7.0, duration=0.1, substeps=32)
    y, _ = M.render(p, 7.0, duration=0.1)
    # second order: 7.6 % at the automatic 3 sub-steps (ratio 0.46), 1 % at 8
    assert np.sqrt(np.mean((y - ref) ** 2)) < 0.1 * np.sqrt(np.mean(ref ** 2))


def test_soft_contacts_keep_one_step_per_sample():
    p = M.NoteParams()      # the C4 default
    assert M.contact_substeps(p, 7.0, 48000) == 1
    a, _ = M.render(p, 3.0, duration=0.05)
    b, _ = M.render(p, 3.0, duration=0.05, substeps=1)
    assert np.array_equal(a, b)
