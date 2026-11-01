#!/usr/bin/env python3
"""Builds the website from the renders (site/render.py, site/screenshots.sh) into static pages:

    python3 site/build.py [--renders site/out] [--out site/public]

index.html (the demos, the pieces and performances, the ensemble video, the features, the window,
the download) and credits.html (every instrument's LICENSE.md, the fonts, the MAESTRO files, the
sounds); the docs stay on GitHub (docs/), where their diagrams render. No external fonts or
scripts; the pages read without JavaScript (only the play buttons need it)."""
import argparse
import html
import json
import os
import re
import shutil

import markdown

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
HERE = os.path.join(REPO, "site")
GITHUB = "https://github.com/ccoors/dussek"
DOCS = f"{GITHUB}/tree/main/docs"
SHOTS = [("grand-visualization", "Grand A with a chord ringing: the Visualization draws each string as the engine moves it"),
         ("grand-felt", "The Felt tab: the hammers' felt, key by key"),
         ("vibraphone", "The vibraphone: its own controls, the motor among them"),
         ("grand-mics", "The Mics tab: the lid, the pillow, the microphones' level"),
         ("grand-sound", "Grand A's Sound tab and the microphones, from above and from the front")]
MAESTRO = ('Performances from the MAESTRO dataset v3.0.0 (Hawthorne et al., "Enabling Factorized Piano Music '
           'Modeling and Generation with the MAESTRO Dataset", ICLR 2019), licensed '
           '<a href="https://creativecommons.org/licenses/by-nc-sa/4.0/">CC BY-NC-SA 4.0</a>; these renders '
           'carry the same license: share them with the credit, non-commercially.')


def esc(s):
    return html.escape(s, quote=True)


def version():
    cmake = open(os.path.join(REPO, "plugin", "CMakeLists.txt")).read()
    return re.search(r"project\(dussek-plugin VERSION ([0-9.]+)", cmake).group(1)


def page(title, description, body, root="", body_class=""):
    t = open(os.path.join(HERE, "template.html")).read()
    return t.format(title=esc(title), description=esc(description), body=body, root=root, body_class=body_class)


def header(root, current=""):
    links = [("Listen", f"{root}index.html#listen"), ("Features", f"{root}index.html#features"),
             ("Docs", DOCS), ("Credits", f"{root}credits.html")]
    items = "".join(f'<a href="{h}"{" aria-current=\"page\"" if n == current else ""}>{n}</a>' for n, h in links)
    return (f'<header class="bar"><a class="wordmark" href="{root}index.html"><img src="{root}icon.png" alt="" width="32" height="32">Dussek</a>'
            f'<nav aria-label="Site">{items}</nav><a class="button" href="{root}index.html#download">Download</a></header>')


def play_button(src, label, text="Play"):
    return (f'<button class="play" type="button" data-src="{esc(src)}" data-title="{esc(label)}" aria-label="{esc(text)}: {esc(label)}">'
            f'<span class="play-icon" aria-hidden="true"></span><span class="play-text">{esc(text)}</span></button>')


