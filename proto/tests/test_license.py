"""Every instrument's license and credits, written from its project's [license] block and those of
the projects it borrows from ([uses], followed through)."""
import os

import project as P
import license as L


def exported():
    return L.exports()


def write_project(root, name, body):
    os.makedirs(os.path.join(root, name))
    with open(os.path.join(root, name, "project.toml"), "w") as f:
        f.write(body)


BLOCK = """[license]
set = "{set}"
author = "Someone"
url = "https://example.org/{set}"
terms = "as stated"
source_license = "{lic}"
"""


def test_every_instrument_has_its_license():
    stale = []
    for p in exported():
        path = os.path.join(P.REPO, "instruments", p, "LICENSE.md")
        if not os.path.isfile(path) or open(path).read() != L.license_text(p):
            stale.append(p)
    assert stale == [], f"rewrite with `python3 proto/license.py --all`: {stale}"


def test_every_project_with_recordings_has_a_license_block():
    projects, _, _ = P.index()
    missing = []
    for name, cfg in sorted(projects.items()):
        has_sources = any(p.get("source") for p in cfg.get("part", []))
        block = cfg.get("license", {})
        if has_sources and not all(block.get(k) for k in ("set", "author", "url", "terms", "source_license")):
            missing.append(name)
        if block and block.get("source_license") not in L.SOURCE_LICENSES:
            missing.append(f"{name}: source_license {block.get('source_license')!r}")
    assert missing == []


def test_sharealike_reaches_through_uses(tmp_path, monkeypatch):
    root = str(tmp_path)
    write_project(root, "donor", BLOCK.format(set="Donor set", lic="CC-BY-SA-4.0"))
    write_project(root, "middle", BLOCK.format(set="Middle set", lic="CC0") + '\n[uses]\nfelt = "donor"\n')
    write_project(root, "made", BLOCK.format(set="Own set", lic="CC0") + '\n[uses]\nrelease = "middle"\n')
    write_project(root, "plain", BLOCK.format(set="Plain set", lic="CC0"))
    monkeypatch.setenv("PE_PROJECTS", root)
    assert L.effective_license("made") == "CC BY-SA 4.0"
    assert L.effective_license("plain") == "CC BY 4.0"
    assert "ShareAlike" in L.license_text("made")


def test_a_borrowing_credits_both():
    text = L.license_text("grand_g")
    assert "Osiris" in text and "Headroom Piano" in text


def test_music_made_with_it_is_the_players():
    for p in exported():
        assert L.MUSIC_LINE in L.license_text(p), p


def test_a_project_without_a_block_says_so(tmp_path, monkeypatch):
    root = str(tmp_path)
    write_project(root, "lender", BLOCK.format(set="Lender set", lic="CC0"))
    write_project(root, "new", '[uses]\nrelease = "lender"\n')
    monkeypatch.setenv("PE_PROJECTS", root)
    text = L.license_text("new")
    assert "not stated" in text and "Lender set" in text
    # no licensor and no license claimed for recordings nobody has named
    assert L.AUTHOR not in text and "licensed under" not in text


def test_the_instruments_author_from_its_block(tmp_path, monkeypatch):
    root = str(tmp_path)
    write_project(root, "theirs", BLOCK.format(set="Their set", lic="CC0") + 'by = "Someone Else"\n')
    monkeypatch.setenv("PE_PROJECTS", root)
    text = L.license_text("theirs")
    assert "by Someone Else" in text and L.AUTHOR not in text


def test_rolled_pianos_keep_the_license_the_engine_wrote(tmp_path, monkeypatch):
    # a rolled piano's LICENSE.md comes from the engine's roll (its recipe's sources)
    root = tmp_path / "projects"
    write_project(str(root), "my_roll", 'family = "rolled"\n')
    write_project(str(root), "plain", BLOCK.format(set="Plain set", lic="CC0"))
    inst = tmp_path / "instruments"
    for n in ("my_roll", "plain"):
        (inst / n).mkdir(parents=True)
        (inst / n / "instrument.json").write_text("{}")
    monkeypatch.setenv("PE_PROJECTS", str(root))
    assert L.exports(str(inst)) == ["plain"]


def test_the_license_makes_nothing_stale(tmp_path, monkeypatch):
    # the credit is written beside the export, no step reads it: a changed block leaves every step
    # fresh (a re-export would be a sound change)
    from test_take_checks import NAME, _copy, _prints
    steps = [("analyze", "source"), ("felt", "part"), ("export", "project")]
    plain = _prints(monkeypatch, _copy(tmp_path, "plain", lambda t: t), steps)
    changed = _prints(monkeypatch, _copy(tmp_path, "changed", lambda t: t.replace(
        "[license]\n", "[license]\nnote = \"changed\"\n", 1)), steps)
    assert NAME == "grand_a" and plain == changed
