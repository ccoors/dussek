"""Project tables overridden from the command line (analysis/project.py `override`): variants of an
export without editing its project file (design 2026-10-21-even-loudness)."""
import pytest

import project as P


@pytest.fixture(autouse=True)
def clean():
    P.clear_overrides()
    yield
    P.clear_overrides()


def test_an_override_wins_over_the_file():
    before = dict(P.section("grand_a", "felt_core"))
    P.override("grand_a", "felt_core", {"notes": [21, 60]})
    got = P.section("grand_a", "felt_core")
    assert got["notes"] == [21, 60] and got["f05"] == before["f05"]
    P.clear_overrides()
    assert P.section("grand_a", "felt_core") == before


def test_an_override_creates_a_missing_table():
    # a table no project file has (the grands carry [even_levels] since step 139)
    assert P.section("grand_a", "trial") == {}
    P.override("grand_a", "trial", {"scatter_db": 1.0})
    assert P.section("grand_a", "trial") == {"scatter_db": 1.0}


def test_parse_with():
    assert P.parse_with("even_levels.scatter_db=1.0") == ("even_levels", "scatter_db", 1.0)
    assert P.parse_with("even_levels.seed=3") == ("even_levels", "seed", 3)
    for bad in ("even_levels", "even_levels.x=not toml", "=1"):
        with pytest.raises(ValueError, match="even_levels|=1"):
            P.parse_with(bad)


def test_an_override_nobody_reads_is_reported():
    P.override("grand_a", "even_level", {"scatter_db": 1.0})       # a typo: no such table is read
    P.override("grand_a", "felt_core", {"notes": [21, 60]})
    P.section("grand_a", "felt_core")
    assert P.unused_overrides() == [("grand_a", "even_level", "scatter_db")]


def test_an_override_names_what_a_project_borrows():
    # a variant that borrows another lender's data (step 151: Grand E's dampers on the uprights)
    assert P.provider("grand_e", "dampers") == "grand_e"
    P.override("grand_e", "uses", {"dampers": "upright_a"})
    assert P.provider("grand_e", "dampers") == "upright_a" and P.unused_overrides() == []
    assert P.uses("grand_e", "release") == "grand_a"