def index(clips, groups, ver):
    by = {}
    for c in clips:
        by.setdefault(c["section"], []).append(c)
    v = esc(ver)
    hero = f'''
<section class="hero" aria-labelledby="title">
  {header("")}
  <div class="hero-fields">
    <h1 id="title" class="hero-title"><span class="visually-hidden">Dussek: </span><span class="hero-copy hero-copy-top" aria-hidden="true">Struck,<br>not sampled</span><span class="hero-copy hero-copy-bottom">Struck,<br>not sampled</span></h1>
    <p class="hero-lead">Dussek is a free piano, harpsichord, harp and percussion instrument for your computer:
    every note simulated as you play it, none of it recorded.</p>
  </div>
</section>'''

    cards = []
    for c in by.get("demo", []):
        group = c.get("group") or ""
        others = [n for n in groups.get(group, []) if n != c["instrument"]]
        also = f"Also {', '.join(others[:-1]) + ' and ' + others[-1] if len(others) > 1 else others[0]}." if others else "The only one of its kind."
        cards.append(f'''
    <li class="card">
      <img src="{esc(c.get("frame", f"frames/{c['id']}.jpg"))}" alt="{esc(c["instrument"])} playing its demo: the strings as the engine moves them" width="960" height="720" loading="lazy">
      <div class="card-panel">
        <h3>{esc(group)}</h3>
        <p>{esc(c["instrument"])}. {esc(also)}</p>
        {play_button(c["steps"][0]["file"], f"{c['instrument']} ({group})")}
      </div>
    </li>''')
    showcase = f'''
<section id="listen" class="showcase" aria-labelledby="listen-title">
  <div class="showcase-stage">
    <img class="showcase-ground" src="screenshots/grand-visualization.png" alt="" width="1800" height="1248">
    <h2 id="listen-title" class="showcase-title">Nine kinds of instrument</h2>
    <div class="cards-window"><ol class="cards">{"".join(cards)}
    </ol></div>
  </div>
</section>'''

    names = []
    for c in by.get("piece", []) + by.get("performance", []):
        names.append(f'<button class="name" type="button" data-src="{esc(c["steps"][0]["file"])}" data-title="{esc(c["title"])}">{esc(c.get("short", c["title"]))}</button>')
    perf = by.get("performance", [])
    perf_pianos = ", ".join(f'{esc(c["title"].split(":")[0])} on {esc(c["instrument"])}' for c in perf)
    work = f'''
<section id="pieces" class="field field-yellow work" aria-labelledby="pieces-title">
  <p class="names"><span id="pieces-title" class="names-head">Whole pieces:</span> {", ".join(names)}.</p>
  <p class="work-note">Four pieces written for Dussek (CC0) on Grand A; three recorded performances replayed: {perf_pianos}.</p>
  <p class="work-note license">{MAESTRO}</p>
</section>'''

    ensemble = f'''
<section id="ensemble" class="field field-dark ensemble" aria-labelledby="ensemble-title">
  <h2 id="ensemble-title">Seven at once</h2>
  <figure>
    <video controls preload="none" poster="video/ensemble.jpg" width="1920" height="1080">
      <source src="video/ensemble.mp4" type="video/mp4">
    </video>
    <figcaption>Ensemble Demo 1: piano, vibraphone, marimba, harp, kalimba, bells and tubular bells,
    each instrument's strings or bars drawn as the engine moves them.</figcaption>
  </figure>
</section>'''

    rows = []
    for c in by.get("feature", []):
        steps = "".join(
            f'<button type="button" class="step" data-src="{esc(s["file"])}" data-title="{esc(c["title"])}: {esc(s["label"])}" '
            f'aria-pressed="false">{esc(s["label"])}</button>' for s in c["steps"])
        rows.append(f'''
    <li class="feature" data-feature="{esc(c["id"])}">
      <h3>{esc(c["title"])}</h3>
      <div class="feature-body">
        <p>{esc(c["listen"])}</p>
        <div class="steps" role="group" aria-label="{esc(c["title"])}: play one">{steps}</div>
      </div>
    </li>''')
    features = f'''
<section id="features" class="field field-green features" aria-labelledby="features-title">
  <ul class="feature-list">
    <li class="feature feature-head"><h2 id="features-title">What it plays</h2>
      <div class="feature-body"><p>Each one the same phrase, plain and then with it. Switch while it plays: it keeps its place.</p></div></li>{"".join(rows)}
  </ul>
</section>'''

    prints = "".join(
        f'<a class="print print-{i + 1}" href="screenshots/{f}.png"><img src="screenshots/{f}.png" alt="{esc(alt)}" width="1800" height="1248" loading="lazy"></a>'
        for i, (f, alt) in enumerate(SHOTS[:3]))
    window = f'''
<section id="window" class="field field-cyan window" aria-labelledby="window-title">
  <div class="window-words"><h2 id="window-title" class="window-title">The<br>window</h2>
  <p class="window-note">The plugin and the standalone share one window: every control by its instrument, the microphones
  around it, the strings as they move. <a href="screenshots/grand-mics.png">The Mics tab</a>, <a href="screenshots/grand-sound.png">the Sound tab</a>.</p></div>
  <div class="prints">{prints}</div>
</section>'''

    assets = [("windows", "Windows", f"dussek-{v}-windows-setup.exe", "Windows 10 or newer, 64 bit: the standalone in the Start menu, the plugins, an uninstaller."),
              ("macos", "macOS", f"dussek-{v}-macos.pkg", "Apple silicon, macOS 13.3 or newer: the app, the plugins. Untested: reports welcome."),
              ("linux", "Linux", f"dussek-{v}-linux.tar.gz", "x86-64, glibc 2.35 or newer: unpack, run ./install.sh.")]
    choices = "".join(
        f'<label class="os"><input type="radio" name="os" value="{k}" data-file="{f}" data-note="{esc(n)}"{" checked" if k == "windows" else ""}><span>{name}</span></label>'
        for k, name, f, n in assets)
    files = "".join(f'<li><a href="{GITHUB}/releases/download/v{v}/{f}">{f}</a></li>' for _, _, f, _ in assets)
    download = f'''
<section id="download" class="field field-blue download" aria-labelledby="download-title">
  <div class="download-words">
    <h2 id="download-title">Download<br>Dussek</h2>
    <p>Version {v}, free: the standalone, LV2, CLAP and VST3, and every instrument. Where a host offers LV2 or CLAP,
    prefer them to the VST3. <a href="{GITHUB}/releases/tag/v{v}">The release notes</a>.</p>
  </div>
  <form class="installer" data-ui action="{GITHUB}/releases/download/v{v}/{assets[0][2]}" method="get">
    <p class="installer-title">Install Dussek {v}</p>
    <fieldset><legend>Your system</legend>{choices}</fieldset>
    <p class="installer-note" aria-live="polite">{esc(assets[0][3])}</p>
    <a class="button button-large installer-go" href="{GITHUB}/releases/download/v{v}/{assets[0][2]}">Download for Windows</a>
    <ul class="installer-files">{files}</ul>
  </form>
</section>'''

    return hero + showcase + work + ensemble + features + window + download + footer("")


