#!/usr/bin/env python3
"""Builds the website from the renders (site/render.py, site/screenshots.sh) into static pages:

    python3 site/build.py [--renders site/out] [--out site/public]

index.html (the demos, the pieces and performances, the ensemble video, the features, the window,
the download), weidinger.html (the brass's plugin: its instruments, the brass album, its features, the
same download) and credits.html (every instrument's LICENSE.md, the fonts, the MAESTRO files, the
sounds); the docs stay on GitHub (docs/), where their diagrams render. For search engines and
language models: each page's preview card (docs/social-preview.png), the start page's schema.org
data, llms.txt (the site in Markdown, linked from each page's head) and sitemap.xml (robots.txt
would count only at the host's root, outside this site). No external fonts or
scripts; the pages work without JavaScript (each play control is a link to its clip)."""
import argparse
import hashlib
import html
import json
import os
import re
import shutil

import markdown

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
HERE = os.path.join(REPO, "site")
GITHUB = "https://github.com/ccoors/dussek"
SITE = "https://ccoors.github.io/dussek/"
ITCH = "https://ccoors.itch.io/dussek"  # the same files, for the itch app (packaging/itch/README.md)
DESCRIPTION = ("A free piano, harpsichord, harp and percussion instrument: every note simulated, none recorded. "
               "LV2, CLAP, VST3 and a standalone.")
DOCS = f"{GITHUB}/tree/main/docs"
WEIDINGER = ("Weidinger, Dussek's second plugin: trumpet, horn, tenor and bass trombone and tuba, physically "
             "modeled and played as lines. A preview, in the same download.")
BRASS_SHOTS = [("weidinger-player", "The tenor trombone's Player tab: the virtual player, the air, the glide on the slide"),
               ("weidinger-mics", "The horn's bell from above and from the front, held back and to the right; the microphones at Close 1, on its axis"),
               ("weidinger-trumpet", "The trumpet's Sound tab: the mutes, growl, flutter and vibrato; its bell from the player's ears")]
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


def count(n):
    """A small number as a word ("nine"); a larger one in digits."""
    words = ["no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"]
    return words[n] if n < len(words) else str(n)


def listing(names):
    return names[0] if len(names) == 1 else ", ".join(names[:-1]) + " and " + names[-1]


def version(given=None):
    """The release the download links name: `given` (the latest published one, in CI), else the code's."""
    if given:
        return given.lstrip("v")
    cmake = open(os.path.join(REPO, "plugin", "CMakeLists.txt")).read()
    return re.search(r"project\(dussek-plugin VERSION ([0-9.]+)", cmake).group(1)


def stamp(name):
    """A file's content hash, in its URL: a browser fetches it anew once it changes (Pages caches for minutes)."""
    return hashlib.sha256(open(os.path.join(HERE, name), "rb").read()).hexdigest()[:10]


def page(title, description, body, root="", body_class="", path="", head=""):
    """`path`: the page's address under SITE (its canonical URL and its preview card's)."""
    t = open(os.path.join(HERE, "template.html")).read()
    return t.format(title=esc(title), description=esc(description), body=body, root=root, body_class=body_class,
                    url=esc(SITE + path), site=SITE, head=head, css_v=stamp("style.css"), js_v=stamp("player.js"))


def instruments():
    """The menu's instruments: (directory, menu block), hidden ones left out."""
    out = []
    for d in sorted(os.listdir(os.path.join(REPO, "instruments"))):
        f = os.path.join(REPO, "instruments", d, "instrument.json")
        if os.path.isfile(f):
            menu = json.load(open(f)).get("menu", {})
            if not menu.get("hidden"):
                out.append((d, menu))
    return out


def by_group(items):
    """{group: [item]} in the menu's order, bass to treble as on the start page; any other group after them."""
    order = ["Grand pianos", "Upright pianos", "Harpsichords", "Clavichords", "Harps", "Guitars", "Zithers", "Percussion", "Thumb pianos"]
    groups = {}
    for g, item in items:
        groups.setdefault(g, []).append(item)
    return {g: groups[g] for g in sorted(groups, key=lambda g: (order.index(g) if g in order else len(order), g))}


def installers(v):
    """The installers: (key, system, file, what it needs and holds)."""
    return [("windows", "Windows", f"dussek-{v}-windows-setup.exe", "Windows 10 or newer, 64 bit: the standalone in the Start menu, the plugins, an uninstaller."),
            ("macos", "macOS", f"dussek-{v}-macos.pkg", "Apple silicon, macOS 13.3 or newer: the app, the plugins. Untested: reports welcome."),
            ("linux", "Linux", f"dussek-{v}-linux.tar.gz", "x86-64, glibc 2.35 or newer: unpack, run ./install.sh.")]


