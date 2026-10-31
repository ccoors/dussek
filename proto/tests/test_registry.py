"""projects/steps.toml against the projects: chains, steps, owned keys, skips and borrowings."""
import os
import tomllib

import project as P

REG = tomllib.load(open(os.path.join(P.REPO, "projects", "steps.toml"), "rb"))
NAME = lambda s: s.split("#")[0]  # noqa: E731


def test_every_chain_step_exists_and_runs_a_script():
    for fam, steps in REG["chain"].items():
        for s in steps:
            e = REG["step"][NAME(s)]
            assert e["scope"] in ("source", "part", "project")
            run = e["run"]
            if run[0] == "cargo":                  # a Rust binary (the roll): its manifest and source
                manifest = run[run.index("--manifest-path") + 1]
                binary = run[run.index("--bin") + 1]
                src = os.path.join(P.REPO, os.path.dirname(manifest), "src", "bin", binary + ".rs")
                assert os.path.exists(src), (fam, s)
                continue
            script = next(a for a in run if a.endswith(".py"))
            assert os.path.exists(os.path.join(P.REPO, script)), (fam, s)


def test_owned_keys_are_disjoint_within_a_chain():
    for fam, steps in REG["chain"].items():
        seen = {}
        for s in dict.fromkeys(NAME(x) for x in steps):
            for k in REG["step"][s].get("owns", []):
                assert k not in seen, f"{fam}: {k} owned by {seen[k]} and {s}"
                seen[k] = s
        for s in steps:
            for k in REG["step"][NAME(s)].get("amends", []):
                assert k in seen, f"{fam}: {s} amends {k}, which no step of the chain owns"


def test_every_project_has_a_chain_and_valid_skips_and_uses():
    projects = P.index()[0]
    for name, cfg in projects.items():
        fam = cfg["project"]["family"]
        assert fam in REG["chain"], name
        assert set(cfg["project"].get("skip", [])) <= set(REG["chain"][fam]), name
        for role, other in cfg.get("uses", {}).items():
            assert other in projects and other != name, (name, role)


def test_reads_name_known_kinds():
    for name, e in REG["step"].items():
        for r in e.get("reads", []) + e.get("seeds", []) + e.get("owns", []) + e.get("amends", []):
            what = r.split(":", 1)[1] if r.startswith("uses.") else r
            # "export": a borrowed project's exported files (the roll reads them)
            assert what in ("*", "export") or what.split(".")[0] in ("project", "fit", "cache"), (name, r)
