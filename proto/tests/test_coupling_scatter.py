import numpy as np

import fit_coupling as FC
import scale


def test_the_coupling_fit_does_not_see_the_last_runs_scatter():
    # the coupling fit rendered with the string scatter that the previous run's
    # fine_structure.py had left in the profile: a refit's result depended on the run before
    prof = scale.Profile("grand_a")
    x = FC.global_to_note(prof.felt_fit["coupling"], 60, prof.design)
    a = FC.note_params(prof, 60, x)
    prof.felt_fit["string_scatter_cents"] = 1.2
    b = FC.note_params(prof, 60, x)
    assert a.scatter_cents == b.scatter_cents == 0.0