def structured(v):
    """The start page's schema.org data: the software, free, its systems and its release."""
    data = {"@context": "https://schema.org", "@type": "SoftwareApplication", "name": "Dussek", "url": SITE,
            "description": DESCRIPTION, "applicationCategory": "MultimediaApplication",
            "applicationSubCategory": "Virtual instrument (LV2, CLAP, VST3 plugin and standalone)",
            "operatingSystem": "Linux, Windows, macOS", "softwareVersion": v,
            "downloadUrl": f"{GITHUB}/releases/tag/v{v}", "image": SITE + "social-preview.png",
            "license": "https://spdx.org/licenses/AGPL-3.0-or-later", "sameAs": [GITHUB, ITCH],
            "author": {"@type": "Person", "name": "Christian Friedrich Coors"},
            "offers": {"@type": "Offer", "price": "0", "priceCurrency": "EUR"}}
    return '<script type="application/ld+json">' + json.dumps(data, ensure_ascii=False).replace("</", "<\\/") + "</script>"


def llms_txt(v):
    """The site in Markdown for a language model's reader (llmstxt.org)."""
    groups = by_group((m.get("group", "Other"), m.get("name", d)) for d, m in instruments())
    lines = ["# Dussek", "", f"> {DESCRIPTION}", "",
             "Dussek is a physically modeled instrument: each string, bar or tongue is simulated as it is played, "
             "from measurements of freely licensed recordings; no samples ship with it. It runs as an LV2, CLAP and "
             "VST3 plugin and as a standalone on Linux, Windows and macOS. "
             "The code is under the AGPL-3.0-or-later; each instrument under CC BY 4.0 or CC BY-SA 4.0. Free of charge.",
             "", f"## Instruments ({sum(len(n) for n in groups.values())})", ""]
    lines += [f"- {g}: {', '.join(sorted(names))}" for g, names in groups.items()]
    lines += ["", f"## Download (version {v})", ""]
    lines += [f"- [{name}]({GITHUB}/releases/download/v{v}/{f}): {note}" for _, name, f, note in installers(v)]
    lines += [f"- [Release notes and the portable archives]({GITHUB}/releases/tag/v{v})",
              f"- [itch.io]({ITCH}): the same files, for the itch app's updates", "",
              "## Docs", "",
              f"- [Website]({SITE}): demos of every kind of instrument, the features, screenshots",
              f"- [Weidinger]({SITE}weidinger.html): the brass's plugin (trumpet, horn, trombones, tuba), a preview in the same download",
              f"- [README]({GITHUB}#readme): what it is, how to install and build it",
              f"- [Quick start]({GITHUB}/blob/main/docs/quick-start.md): the first notes in a host or the standalone",
              f"- [The plugin]({GITHUB}/blob/main/docs/plugin.md): every control",
              f"- [Building instruments]({GITHUB}/blob/main/docs/building-instruments.md): an instrument from your own recordings",
              f"- [Architecture]({GITHUB}/blob/main/docs/architecture.md): how the engine models strings, bars and the room",
              "", "## Optional", "",
              f"- [Credits]({SITE}credits.html): every instrument's license and the recordings it was measured from",
              f"- [Source]({GITHUB}): Rust engine, C++ plugin (DPF), Python fitting", ""]
    return "\n".join(lines)


def header(root, current=""):
    links = [("Listen", f"{root}index.html#listen"), ("Features", f"{root}index.html#features"),
             ("Brass", f"{root}weidinger.html"), ("Docs", DOCS), ("Credits", f"{root}credits.html")]
    items = "".join(f'<a href="{h}"{" aria-current=\"page\"" if n == current else ""}>{n}</a>' for n, h in links)
    return (f'<header class="bar"><a class="wordmark" href="{root}index.html"><img src="{root}icon.png" alt="" width="32" height="32">Dussek</a>'
            f'<nav aria-label="Site">{items}</nav><a class="button" href="{root}index.html#download">Download</a></header>')


def control(cls, src, title, inner, extra=""):
    """A player's control: a link to its clip (without the script the browser plays it), the page's
    player with it (player.js makes it a toggle button)."""
    return f'<a class="{cls}" href="{esc(src)}" data-src="{esc(src)}" data-title="{esc(title)}"{extra}>{inner}</a>'


