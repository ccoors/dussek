#!/usr/bin/env python3
"""Renders the website's clips (site/clips.toml) to loudness-matched Ogg (Opus) files and writes
their list for the pages (site/out/clips.json):

    python3 site/render.py                 # every clip (needs `just plugin` and the engine's midi2wav)
    python3 site/render.py --only grand,pedals
    python3 site/render.py --check         # the table only: names, keys, controls, files
    python3 site/render.py --video         # also the ensemble piece's video (albums/demos/mix.py)

A clip's settings name plugin controls; plugin/build/settings_line turns them into the plugin's own
Copy CLI line, so a clip plays what the plugin plays. Each clip starts just before its first note.
A feature's steps share one gain ("together": their differences in level stay) or are each
matched ("each"); either way the clip sits at -18 LUFS, its true peak at most -1 dBFS."""
import argparse
import concurrent.futures
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import tomllib

import mido

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
MIDI2WAV = os.path.join(REPO, "engine", "target", "release", "midi2wav")
SETTINGS_LINE = os.path.join(REPO, "plugin", "build", "settings_line")
SECTIONS = ("demo", "piece", "performance", "feature")
LUFS = -18.0
PEAK = -1.0
LEAD_S = 0.15  # kept before the first note


def midi_path(name):
    return os.path.join(REPO, name) if "/" in name else os.path.join(REPO, "site", "midi", f"{name}.mid")


def steps_of(clip):
    return clip.get("steps") or [{"label": clip["title"]}]


def value(clip, step, key, default=None):
    return step.get(key, clip.get(key, default))


def notes_of(path):
    """The MIDI file's note-on keys and the first one's time (s; None without notes)."""
    keys, first, now = [], None, 0.0
    for msg in mido.MidiFile(path):
        now += msg.time
        if msg.type == "note_on" and msg.velocity > 0:
            keys.append(msg.note)
            first = now if first is None else first
    return keys, first


def instrument(name):
    return json.load(open(os.path.join(REPO, "instruments", name, "instrument.json")))


def settings_line(inst_dir, settings):
    """The plugin's Copy CLI line for the controls: midi2wav's options (None, message: refused)."""
    args = [SETTINGS_LINE, inst_dir] + [f"{k}={v}" for k, v in settings.items()]
    r = subprocess.run(args, capture_output=True, text=True)
    if r.returncode != 0:
        return None, r.stderr.strip()
    tokens = shlex.split(r.stdout)
    return tokens[tokens.index("out.wav") + 1:], ""


def check(clips):
    """Every problem of the table, as "<clip id>: <what>" lines."""
    errors, seen = [], set()
    for c in clips:
        cid = c.get("id", "?")
        if cid in seen:
            errors.append(f"{cid}: the id is used twice")
        seen.add(cid)
        if c.get("section") not in SECTIONS:
            errors.append(f"{cid}: section `{c.get('section')}` is none of {', '.join(SECTIONS)}")
        if c.get("match", "together") not in ("together", "each"):
            errors.append(f"{cid}: match is `together` or `each`")
        for s in steps_of(c):
            where = f"{cid}" + (f" ({s['label']})" if "steps" in c else "")
            if "source" in s:
                if not os.path.isfile(os.path.join(REPO, s["source"])):
                    errors.append(f"{where}: no sound file {s['source']}")
                continue
            name = value(c, s, "instrument", "")
            if not os.path.isfile(os.path.join(REPO, "instruments", name, "instrument.json")):
                errors.append(f"{where}: no instrument `{name}`")
                continue
            midi = midi_path(value(c, s, "midi", ""))
            if not os.path.isfile(midi):
                errors.append(f"{where}: no MIDI file {midi}")
            else:
                keys, _ = notes_of(midi)
                lo, hi = instrument(name)["recorded"]
                out = sorted({k for k in keys if not lo <= k <= hi})
                if out:
                    errors.append(f"{where}: keys {out} outside {name}'s recorded {lo}–{hi}")
            for m in value(c, s, "morph", []):
                if not os.path.isfile(os.path.join(REPO, m.split("=")[0], "instrument.json")):
                    errors.append(f"{where}: no instrument to morph with, {m}")
            if "input" in s and not os.path.isfile(os.path.join(REPO, s["input"])):
                errors.append(f"{where}: no sound file {s['input']}")
            settings = {**c.get("set", {}), **s.get("set", {})}
            if settings and os.path.exists(SETTINGS_LINE):
                line, msg = settings_line(os.path.join("instruments", name), settings)
                if line is None:
                    errors.append(f"{where}: {msg}")
    return errors


