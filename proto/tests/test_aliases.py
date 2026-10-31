"""instruments/aliases.txt: every alias leads to an instrument folder (directly or through another
alias), and no alias names a folder that exists (the folder would win; the alias would be dead)."""
import os

import project as P

INSTRUMENTS = os.path.join(P.REPO, "instruments")


def aliases():
    out = {}
    with open(os.path.join(INSTRUMENTS, "aliases.txt")) as f:
        for line in f:
            words = line.split()
            if len(words) >= 2 and not words[0].startswith("#"):
                out[words[0]] = words[1]
    return out


def test_every_alias_leads_to_an_instrument():
    a = aliases()
    folders = {d for d in os.listdir(INSTRUMENTS) if os.path.isfile(os.path.join(INSTRUMENTS, d, "instrument.json"))}
    for old, new in a.items():
        seen = [old]
        while new in a and new not in seen:
            seen.append(new)
            new = a[new]
        assert new in folders, f"aliases.txt: {old} leads to {new}, no instrument folder"
        assert old not in folders, f"aliases.txt: {old} is a folder: the alias never applies"