def play_button(src, label, text="Play"):
    return control("play", src, label, f'<span class="play-icon" aria-hidden="true"></span><span class="play-text">{esc(text)}</span>',
                   f' aria-label="{esc(text)}: {esc(label)}"')


def by_section(clips, page_name):
    by = {}
    for c in clips:
        if c.get("page", "dussek") == page_name:
            by.setdefault(c["section"], []).append(c)
    return by


def index(clips, groups, ver):
    by = by_section(clips, "dussek")
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
        also = f"Also {listing(others)}." if others else "The only one of its kind."
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
    <h2 id="listen-title" class="showcase-title">{count(len(cards)).capitalize()} kinds of instrument</h2>
    <ol class="cards">{"".join(cards)}
    </ol>
  </div>
</section>'''

    def name(c, mark=""):
        return control("name", c["steps"][0]["file"], c["title"] + mark, esc(c.get("short", c["title"])))
    pieces = ", ".join(name(c) for c in by.get("piece", []))
    played_on = listing(list(dict.fromkeys(c["instrument"] for c in by.get("piece", []))) or ["Dussek"])
    perf = by.get("performance", [])
    # each performance marked where it is named: its own license, not the pieces' CC0
    performances = ", ".join(f'{name(c, " (MAESTRO, CC BY-NC-SA 4.0)")} <span class="name-license">(on {esc(c["instrument"])}; MAESTRO, CC BY-NC-SA 4.0)</span>' for c in perf)
    work = f'''
<section id="pieces" class="field field-yellow work" aria-labelledby="pieces-title">
  <p class="names"><span id="pieces-title" class="names-head">Whole pieces:</span> {pieces}.</p>
  <p class="names names-second">Replayed: {performances}.</p>
  <p class="work-note">The {count(len(by.get("piece", [])))} pieces were written for Dussek (CC0) and play on {esc(played_on)}; the {count(len(perf))} performances were recorded on a computer-controlled piano and play again on Dussek's grands.</p>
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

    features = f'''
<section id="features" class="field field-green features" aria-labelledby="features-title">
  <ul class="feature-list">
    <li class="feature feature-head"><h2 id="features-title">What it plays</h2>
      <div class="feature-body"><p>Each one the same phrase, plain and then with it; each step plays from its start.</p></div></li>{feature_rows(by)}
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

    assets = installers(v)
    choices = "".join(
        f'<label class="os"><input type="radio" name="os" value="{k}" data-file="{f}" data-note="{esc(n)}"{" checked" if k == "windows" else ""}><span>{name}</span></label>'
        for k, name, f, n in assets)
    files = "".join(f'<li><a href="{GITHUB}/releases/download/v{v}/{f}">{f}</a></li>' for _, _, f, _ in assets)
    download = f'''
<section id="download" class="field field-blue download" aria-labelledby="download-title">
  <div class="download-words">
    <h2 id="download-title">Download<br>Dussek</h2>
    <p>Version {v}, free: the standalone, LV2, CLAP and VST3, and every instrument; <a href="weidinger.html">Weidinger</a>,
    the brass, in the same files (a preview). Where a host offers LV2 or CLAP, prefer them to the VST3. <a href="{GITHUB}/releases/tag/v{v}">The release notes</a>; the same files
    <a href="{ITCH}">on itch.io</a>.</p>
  </div>
  <form class="installer" data-ui action="{GITHUB}/releases/download/v{v}/{assets[0][2]}" method="get">
    <p class="installer-title">Install Dussek {v}</p>
    <fieldset><legend>Your system</legend><div class="os-choices">{choices}</div></fieldset>
    <p class="installer-note" aria-live="polite">{esc(assets[0][3])}</p>
    <a class="button button-large installer-go" href="{GITHUB}/releases/download/v{v}/{assets[0][2]}">Download for Windows</a>
    <ul class="installer-files">{files}</ul>
  </form>
</section>'''

    brass = f'''
<section id="brass" class="field field-dark brass-band" aria-labelledby="brass-title">
  <h2 id="brass-title">And the brass</h2>
  <p class="work-note">{esc(WEIDINGER)} <a href="weidinger.html">Listen to Weidinger</a>.</p>