def ff(*args):
    subprocess.run(["ffmpeg", "-v", "error", "-y", *args], check=True)


def measure(wav):
    """Integrated loudness (LUFS) and true peak (dBFS)."""
    r = subprocess.run(["ffmpeg", "-hide_banner", "-i", wav, "-af", "ebur128=peak=true", "-f", "null", "-"],
                       capture_output=True, text=True)
    lufs = float(re.findall(r"I:\s+(-?[\d.]+) LUFS", r.stderr)[-1])
    peak = float(re.findall(r"Peak:\s+(-?[\d.inf]+) dBFS", r.stderr)[-1])
    return lufs, peak


def render_step(c, s, wav, tmp, frame=None):
    """One step into `wav` (48 kHz), cut to start just before its first note and to `seconds`; with
    `frame`, also a still of its Visualization there (a demo's card picture)."""
    raw = wav + ".raw.wav"
    if "source" in s:
        ff("-i", os.path.join(REPO, s["source"]), "-ar", "48000", raw)
        start = 0.0
    else:
        name = value(c, s, "instrument")
        inst_dir = os.path.join(REPO, "instruments", name)
        settings = {**c.get("set", {}), **s.get("set", {})}
        line, msg = settings_line(inst_dir, settings)
        if line is None:
            raise SystemExit(f"{c['id']}: {msg}")
        midi = midi_path(value(c, s, "midi"))
        cmd = ["nice", "-n", "19", MIDI2WAV, inst_dir, midi, raw, *line, "--tail", str(value(c, s, "tail", 4)), "--jobs", "1"]
        for m in value(c, s, "morph", []):
            cmd += ["--morph", os.path.join(REPO, m)]
        if "input" in s:
            sound = os.path.join(tmp, f"{c['id']}-input.wav")
            ff("-i", os.path.join(REPO, s["input"]), sound)
            cmd += ["--input", sound]
        video = os.path.join(tmp, f"{c['id']}.mp4")
        if frame:
            cmd += ["--video", video, "--size", "960x720", "--fps", "30"]
        subprocess.run(cmd, check=True, capture_output=True)
        _, first = notes_of(midi)
        start = max(0.0, first - LEAD_S) if first is not None else 0.0
        if frame:
            length = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", raw],
                                          capture_output=True, text=True, check=True).stdout)
            at = start + 0.45 * (min(length, start + c.get("seconds", length)) - start)
            ff("-ss", f"{at:.2f}", "-i", video, "-frames:v", "1", "-q:v", "3", frame)
    cut = ["-ss", f"{start:.3f}"]
    if "seconds" in c:
        fade = 1.0
        cut += ["-t", str(c["seconds"]), "-af", f"afade=t=out:st={c['seconds'] - fade}:d={fade}"]
    ff(*cut, "-i", raw, "-ar", "48000", wav)
    os.remove(raw)


def render_clip(c, out_dir, tmp):
    """The clip's steps rendered, matched and encoded: its entry for clips.json."""
    steps = steps_of(c)
    wavs = [os.path.join(tmp, f"{c['id']}-{i}.wav") for i in range(len(steps))]
    frame = None
    if c["section"] == "demo":
        os.makedirs(os.path.join(os.path.dirname(out_dir), "frames"), exist_ok=True)
        frame = os.path.join(os.path.dirname(out_dir), "frames", f"{c['id']}.jpg")
    for i, (s, w) in enumerate(zip(steps, wavs)):
        render_step(c, s, w, tmp, frame if i == 0 else None)
    levels = [measure(w) for w in wavs]
    if c.get("match", "together") == "each":
        gains = [LUFS - lufs for lufs, _ in levels]
    else:
        gains = [LUFS - levels[0][0]] * len(steps)
    # no step's true peak above PEAK: the whole clip turned down as far as its loudest needs
    over = max(peak + g - PEAK for (_, peak), g in zip(levels, gains))
    gains = [g - max(0.0, over) for g in gains]
    files = []
    for i, (s, w, g) in enumerate(zip(steps, wavs, gains)):
        name = f"{c['id']}.ogg" if len(steps) == 1 else f"{c['id']}-{i + 1}.ogg"
        ff("-i", w, "-af", f"volume={g:.2f}dB", "-c:a", "libopus", "-b:a", "128k", os.path.join(out_dir, name))
        files.append({"label": s["label"], "file": f"audio/{name}"})
    name = value(c, steps[0], "instrument") if "source" not in steps[0] else c.get("instrument")
    menu = instrument(name).get("menu", {}) if name else {}
    entry = {"id": c["id"], "section": c["section"], "title": c["title"], "instrument": menu.get("name", name),
             "group": menu.get("group"), "listen": c.get("listen", ""), "license": c.get("license", "cc0"), "steps": files}
    if frame:
        entry["frame"] = f"frames/{c['id']}.jpg"
    if "short" in c:
        entry["short"] = c["short"]
    return entry


