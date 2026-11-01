#!/usr/bin/env python3
"""Names a subset of Source Serif 4 "Dussek Serif" in every name record, as the SIL Open Font License
asks of a modified version (Source is its Reserved Font Name); the copyright notice (ID 0) stays:

    python3 site/fonts/rename.py site/fonts/dussek-serif.woff2 site/fonts/dussek-serif-italic.woff2
"""
import sys

from fontTools.ttLib import TTFont

RENAMES = [("SourceSerif4Variable", "DussekSerif"), ("SourceSerif4", "DussekSerif"), ("Source Serif 4", "Dussek Serif")]

for path in sys.argv[1:]:
    font = TTFont(path)
    for record in font["name"].names:
        if record.nameID == 0:
            continue
        text = record.toUnicode()
        for old, new in RENAMES:
            text = text.replace(old, new)
        record.string = text
    font.save(path)
    print(path)