</section>'''
    return '<main id="main">' + hero + showcase + work + ensemble + features + brass + window + download + "</main>" + footer("")


def feature_rows(by):
    rows = []
    for c in by.get("feature", []):
        steps = "".join(control("step", s["file"], f'{c["title"]}: {s["label"]}', esc(s["label"])) for s in c["steps"])
        rows.append(f'''
    <li class="feature" data-feature="{esc(c["id"])}">
      <h3>{esc(c["title"])}</h3>
      <div class="feature-body">
        <p>{esc(c["listen"])}</p>
        <div class="steps" role="group" aria-label="{esc(c["title"])}: play one">{steps}</div>
      </div>
    </li>''')
    return "".join(rows)


NOTE_NAMES = ["C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "G♯", "A", "B♭", "B"]
MUTE_NAMES = {"straight": "straight", "cup": "cup", "wah": "wah", "wah_stem": "wah with its stem", "bucket": "bucket",
              "plunger": "plunger"}


def brass_facts(d):
    """A brass instrument's card line from its export: its recorded keys and its mutes."""
    doc = json.load(open(os.path.join(REPO, "instruments", d, "instrument.json")))
    note = lambda k: f"{NOTE_NAMES[k % 12]}{k // 12 - 1}"
    lo, hi = doc["recorded"]
    mutes = [MUTE_NAMES.get(m, m) for m in doc["brass"].get("order", []) if m != "open"]
    played = (f"open and with {count(len(mutes))} mutes: {listing(mutes)}" if len(mutes) > 1
              else f"open and with a {mutes[0]} mute" if mutes else "open (no mutes yet)")
    return f"{note(lo)} to {note(hi)}, {played}."


def weidinger(clips, ver):
    """Weidinger's page: the brass's plugin, in the same download as Dussek."""
    by = by_section(clips, "weidinger")
    v = esc(ver)
    hero = f'''
<section class="hero" aria-labelledby="title">
  {header("", "Brass")}
  <div class="hero-fields">
    <h1 id="title" class="hero-title"><span class="visually-hidden">Weidinger: </span><span class="hero-copy hero-copy-top" aria-hidden="true">Blown,<br>not sampled</span><span class="hero-copy hero-copy-bottom">Blown,<br>not sampled</span></h1>
    <p class="hero-lead">Weidinger is Dussek's brass: trumpet, horn, trombones and tuba, the lips and the air simulated
    as you play, a virtual player shaping each line. A preview, in Dussek's download.</p>
  </div>
</section>'''
    cards = []
    for c in by.get("demo", []):
        d = c.get("dir") or c["id"]
        cards.append(f'''
    <li class="card">
      <img src="screenshots/weidinger-{esc(d)}-bell.png" alt="The {esc(c["title"].lower())}'s bell as it is held, from above and from the front, the microphones at the player's ears" width="868" height="392" loading="lazy">
      <div class="card-panel">
        <h3>{esc(c["title"])}</h3>
        <p>{esc(brass_facts(d))}</p>
        {play_button(c["steps"][0]["file"], c["title"])}
      </div>
    </li>''')
    showcase = f'''
<section id="listen" class="showcase" aria-labelledby="listen-title">
  <div class="showcase-stage">
    <img class="showcase-ground" src="screenshots/weidinger-player.png" alt="" width="1800" height="1248">
    <h2 id="listen-title" class="showcase-title">{count(len(cards)).capitalize()} brass instruments</h2>
    <ol class="cards">{"".join(cards)}
    </ol>
  </div>
</section>'''
    names = ", ".join(control("name", c["steps"][0]["file"], c["title"], esc(c["title"])) for c in by.get("piece", []))
    work = f'''
<section id="pieces" class="field field-yellow work" aria-labelledby="pieces-title">
  <p class="names"><span id="pieces-title" class="names-head">A brass band:</span> {names}.</p>
  <p class="work-note">{count(len(by.get("piece", []))).capitalize()} pieces written for Weidinger (CC0): two trumpets, horn, trombone,
  bass trombone and tuba, each part one instrument with its mutes, growl, flutter, vibrato and air, mixed in stereo.</p>
</section>'''
    features = f'''
<section id="features" class="field field-green features" aria-labelledby="features-title">
  <ul class="feature-list">
    <li class="feature feature-head"><h2 id="features-title">What it plays</h2>
      <div class="feature-body"><p>Each one the same phrase, plain and then with it; each step plays from its start.</p></div></li>{feature_rows(by)}
  </ul>
</section>'''
    prints = "".join(
        f'<a class="print print-{i + 1}" href="screenshots/{f}.png"><img src="screenshots/{f}.png" alt="{esc(alt)}" width="1800" height="1248" loading="lazy"></a>'
        for i, (f, alt) in enumerate(BRASS_SHOTS))
    window = f'''
