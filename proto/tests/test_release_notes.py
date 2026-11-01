"""packaging/release-notes.sh: a version's section of CHANGELOG.md plus the fixed footer, the text of
its GitHub release; no section, no release (the release workflow's check)."""
import subprocess

import project as P

SCRIPT = f"{P.REPO}/packaging/release-notes.sh"


def run(version, changelog):
    return subprocess.run(["sh", SCRIPT, version, str(changelog)], capture_output=True, text=True)


def test_a_version_without_a_section_has_no_notes(tmp_path):
    log = tmp_path / "CHANGELOG.md"
    log.write_text("# Changelog\n\n## 1.0.0 — 2027-01-01\n\n### New\n\n- Things.\n")
    r = run("9.9.9", log)
    assert r.returncode == 1 and r.stdout == ""


def test_the_section_then_the_footer(tmp_path):
    log = tmp_path / "CHANGELOG.md"
    log.write_text(
        "# Changelog\n\n## 9.9.9 — 2027-02-02\n\n### Breaking\n\n- A gone.\n\n### New\n\n- B.\n\n"
        "## 9.9.8 — 2027-01-01\n\n### New\n\n- Older.\n"
    )
    r = run("9.9.9", log)
    assert r.returncode == 0
    head, footer = r.stdout.split("\n---\n", 1)
    assert "### Breaking" in head and "- B." in head
    assert "Older" not in head and "## 9.9.9" not in head  # the release's title carries the version
    assert "Linux and Windows" in footer and "macOS" in footer and "untested" in footer
    assert "LV2 or CLAP" in footer and "SmartScreen" in footer and "Open Anyway" in footer


def test_the_committed_changelog_has_the_code_s_version():
    cmake = open(f"{P.REPO}/plugin/CMakeLists.txt").read()
    version = cmake.split("project(dussek-plugin VERSION ", 1)[1].split()[0].rstrip(")")
    r = run(version, f"{P.REPO}/CHANGELOG.md")
    assert r.returncode == 0, f"CHANGELOG.md has no section for {version}"


def test_a_release_s_version_must_be_the_code_s():
    check = f"{P.REPO}/packaging/check-release.sh"
    assert subprocess.run(["sh", check, "0.0.1"], capture_output=True).returncode == 1
    cmake = open(f"{P.REPO}/plugin/CMakeLists.txt").read()
    version = cmake.split("project(dussek-plugin VERSION ", 1)[1].split()[0].rstrip(")")
    r = subprocess.run(["sh", check, version], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
