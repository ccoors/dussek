"""The project's name is Dussek: no tracked file names the working name it had before (step 213)."""
import re

import pytest

from test_public_tree import tracked_text

OLD = re.compile(r"piano[-_ ]?engine", re.I)
# the design log's past steps are history and keep the name they were written under
ALLOWED = ("docs/log/", "proto/tests/test_the_name.py")


def old_name_files():
    """the tracked text files (repository-relative) that name the old name, outside ALLOWED"""
    return sorted(rel for rel, lines in tracked_text()
                  if not rel.startswith(ALLOWED) and any(OLD.search(l) for l in lines))


@pytest.mark.xfail(strict=True, reason="the rename in progress")
def test_the_old_name_is_gone():
    assert old_name_files() == []
