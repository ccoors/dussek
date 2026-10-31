#!/usr/bin/env python3
"""Test MIDI files for the engine and the plugin (no dependencies).

Writes into data/midis/test/ (or the directory given), each section named by a marker:

  velocity_ramp_c4.mid    C4 at every velocity 1 … 127, 0.8 s apart (0.4 s held): is the
                          level even over the velocities, any jump or dip?
  velocity_ramp_keys.mid  every third key A0 … C8, each at 17 velocities 1, 8, 16 … 120, 127
                          (0.35 s apart): the same across the keyboard, by ear. Each strike
                          finds its string still moving (the top keys have no dampers, the
                          bass ones fall slowly) and partly cancels it: the peaks there are no
                          measure of the velocity curve (rendered one note at a time, every key
                          rises with every step)
  repeat_accel.mid        A1, C4, C6 repeated, speeding up from 2 to 30 notes per second
                          over 10 s, 2 s at 30, velocity 80, each held half its interval;
                          then again with the sustain pedal down (the restrike on a ringing string)
  repeat_dynamics.mid     C4 and A1 at 12 notes per second: a crescendo 10 → 127 over 8 s and
                          back; then accents (127 / 40 alternating)
  repeat_overlap.mid      C4 struck again while still held (15 per second, each held three
                          intervals: note-ons on a held key); then 20 note-ons with no note-offs
                          at all, ended by one note-off

Usage: data/make_test_midi.py [OUT_DIR]
"""

import os
import struct
import sys

PPQ = 960
TEMPO = 500_000  # µs per beat: 120 bpm, 1920 ticks per second
TICKS_PER_S = PPQ * 1_000_000 / TEMPO
NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def name(key):
    return f"{NAMES[key % 12]}{key // 12 - 1}"


def vlq(n):
    out = [n & 0x7F]
    n >>= 7
    while n:
        out.append(0x80 | (n & 0x7F))
        n >>= 7
    return bytes(reversed(out))


class Track:
    """Events at times in seconds; written sorted (at one tick: markers, controllers, note-offs, note-ons)."""

    def __init__(self):
        self.events = []  # (tick, order, bytes)

    def _at(self, t, order, data):
        self.events.append((round(t * TICKS_PER_S), order, len(self.events), data))

    def marker(self, t, text):
        b = text.encode()
        self._at(t, 0, b"\xff\x06" + vlq(len(b)) + b)

    def cc(self, t, number, value):
        self._at(t, 1, bytes([0xB0, number, value]))

    def bend(self, t, value):
        """The pitch wheel at −1 … 1 (re its middle)."""
        v = max(0, min(16383, round(8192 + 8192 * value)))
        self._at(t, 1, bytes([0xE0, v & 0x7F, v >> 7]))

    def off(self, t, key):
        self._at(t, 2, bytes([0x80, key, 64]))

    def on(self, t, key, velocity):
        self._at(t, 3, bytes([0x90, key, velocity]))

    def note(self, t, key, velocity, length):
        self.on(t, key, velocity)
        self.off(t + length, key)

    def save(self, path, title):
        b = title.encode()
        body = bytearray(vlq(0) + b"\xff\x03" + vlq(len(b)) + b)
        body += vlq(0) + b"\xff\x51\x03" + TEMPO.to_bytes(3, "big")
        last = 0
        for tick, _, _, data in sorted(self.events):
            body += vlq(tick - last) + data
            last = tick
        body += vlq(round(TICKS_PER_S)) + b"\xff\x2f\x00"  # a second after the last event
        with open(path, "wb") as f:
            f.write(b"MThd" + struct.pack(">IHHH", 6, 0, 1, PPQ))
            f.write(b"MTrk" + struct.pack(">I", len(body)) + body)
        return last / TICKS_PER_S


def velocity_ramp_c4():
    tr = Track()
    for v in range(1, 128):
        t = 0.5 + (v - 1) * 0.8
        if v % 16 == 1:
            tr.marker(t, f"velocity {v}")
        tr.note(t, 60, v, 0.4)
    return tr


def velocity_ramp_keys():
    tr = Track()
    velocities = [1] + list(range(8, 121, 8)) + [127]
    t = 0.5
    for key in range(21, 109, 3):
        tr.marker(t, name(key))
        for v in velocities:
            tr.note(t, key, v, 0.25)
            t += 0.35
        t += 0.7
    return tr


def accelerate(tr, t, key, velocity):
    """2 → 30 notes per second over 10 s (the rate exponential in time), then 2 s at 30."""
    end_ramp, end = t + 10.0, t + 12.0
    while t < end:
        rate = 2.0 * 15.0 ** min(1.0, (t - (end_ramp - 10.0)) / 10.0) if t < end_ramp else 30.0
        dt = 1.0 / rate
        tr.note(t, key, velocity, dt / 2)
        t += dt
    return t


def repeat_accel():
    tr = Track()
    t = 0.5
    for pedal in (False, True):
        for key in (33, 60, 84):
            tr.marker(t, f"{name(key)}{', pedal down' if pedal else ''}")
            if pedal:
                tr.cc(t - 0.2, 64, 127)
            t = accelerate(tr, t, key, 80)
            if pedal:
                tr.cc(t + 1.5, 64, 0)
            t += 3.0
    return tr


def repeat_dynamics():
    tr = Track()
    t, dt = 0.5, 1.0 / 12
    for key in (60, 33):
        tr.marker(t, f"{name(key)} crescendo, decrescendo")
        n = round(8.0 / dt)
        for i in range(2 * n):
            x = i / (n - 1) if i < n else (2 * n - 1 - i) / (n - 1)
            tr.note(t, key, max(1, min(127, round(10 + 117 * x))), dt / 2)
            t += dt
        t += 2.0
    tr.marker(t, "C4 accents")
    for i in range(48):
        tr.note(t, 60, 127 if i % 2 == 0 else 40, dt / 2)
        t += dt
    return tr


def repeat_overlap():
    tr = Track()
    t, dt = 0.5, 1.0 / 15
    tr.marker(t, "C4 struck while held")
    for _ in range(90):
        tr.note(t, 60, 80, 3 * dt)
        t += dt
    t += 2.0
    tr.marker(t, "C4 note-ons without note-offs")
    for _ in range(20):
        tr.on(t, 60, 80)
        t += 1.0 / 8
    tr.off(t + 0.5, 60)
    return tr


FILES = {
    "velocity_ramp_c4": velocity_ramp_c4,
    "velocity_ramp_keys": velocity_ramp_keys,
    "repeat_accel": repeat_accel,
    "repeat_dynamics": repeat_dynamics,
    "repeat_overlap": repeat_overlap,
}


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "midis", "test")
    os.makedirs(out, exist_ok=True)
    for stem, make in FILES.items():
        tr = make()
        path = os.path.join(out, stem + ".mid")
        length = tr.save(path, stem)
        notes = sum(1 for e in tr.events if e[3][0] == 0x90)
        print(f"{path}: {notes} notes, {length:.1f} s")


if __name__ == "__main__":
    main()