def group_members():
    """Each menu group's instruments' names, in menu order (the hidden ones left out)."""
    groups = {}
    for d in sorted(os.listdir(os.path.join(REPO, "instruments"))):
        f = os.path.join(REPO, "instruments", d, "instrument.json")
        if os.path.isfile(f):
            menu = json.load(open(f)).get("menu", {})
            if not menu.get("hidden"):
                groups.setdefault(menu.get("group", "Other"), []).append(menu.get("name", d))
    return groups


def video(out_dir):
    """The ensemble piece's video, for the web: 1280 x 720 H.264, its poster frame."""
    subprocess.run(["nice", "-n", "19", sys.executable, os.path.join(REPO, "albums", "demos", "mix.py"), "--video"], check=True)
    src = os.path.join(REPO, "albums", "demos", "out", "05-ensemble-demo-1.mp4")
    os.makedirs(out_dir, exist_ok=True)
    ff("-i", src, "-vf", "scale=1280:720", "-c:v", "libx264", "-preset", "slow", "-crf", "24", "-pix_fmt", "yuv420p",
       "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart", os.path.join(out_dir, "ensemble.mp4"))
    ff("-ss", "20", "-i", src, "-frames:v", "1", "-vf", "scale=1280:720", os.path.join(out_dir, "ensemble.jpg"))


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("--table", default=os.path.join(REPO, "site", "clips.toml"))
    ap.add_argument("--out", default=os.path.join(REPO, "site", "out"))
    ap.add_argument("--only", help="clip ids, comma-separated")
    ap.add_argument("--check", action="store_true", help="check the table, render nothing")
    ap.add_argument("--video", action="store_true", help="also the ensemble piece's video")
    args = ap.parse_args()
    clips = tomllib.load(open(args.table, "rb")).get("clip", [])
    if not os.path.exists(SETTINGS_LINE):
        sys.exit(f"render.py: {SETTINGS_LINE} is missing: build the plugin first (just plugin)")
    errors = check(clips)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        sys.exit(1)
    if args.check:
        return
    if not os.path.exists(MIDI2WAV):
        sys.exit(f"render.py: {MIDI2WAV} is missing: build the engine first (just engine)")
    only = set(args.only.split(",")) if args.only else None
    chosen = [c for c in clips if only is None or c["id"] in only]
    audio = os.path.join(args.out, "audio")
    os.makedirs(audio, exist_ok=True)
    manifest_path = os.path.join(args.out, "clips.json")
    entries = {e["id"]: e for e in json.load(open(manifest_path))["clips"]} if only and os.path.exists(manifest_path) else {}
    with tempfile.TemporaryDirectory() as tmp, concurrent.futures.ThreadPoolExecutor(os.cpu_count()) as pool:
        for entry in pool.map(lambda c: render_clip(c, audio, tmp), chosen):
            entries[entry["id"]] = entry
            print(f"{entry['id']}: {len(entry['steps'])} file(s)")
    ordered = [entries[c["id"]] for c in clips if c["id"] in entries]
    json.dump({"clips": ordered, "groups": group_members()}, open(manifest_path, "w"), indent=1, ensure_ascii=False)
    # files of steps or clips since removed from the table
    used = {os.path.basename(s["file"]) for e in ordered for s in e["steps"]}
    for f in os.listdir(audio):
        if f.endswith(".ogg") and f not in used:
            os.remove(os.path.join(audio, f))
    if args.video:
        video(os.path.join(args.out, "video"))


if __name__ == "__main__":
    main()
