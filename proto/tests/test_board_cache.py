import numpy as np

import board


def test_boards_drawn_alike_share_the_admittance_table():
    # every task that builds a Profile builds its Board: the 0.6 s table was computed anew
    # for each (Grand D's knock fit: 253 times)
    f = np.array([100.0, 1234.5, 8000.0])
    a = board.Board(seed=7, density=1 / 15)
    y = a.admittance(f)
    b = board.Board(seed=7, density=1 / 15)
    assert b._grid is a._grid and np.array_equal(b.admittance(f), y)
    c = board.Board(seed=8, density=1 / 15)
    assert not np.allclose(c.admittance(f), y)