def footer(root):
    return f'''
<footer class="footer field-dark">
  <p class="footer-name">Dussek</p>
  <div class="footer-cols">
    <p>Named after Jan Ladislav Dussek (1760–1812), the pianist who had the piano's keyboard extended to five and
    a half octaves. Code under the AGPL-3.0-or-later; each instrument under CC BY 4.0 or CC BY-SA 4.0.</p>
    <ul>
      <li><a href="{GITHUB}">The source on GitHub</a></li>
      <li><a href="{GITHUB}/releases">Releases</a></li>
      <li><a href="{DOCS}">Docs</a></li>
      <li><a href="{root}credits.html">Credits</a></li>
      <li><a href="{GITHUB}/issues">Report a problem</a></li>
    </ul>
  </div>
</footer>'''


def md(text):
    return markdown.markdown(text, extensions=["tables", "fenced_code", "toc"])


def credits_page(out):
    groups = {}
    for d in sorted(os.listdir(os.path.join(REPO, "instruments"))):
        f = os.path.join(REPO, "instruments", d, "instrument.json")
        if not os.path.isfile(f):
            continue
        menu = json.load(open(f)).get("menu", {})
        if menu.get("hidden"):
            continue
        lic = os.path.join(REPO, "instruments", d, "LICENSE.md")
        text = open(lic, encoding="utf-8").read() if os.path.exists(lic) else ""
        text = re.sub(r"^# .*\n", "", text, count=1)
        groups.setdefault(menu.get("group", "Other"), []).append((menu.get("name", d), md(text)))
    # the menu's order, bass to treble as on the start page; any other group after them
    order = ["Grand pianos", "Upright pianos", "Harpsichords", "Clavichords", "Harps", "Guitars", "Zithers", "Percussion", "Thumb pianos"]
    parts = []
    for g in sorted(groups, key=lambda g: (order.index(g) if g in order else len(order), g)):
        parts.append(f"<h2>{esc(g)}</h2>")
        for name, body in sorted(groups[g]):
            parts.append(f'<section class="credit"><h3>{esc(name)}</h3>{body}</section>')
    site = f'''<h2>This website</h2>
<p>The demos, the features and the pieces were rendered by Dussek from MIDI files in the repository (CC0, except the
performances). {MAESTRO}</p>
<p>The audio input's sound: a falling sound recorded by Christian Friedrich Coors, CC0.</p>
<p>The typefaces: Inter (the Inter Project Authors) and Source Serif 4 (as "Dussek Serif"), both under the SIL Open
Font License 1.1, served from this site.</p>'''
    body = header("", "Credits") + f'<main class="doc credits"><h1>Credits</h1><p>Every instrument is measured from freely licensed recordings; each carries its license and the credits of what it was measured from.</p>{"".join(parts)}{site}</main>' + footer("")
    open(os.path.join(out, "credits.html"), "w", encoding="utf-8").write(
        page("Credits · Dussek", "The instruments' licenses and the recordings they were measured from", body))


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("--renders", default=os.path.join(HERE, "out"))
    ap.add_argument("--out", default=os.path.join(HERE, "public"))
    args = ap.parse_args()
    manifest = json.load(open(os.path.join(args.renders, "clips.json")))
    if os.path.exists(args.out):
        shutil.rmtree(args.out)
    os.makedirs(args.out)
    for sub in ("audio", "frames", "video", "screenshots"):
        if os.path.isdir(os.path.join(args.renders, sub)):
            shutil.copytree(os.path.join(args.renders, sub), os.path.join(args.out, sub))
    shutil.copytree(os.path.join(HERE, "fonts"), os.path.join(args.out, "fonts"))
    for f in ("style.css", "player.js"):
        shutil.copy(os.path.join(HERE, f), args.out)
    shutil.copy(os.path.join(REPO, "packaging", "icon", "dussek.png"), os.path.join(args.out, "icon.png"))
    groups = manifest.get("groups") or {}
    body = index(manifest["clips"], groups, version())
    open(os.path.join(args.out, "index.html"), "w", encoding="utf-8").write(page(
        "Dussek: a physically modeled piano",
        "A free piano, harpsichord, harp and percussion instrument: every note simulated, none recorded. LV2, CLAP, VST3 and a standalone.",
        body, "", "home"))
    credits_page(args.out)
    open(os.path.join(args.out, ".nojekyll"), "w").close()
    print(args.out)


if __name__ == "__main__":
    main()
