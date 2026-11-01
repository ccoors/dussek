"""site/build.py: the pages from a renders folder (here a fixture of empty files, as render.py lays
them out): every link and source resolves inside the site or points outside it on purpose, the
performances carry their license and citation, the credits name every instrument."""
import glob
import html.parser
import json
import os
import subprocess
import sys
import tomllib
from urllib.parse import urlparse

import pytest

import project as P

BUILD = f"{P.REPO}/site/build.py"
pytest.importorskip("markdown")


class Links(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.refs = []

    def handle_starttag(self, tag, attrs):
        for k, v in attrs:
            if k in ("href", "src", "poster") and v:
                self.refs.append(v)


def fixture_renders(root):
    """What render.py and screenshots.sh leave: clips.json, the audio, the frames, the video, the shots."""
    clips = tomllib.load(open(f"{P.REPO}/site/clips.toml", "rb"))["clip"]
    entries = []
    for c in clips:
        steps = c.get("steps") or [{"label": c["title"]}]
        files = [{"label": s["label"], "file": f"audio/{c['id']}{'' if len(steps) == 1 else f'-{i + 1}'}.ogg"}
                 for i, s in enumerate(steps)]
        entries.append({"id": c["id"], "section": c["section"], "title": c["title"], "instrument": c["title"],
                        "group": None, "listen": c.get("listen", ""), "license": c.get("license", "cc0"), "steps": files})
        for f in files:
            os.makedirs(os.path.dirname(f"{root}/{f['file']}"), exist_ok=True)
            open(f"{root}/{f['file']}", "wb").close()
        if c["section"] == "demo":
            os.makedirs(f"{root}/frames", exist_ok=True)
            open(f"{root}/frames/{c['id']}.jpg", "wb").close()
    json.dump({"clips": entries, "groups": {}}, open(f"{root}/clips.json", "w"))
    os.makedirs(f"{root}/video", exist_ok=True)
    for f in ("video/ensemble.mp4", "video/ensemble.jpg"):
        open(f"{root}/{f}", "wb").close()
    os.makedirs(f"{root}/screenshots", exist_ok=True)
    for f in ("grand-sound", "grand-felt", "grand-mics", "grand-visualization", "vibraphone"):
        open(f"{root}/screenshots/{f}.png", "wb").close()


@pytest.fixture(scope="module")
def site(tmp_path_factory):
    root = tmp_path_factory.mktemp("site")
    fixture_renders(root / "out")
    r = subprocess.run([sys.executable, BUILD, "--renders", str(root / "out"), "--out", str(root / "public")],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return root / "public"


def test_every_link_resolves(site):
    pages = glob.glob(f"{site}/**/*.html", recursive=True)
    assert len(pages) >= 2
    broken = []
    for page in pages:
        p = Links()
        p.feed(open(page, encoding="utf-8").read())
        for ref in p.refs:
            u = urlparse(ref)
            if u.scheme in ("http", "https", "mailto") or ref.startswith("#"):
                continue
            target = os.path.normpath(os.path.join(os.path.dirname(page), u.path))
            if not os.path.exists(target):
                broken.append(f"{os.path.relpath(page, site)} → {ref}")
    assert not broken, "\n".join(broken)


def test_performances_carry_their_license(site):
    page = open(f"{site}/index.html", encoding="utf-8").read()
    assert page.count("CC BY-NC-SA 4.0") >= 1 and "Hawthorne" in page and "MAESTRO" in page


def test_credits_name_every_instrument(site):
    page = open(f"{site}/credits.html", encoding="utf-8").read()
    for f in glob.glob(f"{P.REPO}/instruments/*/instrument.json"):
        menu = json.load(open(f)).get("menu", {})
        if not menu.get("hidden"):
            assert menu["name"] in page, menu["name"]


def test_no_external_fonts_or_scripts(site):
    for page in glob.glob(f"{site}/**/*.html", recursive=True):
        p = Links()
        text = open(page, encoding="utf-8").read()
        p.feed(text)
        assert "fonts.googleapis" not in text
        for ref in p.refs:
            if ref.endswith((".js", ".css", ".woff2")):
                assert not urlparse(ref).scheme, f"{page}: {ref} is not the site's own"
