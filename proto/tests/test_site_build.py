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
            if k in ("href", "src", "poster", "data-src") and v:
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
    # each performance marked by its own name, apart from the CC0 pieces; the citation on the page
    page = open(f"{site}/index.html", encoding="utf-8").read()
    assert "Hawthorne" in page and "MAESTRO" in page
    clips = tomllib.load(open(f"{P.REPO}/site/clips.toml", "rb"))["clip"]
    for c in clips:
        if c["section"] == "performance":
            at = page.index(f'data-src="audio/{c["id"]}.ogg"')
            name_end = page.index("</a>", at)
            assert "CC BY-NC-SA 4.0" in page[name_end:name_end + 200], f"{c['id']}: not marked where it is named"


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


def test_a_missing_clip_is_a_broken_link(tmp_path):
    # the players' sources are links too: a render that went missing is reported
    fixture_renders(tmp_path / "out")
    r = subprocess.run([sys.executable, BUILD, "--renders", str(tmp_path / "out"), "--out", str(tmp_path / "public")],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    os.remove(tmp_path / "public" / "audio" / "grand.ogg")
    check = subprocess.run([sys.executable, f"{P.REPO}/site/check_links.py", str(tmp_path / "public")], capture_output=True, text=True)
    assert check.returncode == 1 and "audio/grand.ogg" in check.stdout + check.stderr


def test_a_release_rebuilds_the_site():
    # a release made with the workflow's own token starts no workflow by its event: the release job
    # starts the site's run itself, on main (the Pages environment deploys from main only)
    release = open(f"{P.REPO}/.github/workflows/release.yml").read()
    publish = release[release.index("  publish:"):]
    assert "gh workflow run site.yml --ref main" in publish and "actions: write" in publish


class Tags(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.tags = []

    def handle_starttag(self, tag, attrs):
        self.tags.append((tag, dict(attrs)))


def tags_of(page):
    p = Tags()
    p.feed(open(page, encoding="utf-8").read())
    return p.tags


def test_each_page_has_one_main(site):
    for page in ("index.html", "credits.html"):
        assert [t for t, _ in tags_of(f"{site}/{page}")].count("main") == 1, page


def test_play_controls_are_links_without_javascript(site):
    # each player's control is a link to its file: without the script it opens the clip; with it, the
    # page's player plays it
    controls = [(t, a) for t, a in tags_of(f"{site}/index.html") if "data-src" in a]
    assert controls
    for t, a in controls:
        assert t == "a" and a.get("href") == a["data-src"], (t, a)


def build(tmp_path, change):
    fixture_renders(tmp_path / "out")
    manifest = json.load(open(tmp_path / "out" / "clips.json"))
    change(manifest["clips"])
    json.dump(manifest, open(tmp_path / "out" / "clips.json", "w"))
    r = subprocess.run([sys.executable, BUILD, "--renders", str(tmp_path / "out"), "--out", str(tmp_path / "public")],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return open(tmp_path / "public" / "index.html", encoding="utf-8").read()


def test_the_counts_and_names_follow_the_clips(tmp_path):
    def change(clips):
        demos = [c for c in clips if c["section"] == "demo"]
        pieces = [c for c in clips if c["section"] == "piece"]
        clips.remove(demos[0])
        clips.remove(pieces[0])
        for c in pieces:
            c["instrument"] = "Grand Z"
    page = build(tmp_path, change)
    clips = tomllib.load(open(f"{P.REPO}/site/clips.toml", "rb"))["clip"]
    words = ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"]
    demos = sum(c["section"] == "demo" for c in clips) - 1
    pieces = sum(c["section"] == "piece" for c in clips) - 1
    performances = sum(c["section"] == "performance" for c in clips)
    assert f"{words[demos - 1].capitalize()} kinds of instrument" in page
    assert f"The {words[pieces - 1]} pieces were written for Dussek (CC0) and play on Grand Z" in page
    assert f"the {words[performances - 1]} performances" in page


def load_build():
    import importlib.util
    spec = importlib.util.spec_from_file_location("site_build", BUILD)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_credits_escape_raw_html():
    # a license file's text is text: a tag in it shows, it does not run; its links stay links
    out = load_build().md("A <script>alert(1)</script> and <https://example.org/x>.")
    assert "<script>" not in out and "&lt;script&gt;" in out
    assert '<a href="https://example.org/x">' in out


def test_the_serif_carries_its_new_name_only():
    # the OFL's Reserved Font Name: a modified Source Serif names itself otherwise everywhere, its
    # named instances' PostScript names too (the copyright notice, ID 0, keeps the original's)
    ttLib = pytest.importorskip("fontTools.ttLib")
    pytest.importorskip("brotli")
    for f in glob.glob(f"{P.REPO}/site/fonts/dussek-serif*.woff2"):
        names = ttLib.TTFont(f)["name"].names
        left = [(r.nameID, r.toUnicode()) for r in names if r.nameID != 0 and "Source" in r.toUnicode()]
        assert not left, f"{os.path.basename(f)}: {left[:3]}"


def test_the_pages_carry_a_preview_card(site):
    # a link to the site, shared anywhere, shows the social preview and the page's own words
    for page in ("index.html", "credits.html"):
        meta = {a.get("property") or a.get("name"): a.get("content") for t, a in tags_of(f"{site}/{page}") if t == "meta"}
        assert meta["og:image"] == "https://ccoors.github.io/dussek/social-preview.png", page
        assert meta["og:url"] == f"https://ccoors.github.io/dussek/{'' if page == 'index.html' else page}", page
        assert meta["og:title"] and meta["og:description"] == meta["description"], page
        assert meta["twitter:card"] == "summary_large_image", page
    assert os.path.getsize(f"{site}/social-preview.png") > 0


def test_the_start_page_describes_the_software(site):
    # schema.org data for search engines: what it is, that it is free, for which systems, which release
    text = open(f"{site}/index.html", encoding="utf-8").read()
    start = text.index('<script type="application/ld+json">') + len('<script type="application/ld+json">')
    data = json.loads(text[start:text.index("</script>", start)])
    load = load_build()
    assert data["@type"] == "SoftwareApplication" and data["name"] == "Dussek"
    assert data["softwareVersion"] == load.version()
    assert data["offers"]["price"] == "0"
    assert {"Linux", "Windows", "macOS"} <= set(data["operatingSystem"].split(", "))
    assert data["license"].endswith("AGPL-3.0-or-later")


def test_llms_txt_lists_the_instruments_and_the_downloads(site):
    # llms.txt (llmstxt.org): the site in plain Markdown for a language model's reader
    text = open(f"{site}/llms.txt", encoding="utf-8").read()
    v = load_build().version()
    assert text.startswith("# Dussek\n\n> ")
    for f in glob.glob(f"{P.REPO}/instruments/*/instrument.json"):
        menu = json.load(open(f)).get("menu", {})
        if not menu.get("hidden"):
            assert menu["name"] in text, menu["name"]
    for f in (f"dussek-{v}-windows-setup.exe", f"dussek-{v}-macos.pkg", f"dussek-{v}-linux.tar.gz"):
        assert f"https://github.com/ccoors/dussek/releases/download/v{v}/{f}" in text
    for word in ("LV2", "CLAP", "VST3", "AGPL-3.0-or-later", "CC BY 4.0"):
        assert word in text, word
    for page in ("index.html", "credits.html"):
        assert any(t == "link" and a.get("href") == "llms.txt" for t, a in tags_of(f"{site}/{page}")), page


def test_the_sitemap_lists_the_pages(site):
    sitemap = open(f"{site}/sitemap.xml").read()
    for url in ("https://ccoors.github.io/dussek/", "https://ccoors.github.io/dussek/credits.html"):
        assert f"<loc>{url}</loc>" in sitemap
