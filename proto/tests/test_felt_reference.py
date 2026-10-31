import numpy as np

import datasets as D
import fit_profile_felt as FP
import scale


def test_a_felt_reference_fixes_the_felt_to_the_reference_curve():
    # Osiris's two soft layers (1.5 and 2.5 m/s) say almost nothing about the felt: fitted
    # per note it jumped between 0.5 and 128 N. It takes the Headroom C3's felt instead
    assert D.felt_reference("grand_g") == "grand_c"
    ref = scale.Profile("grand_c")
    grid = FP.reference_felts("grand_g", [36, 60, 84])
    for n, f in grid.items():
        want = ref.felt(n)[1]
        assert f in FP.F05_GRID
        assert abs(np.log2(f) - np.log2(want)) <= 0.25 + 1e-9      # the nearest grid step (√2)
    assert FP.reference_felts("grand_b", [60]) is None
