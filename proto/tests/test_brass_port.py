"""The Rust players against live.py on a phrase (proto/brass/port_check.py, engine/src/bin/brass_check.rs): the horn, the
tenor's plunger and the tuba's attack variant, where the memos are here."""
import os
import subprocess
import sys

import pytest

import project as P

CHECK = os.path.join(P.REPO, "proto", "brass", "port_check.py")


# the tuba with the attack rising over ten periods under 100 Hz and no lean-in (step 244's variant)
ATTACK = ["brass.attack_rise_periods=10", "brass.attack_rise_hz=100", "brass.attack_lean=false"]
# the horn with a tongued note on a release starting from rest (the strip's glissando), the release faded out first
FROM_REST = ["brass.attack_from_rest=true"]
# the tuba's slow rise starting from a fifth of the pressure
RISE_FROM = ATTACK + ["brass.attack_rise_from=0.2"]


@pytest.mark.parametrize("name,mute,overrides", [("horn", "open", []), ("tenor_trombone", "plunger", []), ("tuba", "open", ATTACK),
                                                   ("horn", "open", FROM_REST), ("tuba", "open", RISE_FROM)])
def test_the_port_is_the_player(name, mute, overrides, tmp_path):
    if not os.path.exists(os.path.join(P.project_dir(name), "cache", name, f"tables_{mute}.pkl")):
        pytest.skip("the tables are not here")
    subprocess.run([sys.executable, CHECK, name, mute, str(tmp_path), *(f"--with={o}" for o in overrides)], check=True, capture_output=True)
    manifest = os.path.join(P.REPO, "engine", "Cargo.toml")
    r = subprocess.run(["cargo", "run", "--release", "--quiet", "--manifest-path", manifest, "--bin", "brass_check", "--", str(tmp_path)], capture_output=True, text=True)
    assert r.returncode == 0, r.stdout + r.stderr[-400:]