<section id="window" class="field field-cyan window" aria-labelledby="window-title">
  <div class="window-words"><h2 id="window-title" class="window-title">The<br>window</h2>
  <p class="window-note">Dussek's window, for a bell: the Sound tab's mutes, growl, flutter and vibrato, the Player
  tab's air and the slide's glide; each bell drawn as it is held, the microphones around it. MIDI learn, presets
  and the room as in Dussek.</p></div>
  <div class="prints">{prints}</div>
</section>'''
    download = f'''
<section id="download" class="field field-blue download" aria-labelledby="download-title">
  <div class="download-words">
    <h2 id="download-title">In Dussek's<br>download</h2>
    <p>Weidinger comes in the same files as Dussek, version {v}: its own standalone and LV2, CLAP and VST3 plugins, and
    the five instruments. The Windows installer and the macOS package offer it as a component that can be left out. A
    preview: its instruments are marked experimental, and its sound and controls may still change.
    <a href="{DOCS}/brass.md">Weidinger's docs</a>.</p>
    <p><a class="button button-large" href="index.html#download">Download Dussek and Weidinger</a></p>
  </div>
</section>'''
    return '<main id="main">' + hero + showcase + work + features + window + download + "</main>" + footer("")


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
      <li><a href="{ITCH}">On itch.io</a></li>
      <li><a href="{DOCS}">Docs</a></li>
      <li><a href="{root}credits.html">Credits</a></li>
      <li><a href="{GITHUB}/issues">Report a problem</a></li>
    </ul>
  </div>
</footer>'''


def md(text):
    """A license file's Markdown as HTML; a tag in it shown as text (its <https://…> links kept)."""
    text = re.sub(r"<(?!https?://)", "&lt;", text)
    return markdown.markdown(text, extensions=["tables", "fenced_code", "toc"])


def credits_page(out):
    items = []
    for d, menu in instruments():
        lic = os.path.join(REPO, "instruments", d, "LICENSE.md")
        text = open(lic, encoding="utf-8").read() if os.path.exists(lic) else ""
        text = re.sub(r"^# .*\n", "", text, count=1)
        items.append((menu.get("group", "Other"), (menu.get("name", d), md(text))))
    parts = []
    for g, members in by_group(items).items():
        parts.append(f"<h2>{esc(g)}</h2>")
        for name, body in sorted(members):
            parts.append(f'<section class="credit"><h3>{esc(name)}</h3>{body}</section>')
    site = f'''<h2>This website</h2>
<p>The demos, the features and the pieces were rendered by Dussek from MIDI files in the repository (CC0, except the
performances). {MAESTRO}</p>
<p>The audio input's sound: a falling sound recorded by Christian Friedrich Coors, CC0.</p>
<p>The typefaces: Inter (the Inter Project Authors) and Source Serif 4 (as "Dussek Serif"), both under the SIL Open
Font License 1.1, served from this site.</p>'''
    body = header("", "Credits") + f'<main class="doc credits"><h1>Credits</h1><p>Every instrument is measured from freely licensed recordings; each carries its license and the credits of what it was measured from.</p>{"".join(parts)}{site}</main>' + footer("")
    open(os.path.join(out, "credits.html"), "w", encoding="utf-8").write(
        page("Credits · Dussek", "The instruments' licenses and the recordings they were measured from", body, path="credits.html"))


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("--renders", default=os.path.join(HERE, "out"))
    ap.add_argument("--out", default=os.path.join(HERE, "public"))
    ap.add_argument("--version", help="the release the download links name (default: the code's version)")
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
    shutil.copy(os.path.join(REPO, "docs", "social-preview.png"), args.out)
    groups = manifest.get("groups") or {}
    v = version(args.version)
    body = index(manifest["clips"], groups, v)
    open(os.path.join(args.out, "index.html"), "w", encoding="utf-8").write(page(
        "Dussek: a physically modeled piano", DESCRIPTION, body, "", "home", head=structured(v)))
    open(os.path.join(args.out, "weidinger.html"), "w", encoding="utf-8").write(page(
        "Weidinger: physically modeled brass · Dussek", WEIDINGER, weidinger(manifest["clips"], v), "", "home weidinger",
        path="weidinger.html"))
    credits_page(args.out)
    open(os.path.join(args.out, "llms.txt"), "w", encoding="utf-8").write(llms_txt(v))
    urls = "".join(f"<url><loc>{SITE}{p}</loc></url>" for p in ("", "weidinger.html", "credits.html"))
    open(os.path.join(args.out, "sitemap.xml"), "w").write(
        f'<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">{urls}</urlset>\n')
    open(os.path.join(args.out, ".nojekyll"), "w").close()
    print(args.out)


if __name__ == "__main__":
    main()
