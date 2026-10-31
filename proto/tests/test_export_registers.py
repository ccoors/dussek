import numpy as np

import export
import soundboard as SB


def sb(H, C_rows, notes):
    g = SB.GRID_HZ.tolist()
    return {"grid_hz": g, "H_db": H, "correction": {"notes": notes, "C_db": C_rows}}


def test_register_correction_folds_transfer_and_balance():
    nf = len(SB.GRID_HZ)
    h8, h4 = np.zeros(nf), np.linspace(-6, 0, nf)
    c4 = [[float(a)] * nf for a in (1.0, 3.0)]          # anchors at sounding 40 and 100
    anchors = [28, 88]
    C = export.register_correction(sb(h8.tolist(), [[0.0] * nf] * 2, [40, 100]),
                                   sb(h4.tolist(), c4, [40, 100]), 12, {28: 2.0, 88: -1.0}, anchors)
    assert C.shape == (2, nf)
    assert np.allclose(C[0], 1.0 + h4 - h8 + 2.0)       # key 28 sounds 40
    assert np.allclose(C[1], 3.0 + h4 - h8 - 1.0)       # key 88 sounds 100


def test_balance_of_a_register_with_itself_is_zero():
    import release
    import scale
    p = scale.Profile("grand_a")
    peaks = release.peak_table(p)
    bal = export.register_balance(p, p, 0, peaks, peaks)
    assert set(bal) == set(range(21, 109)) and max(abs(v) for v in bal.values()) < 1e-9


def test_4ft_notes_are_finite_up_to_c8():
    import scale
    p = scale.Profile("flemish4")
    for key in (21, 60, 96, 108):
        n = scale.note_params(p, key + 12)
        assert all(np.isfinite([n.f0, n.B, n.L, n.T, n.string_mass, n.pluck_beta, n.pluck_fq]))
        assert 0.04 <= n.pluck_beta <= 0.5


def test_an_added_register_gets_the_room_s_compensation_back():
    # it plays through the main register's body and room: the room's cuts take from its partials
    # as from the main one's, and an identical register's correction is the main one's
    nf = len(SB.GRID_HZ)
    main = sb([0.0] * nf, [[1.0] * nf, [2.0] * nf], [40, 100])
    room = {"comp_db": np.linspace(0, 4, nf).tolist()}
    anchors, rows = SB.correction_rows(main, room)
    C = export.register_correction(main, main, 0, {a: 0.0 for a in anchors}, anchors, room)
    assert np.allclose(C, rows)
