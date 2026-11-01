"""packaging/itch.sh: a release's five files to itch.io, one channel each (the channel's name gives
the platform); the release workflow runs it once a tag is published, only with an API key and a
target set."""
import os
import stat
import subprocess

import project as P

SCRIPT = f"{P.REPO}/packaging/itch.sh"
FILES = ["windows-setup.exe", "windows-portable.zip", "macos.pkg", "macos-portable.tar.gz", "linux.tar.gz"]


def fake_butler(tmp_path):
    """A butler that records its arguments, one call per line."""
    log = tmp_path / "calls"
    b = tmp_path / "butler"
    b.write_text(f'#!/bin/sh\necho "$@" >> {log}\n')
    b.chmod(b.stat().st_mode | stat.S_IEXEC)
    return b, log


def release(tmp_path, version, files=FILES):
    d = tmp_path / "dist"
    d.mkdir()
    for f in files:
        (d / f"dussek-{version}-{f}").write_bytes(b"x")
    return d


def run(*args, butler):
    return subprocess.run(["sh", SCRIPT, *map(str, args)], capture_output=True, text=True,
                          env={**os.environ, "BUTLER": str(butler)})


def test_each_file_goes_to_its_platform_s_channel(tmp_path):
    butler, log = fake_butler(tmp_path)
    d = release(tmp_path, "9.9.9")
    r = run("9.9.9", "someone/dussek", d, butler=butler)
    assert r.returncode == 0, r.stderr
    calls = log.read_text().splitlines()
    assert calls == [f"push {d}/dussek-9.9.9-{f} someone/dussek:{c} --userversion 9.9.9" for f, c in [
        ("windows-setup.exe", "windows-setup"), ("windows-portable.zip", "windows-portable"),
        ("macos.pkg", "mac-pkg"), ("macos-portable.tar.gz", "mac-portable"), ("linux.tar.gz", "linux")]]


def test_a_missing_file_pushes_nothing(tmp_path):
    butler, log = fake_butler(tmp_path)
    d = release(tmp_path, "9.9.9", FILES[:-1])
    r = run("9.9.9", "someone/dussek", d, butler=butler)
    assert r.returncode == 1 and "linux.tar.gz" in r.stderr
    assert not log.exists()


def test_a_target_without_its_user_pushes_nothing(tmp_path):
    butler, log = fake_butler(tmp_path)
    d = release(tmp_path, "9.9.9")
    r = run("9.9.9", "dussek", d, butler=butler)
    assert r.returncode == 1 and not log.exists()


def test_the_release_pushes_to_itch_only_when_set_up():
    # a pushed tag only, after the GitHub release; without the key or the target the step says so
    # and passes (a fork, or before the itch page exists)
    release = open(f"{P.REPO}/.github/workflows/release.yml").read()
    publish = release[release.index("  publish:"):]
    step = publish[publish.index("- name: itch.io"):]
    step = step[:step.find("\n      - name:", 1) if "\n      - name:" in step[1:] else len(step)]
    assert publish.index("gh release create") < publish.index("- name: itch.io")
    assert "if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v')" in step
    assert "secrets.BUTLER_API_KEY" in step and "vars.ITCH_TARGET" in step
    assert 'packaging/itch.sh "$VERSION" "$ITCH_TARGET" assets' in step
