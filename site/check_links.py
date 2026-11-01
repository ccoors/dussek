#!/usr/bin/env python3
"""Every link of the built site resolves: each href, src, poster and data-src (the players' clips)
of every page names a file in the site, or points outside it on purpose (http, https, mailto, #).

    python3 site/check_links.py site/public      # exit 1, the broken ones listed, when any is missing"""
import glob
import html.parser
import os
import sys
from urllib.parse import urlparse


class Links(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.refs = []

    def handle_starttag(self, tag, attrs):
        for k, v in attrs:
            if k in ("href", "src", "poster", "data-src") and v:
                self.refs.append(v)


def broken(site):
    out = []
    for page in sorted(glob.glob(os.path.join(site, "**", "*.html"), recursive=True)):
        p = Links()
        p.feed(open(page, encoding="utf-8").read())
        for ref in p.refs:
            u = urlparse(ref)
            if u.scheme in ("http", "https", "mailto") or ref.startswith("#"):
                continue
            if not os.path.exists(os.path.normpath(os.path.join(os.path.dirname(page), u.path))):
                out.append(f"{os.path.relpath(page, site)} → {ref}")
    return out


if __name__ == "__main__":
    bad = broken(sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "public"))
    print("\n".join(bad) if bad else "check_links: every link resolves")
    sys.exit(1 if bad else 0)
