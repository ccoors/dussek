import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, os.path.join(HERE, "..", "..", "analysis"))

import re  # noqa: E402

import pytest  # noqa: E402

import project as P  # noqa: E402

CACHE = re.compile(r"/projects/([^/]+)/cache/")
DATA = os.path.join(P.REPO, "data") + os.sep


@pytest.hookimpl(wrapper=True)
def pytest_runtest_call(item):
    """A test that reads a dataset not in data/, or a project's cache (not in git, but a lender's)
    where neither the cache nor the project's recordings are here (a fresh clone, CI): skipped,
    naming what it needs. With the recordings here a missing cache stays a failure: the project
    wants building."""
    try:
        return (yield)
    except FileNotFoundError as e:
        path = str(e.filename or "")
        if path.startswith(DATA):
            pytest.skip(f"needs data/{path[len(DATA):].split(os.sep)[0]} (not downloaded)")
        m = CACHE.search(path)
        if m and not P.has_recordings(m[1]):
            pytest.skip(f"needs {m[1]}'s caches (not in git; its recordings not in data/)")
        raise
