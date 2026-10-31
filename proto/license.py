#!/usr/bin/env python3
"""An instrument's license and credits: instruments/<project>/LICENSE.md, written from the project's
[license] block (the recordings it was fitted from and their terms) and the blocks of the projects
it borrows from ([uses], followed through: a borrowed part carries its recordings along).

The instruments are CC BY 4.0, or CC BY-SA 4.0 where any recordings reached are ShareAlike (an
adaptation keeps the license). Music made with them is the player's.

The exports call `write`; after a change to a [license] block: `python3 proto/license.py --all`
(proto/tests/test_license.py checks every LICENSE.md against its project).

    [license]
    set = "…"             # the recordings' name, as their author gives it
    author = "…"
    url = "…"
    terms = "…"           # their terms, quoted where the source words them
    source_license = "…"  # CC0, public-domain, free-use, CC-BY-4.0 or CC-BY-SA-4.0
    by = "…"              # who made the instrument (the licensor), when not this repository's author

Without a block (a project the editor made, until its recordings are named) the file names no
license: it lists what the borrowed parts carry and says the instrument's own is not stated. A
rolled piano's LICENSE.md is the engine's (its recipe's sources): left alone here.
"""
import argparse
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import project as P  # noqa: E402

AUTHOR = "Christian Friedrich Coors"
NAME = "PianoEngine"
SOURCE_LICENSES = {"CC0", "public-domain", "free-use", "CC-BY-4.0", "CC-BY-SA-4.0"}
LICENSES = {
    "CC BY 4.0": ("Creative Commons Attribution 4.0 International", "https://creativecommons.org/licenses/by/4.0/"),
    "CC BY-SA 4.0": ("Creative Commons Attribution-ShareAlike 4.0 International",
                     "https://creativecommons.org/licenses/by-sa/4.0/"),
}
MUSIC_LINE = "Music made with this instrument (renders, recordings, performances) is yours: use it freely, without credit."


def _config(name):
    # read directly, not through P.section: the license is no input of a fit, and the driver's
    # ledger leaves it out (a changed block rewrites the file, it makes no step stale)
    return P.index()[0].get(name, {})


def menu_name(name):
    return _config(name).get("menu", {}).get("name", name)


def lenders(name):
    """[(project, [roles], through)]: every project reached through [uses], first reached first;
    `through` the project that borrows from it"""
    out, seen, queue = [], {name}, [name]
    while queue:
        cur = queue.pop(0)
        roles = {}
        for role, other in _config(cur).get("uses", {}).items():
            roles.setdefault(other, []).append(role)
        for other, rs in roles.items():
            if other in seen:
                continue
            seen.add(other)
            out.append((other, rs, cur))
            queue.append(other)
    return out


def exports(instruments=None):
    """the projects with an export in `instruments` (default the repository's), rolled pianos left
    out"""
    instruments = instruments or os.path.join(P.REPO, "instruments")
    return [p for p in sorted(P.index()[0])
            if os.path.isfile(os.path.join(instruments, p, "instrument.json")) and _config(p).get("family") != "rolled"]


def effective_license(name):
    """"CC BY-SA 4.0" when this project's recordings or any it borrows from are ShareAlike, else
    "CC BY 4.0\""""
    names = [name] + [o for o, _, _ in lenders(name)]
    sharealike = any(_config(n).get("license", {}).get("source_license") == "CC-BY-SA-4.0" for n in names)
    return "CC BY-SA 4.0" if sharealike else "CC BY 4.0"


def credits(name):
    """[(block or None, how)]: this project's recordings first, then each borrowed project's, one
    entry per set"""
    out, sets = [], set()
    block = _config(name).get("license")
    out.append((block, None))
    if block:
        sets.add(block["set"])
    for other, roles, through in lenders(name):
        b = _config(other).get("license")
        if not b or b["set"] in sets:
            continue
        sets.add(b["set"])
        via = "" if through == name else f", through {menu_name(through)}"
        out.append((b, f"{', '.join(roles)} from {menu_name(other)}{via}"))
    return out


def _credit_line(block, how):
    terms = {"CC0": "CC0 1.0 (https://creativecommons.org/publicdomain/zero/1.0/)",
             "public-domain": "public domain",
             "free-use": "free to use",
             "CC-BY-4.0": "CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/)",
             "CC-BY-SA-4.0": "CC BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/)"}[block["source_license"]]
    line = f"- **{block['set']}** by {block['author']}, {block['url']}: {terms}. {block['terms']}"
    return line + (f" (borrowed: {how})" if how else "")


def license_text(name):
    lic = effective_license(name)
    title, url = LICENSES[lic]
    own = _config(name).get("license")
    lines = [f"# License: {menu_name(name)} (`{name}`)", ""]
    if own is None:
        lines += ["Its license is not stated: its project names no recordings and terms (a `[license]` block).",
                  f"Its borrowed parts below ask for {lic} at least.", ""]
    else:
        lines += [f"This instrument (the files in this directory) was made with {NAME}, by {own.get('by', AUTHOR)}.",
                  f"It is licensed under the **{title}** license ({lic}, {url}).", ""]
        if lic == "CC BY-SA 4.0":
            lines += ["ShareAlike: a changed version of this instrument, or an instrument made with parts of it,",
                      "keeps this license.", ""]
        lines += [MUSIC_LINE, ""]
    lines += ["It was fitted from these recordings. No audio from them is contained here: they were analyzed",
              "(partial frequencies, levels, decays) and a physical model's parameters fitted to them.", ""]
    for block, how in credits(name):
        if block is None:
            lines.append("- its own recordings: not stated (add a `[license]` block to its project)")
        else:
            lines.append(_credit_line(block, how))
    if own is not None:
        lines += ["", f"Credit it as: {menu_name(name)} from {NAME} ({lic}), fitted from "
                  + "; ".join(b["set"] for b, _ in credits(name) if b) + "."]
    return "\n".join(lines) + "\n"


def write(name, outdir):
    """instruments/<name>/LICENSE.md (or outdir's)"""
    path = os.path.join(outdir, "LICENSE.md")
    with open(path, "w") as f:
        f.write(license_text(name))
    return path


def main():
    ap = argparse.ArgumentParser(description="Write the instruments' LICENSE.md from their projects.")
    ap.add_argument("projects", nargs="*")
    ap.add_argument("--all", action="store_true", help="every exported instrument")
    args = ap.parse_args()
    names = args.projects
    if args.all:
        names = exports()
    for n in names:
        print(write(n, os.path.join(P.REPO, "instruments", n)))


if __name__ == "__main__":
    main()
