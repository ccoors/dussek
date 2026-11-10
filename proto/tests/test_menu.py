"""Every exported project has a menu entry, and the exports carry it."""
import json
import os

import project as P

KNOWN = ["Grand pianos", "Upright pianos", "Rolled pianos", "Harpsichords", "Clavichords", "Harps", "Guitars", "Brass", "Percussion", "Thumb pianos", "Zithers"]


def exporting():
    """the projects with an export (instruments/<name>/)"""
    projects, _, _ = P.index()
    return [p for p in sorted(projects) if os.path.isdir(os.path.join(P.REPO, "instruments", p))]


def test_every_exporting_project_has_a_menu():
    missing = [p for p in exporting() if not {"name", "group"} <= set(P.section(p, "menu"))]
    assert not missing, missing


def test_the_listed_groups_are_the_known_ones():
    # a typo would make a group of its own
    odd = {p: m["group"] for p in exporting() if not (m := P.section(p, "menu")).get("hidden") and m["group"] not in KNOWN}
    assert not odd, odd


def test_the_exports_carry_the_menu():
    wrong = [p for p in exporting()
             if json.load(open(os.path.join(P.REPO, "instruments", p, "instrument.json"))).get("menu") != P.section(p, "menu")]
    assert not wrong, wrong
