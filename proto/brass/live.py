"""Play the model live from a MIDI keyboard, with or without the virtual player. Python for the player and
the window, C for the sound (rt.c). usage: live.py [--block 128] [--mute straight] [--no-virtual] [--solo] [--list] [--silent] [--bench seconds]

The keys: velocity and overlap (slur) always. Auto: a line is one trumpet, a chord a section (see Section); Solo: one
trumpet, the last key wins. The virtual player adds the air's shape, vibrato, intonation.
Wheels and controllers: pitch wheel +-2 semitones, CC1 (the mod wheel) adds air above the keys', CC11 or CC2
sets the air (takes over once moved), CC70 vibrato (above 0 it replaces the player's), CC21 growl, CC23 flutter,
aftertouch whichever the window says. The window's Learn buttons move any of them to another controller."""
import collections, ctypes, math, os
import numpy as np
from scipy.signal import butter, sosfilt, sosfiltfilt, firwin
import auto_player, compare5, fit4, fit5, player, rt_tables, sim, spec

HERE = os.path.dirname(os.path.abspath(__file__))
SR = sim.SR
OS = fit4.OS
CTRL = 64  # samples per control step: 1.5 ms
DT = CTRL / SR
DUCK = 3  # control steps of the fade before a note that starts from rest (4.4 ms)
T5 = compare5.T5
TABLE = rt_tables.TABLE
MAKEUP = {"open": 0.0, "straight": 5.5, "cup": 0.0, "wah": 0.0, "wah_stem": 7.0, "bucket": 3.5, "plunger": 0.0}
D = ctypes.POINTER(ctypes.c_double)
# which controller plays what (the window's Learn buttons change them): the wheel adds air, a knob or pedal sets it
SPEAK = spec.load("speak", {})  # "key_dyn": air x; "key_dyn_first": the attack's push (speak_fit.py)
LEVEL = spec.load("level", {})  # per mute, "key": output dB at the air's four anchors; "key_dyn": that dynamics' table's own step (level_fit.py)
NOISE_HZ = (750, 1414, 2828, 5657, 11314)
CONTROLS = {"wheel": 1, "air": 11, "vibrato": 70, "growl": 21, "flutter": 23, "glide": 65, "glide_time": 5, "plunger": 4}
GLIDE_SPAN, GLIDE_T, AUTO_GLIDE = 6, 0.22, ("auto" if spec.S.get("slide") else "off")  # a tritone, the slide's whole travel; s for a minor third
H_X, DYNS = (0.315, 0.59, 0.9), ("pp", "mf", "ff")  # the air each table's filter was fitted at (harm_fit.py)
LEVEL_X = (0.315, 0.55, 0.9, 1.0)  # the air's anchors of the level, 40 to 127: below 40 a note is near its threshold, its level says little
GROWL, FLUTTER_DEPTH, SLUR_KICK = 0.3, 0.6, 0.5
SLUR_HARD = 100 / 127
GAIN = 0.25
TUNE_FOLLOW = 0.6  # how much of the slides' difference the note needs as the air moves (measured on swells)
# a slur: the lips ease past the note (cents, s) and the air dips, all after the key: nothing can come before
SLACK_DOWN, SLACK_UP, SLACK_T, DIP, DIP_T = 200.0, 100.0, 0.04, 0.8, 0.04
# Auto: keys this close are a chord (s); a later key this near one held is a slur (semitones), further is more harmony.
# On one session's 39 overlaps: legato stayed within 2 semitones and lifted within 151 ms, harmony was 4 or wider and
# stayed down at least 1.57 s. A key still down under a slur: a silent trumpet starts on it (s after the slur), speaks
# unheard (s), then fades in (s), so the note comes back without a new attack, before anything can be heard of it in legato
CHORD_S, SLUR_SPAN = 0.03, 5
TAKE_S, HIDE_S, RISE_S = 0.07, 0.09, 0.06
VOICES = 6
SEATS = (0.0, -0.3, 0.3, -0.6, 0.6, -0.15)  # across the stereo image; the first alone is the mono soloist


def attack_rise(periods, knee, f):
    """The attack's rise to its pressure, s: `periods` periods of the note at least 8 ms, blended back to 8 ms over the octave
    above `knee` (0: no knee). A low note rising fast shocks the bore into ringing louder than it then holds; a middle one
    rising slowly speaks late."""
    if periods <= 0:
        return 0.008
    slow = max(0.008, periods / f)
    if knee <= 0 or f <= knee:
        return slow
    w = min(1.0, math.log2(f / knee))
    return math.exp(math.log(slow) * (1 - w) + math.log(0.008) * w)


def attack_share(share, periods, knee, f):
    """The share of its top the attack's rise starts from: `share` under `knee`, blended out over the octave above it, as the
    rise is blended back to 8 ms. It takes the slow rise's wait under the lips' threshold away; an 8 ms rise has none to take,
    and a step of air would start it. No slow rise (periods 0): none."""
    if periods <= 0:
        return 0.0
    if knee <= 0 or f <= knee:
        return share
    return share * (1 - min(1.0, math.log2(f / knee)))


_LIB = []


def lib():
    """The sound's library, loaded once per process."""
    if _LIB:
        return _LIB[0]
    l = ctypes.CDLL(spec.build_c("rt.c", ["-O3", "-march=native", "-ffast-math"]))
    l.rt_new.restype = ctypes.c_void_p
    l.rt_filters.argtypes = [ctypes.c_void_p, D, ctypes.c_int, D, ctypes.c_int]
    dbl = ctypes.c_double
    l.rt_process.argtypes = [ctypes.c_void_p, ctypes.c_int, dbl, dbl, dbl, dbl, dbl, dbl, dbl, dbl, ctypes.c_int, D, D, D, D,
                             ctypes.c_int, D, D, D, D, dbl, dbl, dbl, dbl, dbl, D, ctypes.c_long, dbl, D, ctypes.c_int, D]
    _LIB.append(l)
    return l


_STREAMS = {}


def streams(seed):
    """What a seed draws: the breath's noise, the air's and the lips' wobble, the growl's band, the flutter's jitter
    (noise, wp, wl, band, jit). The same for every player of that seed, so built once per process (the noise's 2M
    samples were three quarters of a render): shared, the noise read-only."""
    key = (seed, tuple(spec.S.get("noise", ())), os.environ.get("BREATH_DB", "0"))
    if key in _STREAMS:
        return _STREAMS[key]
    rng = np.random.default_rng(seed)
    n = 1 << 21
    nz = sosfilt(butter(2, [500, 6000], "bandpass", fs=SR * OS, output="sos"), rng.standard_normal(n))
    noise = (nz * 0.015 / np.std(nz)).astype(np.float64)
    if "noise" in spec.S:  # the breath's balance per band, heard against the recordings (BREATH_DB moves it all, by ear)
        fq = np.fft.rfftfreq(n, 1 / (SR * OS))
        g = np.interp(np.log(np.maximum(fq, 1.0)), np.log(NOISE_HZ), spec.S["noise"]) + float(os.environ.get("BREATH_DB", 0))
        noise = np.fft.irfft(np.fft.rfft(noise) * 10 ** (g / 20), n).astype(np.float64)
    noise.setflags(write=False)
    k = int(60 / DT)
    lp = butter(2, 4, "lowpass", fs=1 / DT, output="sos")
    wp = sosfiltfilt(lp, rng.standard_normal(k)); wp = (wp * 0.05 / np.std(wp)).tolist()
    wl = sosfiltfilt(lp, rng.standard_normal(k)); wl = (wl * 0.003 / np.std(wl)).tolist()
    g = sosfilt(butter(2, [55, 130], "bandpass", fs=1 / DT, output="sos"), rng.standard_normal(k))
    band = (g / np.std(g)).tolist()
    j = rng.standard_normal(k // 40 + 2)
    jit = np.interp(np.arange(k), np.arange(len(j)) * 40, j).tolist()
    _STREAMS[key] = (noise, wp, wl, band, jit)
    return _STREAMS[key]


def ptr(a):
    return a.ctypes.data_as(D)


def plays(m):
    """The keys the export plays (proto/export_brass.py `playable`): `below` under the table, `above` over it."""
    t = rt_tables.TABLE
    return t[0] - spec.S.get("below", 6) <= m <= t[-1] + spec.S.get("above", 0)


class Live:
    """The player: the keys and controllers in, a block of sound out. One note at a time; the last key wins."""

    def __init__(self, mute="open", virtual=True, seed=1):
        self.lib = lib()
        self.state = self.lib.rt_new()
        self.tables = {}
        self.mute = mute
        self.next_mute = mute
        self.table = self.load(mute)
        self.virtual = virtual
        self.seed = seed
        self.noise, self.wp, self.wl, self.band, self.jit = streams(seed)
        self.dec = firwin(96, 20000, fs=SR * OS).astype(np.float64)
        _, lp2 = fit4.lips(1.0, fit5.Q)
        self.lr, self.lq, self.ls, self.lc = (np.array([l[i] for l in lp2], float) for i in range(4))
        self.nl = player.NL * T5["nl"] * SR * OS
        self.t_idx = 0
        self.held = []  # keys down, oldest first
        self.cur = None  # the note sounding
        self.tau = 0.0
        self.air_env = 0.0
        self.kick = 0.0
        self.releasing = None
        self.duck, self.pending, self.pending_off = 0, None, False  # the fade before a note from rest, the note waiting
        self.pm, self.lip, self.burst = 0.0, 200.0, 1.0
        self.rise = 0.008  # the attack's rise to its pressure, s
        self.rise_from = 0.0  # the share of its top it starts from
        self.pb, self.vib_knob, self.pedal, self.wheel = 0.0, 0.0, None, 0.0
        self.glide, self.glide_t, self.glide_mode, self.gl = False, GLIDE_T, AUTO_GLIDE, None
        self.at_a, self.at_b, self.at_now = None, None, None
        self.lip_trim = 0.0
        self.map = dict(CONTROLS)  # what each controller number plays
        self.learning = None
        self.smooth_air = None
        self.growl, self.flutter, self.at_growl, self.at_flutter = 0.0, 0.0, 0.0, 0.0
        self.aftertouch_to = "growl"
        self.vib_phase, self.fl_phase = 0.0, 0.0
        self.count = 0
        self.level, self.last_air = None, 80.0
        self.level_to, self.level_now, self.level_g = 0.0, 0.0, 1.0
        self.plunger_cc, self.opened, self.h_now = None, 0.0, None
        self.last_off = -9.0
        self.now = 0.0
        self.out = np.zeros(CTRL)
        self.E_r, self.E_i, self.r_r, self.r_i = (np.zeros(64) for _ in range(4))
        self.m_count = 0
        self.events = collections.deque()
        self.gain = GAIN * 10 ** ((MAKEUP[mute] + spec.S.get("gain_db", 0.0)) / 20)

    def load(self, mute):
        if mute not in self.tables:
            self.tables[mute] = rt_tables.load(mute)
        return self.tables[mute]

    def post(self, *ev):
        self.events.append(ev)

    def handle(self, ev):
        k = ev[0]
        if k == "on":
            self.note_on(ev[1], ev[2])
        elif k == "off":
            self.note_off(ev[1])
        elif k == "pb":
            self.pb = ev[1]
        elif k == "cc":
            c, v = ev[1], ev[2]
            if self.learning and c not in (120, 123):
                self.map = {k: n for k, n in self.map.items() if n != c}
                self.map[self.learning] = c
                self.learning = None
                return
            what = next((k for k, n in self.map.items() if n == c), None)
            if what == "wheel":
                self.wheel = v
            elif what == "vibrato":
                self.vib_knob = v / 127
            elif what == "air" or c == 2:
                self.pedal = v
            elif what == "growl":
                self.growl = v / 127
            elif what == "flutter":
                self.flutter = v / 127
            elif what == "glide":  # a pedal, or a knob past the middle
                self.glide = v >= 64
            elif what == "plunger":
                self.plunger_cc = v / 127
            elif what == "glide_time":
                self.glide_t = 0.04 * 30 ** (v / 127)  # 40 ms to 1.2 s
            elif c == 120 or c == 123:
                self.held.clear(); self.release()
        elif k == "at":
            if self.aftertouch_to == "growl":
                self.at_growl, self.at_flutter = ev[1] / 127, 0.0
            else:
                self.at_flutter, self.at_growl = ev[1] / 127, 0.0

    def near(self, d):
        x = max(0.0, min(1.0, (d - 0.2) / 0.7))
        return min(compare5.DYN, key=lambda k: abs(compare5.DYN[k] - x))

    def glides(self, m, v):
        """Does this slur go by the slide? When held down for it; or, left to the player, a soft key after a long note, close by.
        Only where the same note can be drawn there: past that the lips change partial, an ordinary slur."""
        if not spec.S.get("slide") or self.cur is None or self.releasing is not None:
            return False
        if abs(m - self.base) > GLIDE_SPAN:
            return False
        if self.glide or self.glide_mode == "on":
            return True
        return self.glide_mode == "auto" and self.virtual and v <= 70 and self.tau >= 0.35 and abs(m - self.cur) <= 5

    def slide_to(self, m):
        """The same note, the tube drawn out: nothing starts, the pitch moves as the modes and lips do. It ends scaled from its table."""
        now = self.warp
        near = min(TABLE, key=lambda t: abs(t - m))
        # this mode goes where the key's own table has its mode: that is where the lips' setting for the key plays in tune
        n = self.table["notes"][near]
        fm = np.abs(n["modes"][self.dyn_here][0].imag) / (2 * np.pi)
        there = 1200 * math.log2(fm[np.argmin(np.abs(np.log(fm / n["f"])))] / n["f"])
        self.gl = (now, float(m - self.base) + (there - self.mode_here) / 100, self.now, self.glide_t * (max(abs(m - self.cur), 0.5) / 3) ** 0.5)
        self.trim_a, self.trim_b = self.lip_trim, (there - self.mode_here) / 100  # the lips stay on the key, not on that offset
        self.at_a = self.at_now if self.at_b else self.at_of(self.base, self.settings)
        # the air and lips go where the key's own table has them: a higher note needs more, or it dies on the way up
        self.at_b = self.at_of(near, [self.setting_of(m, near, d) for d in ("pp", "mf", "ff")])
        self.at_now = self.at_a
        self.level_from, self.gl_g = self.level_now, 0.0
        self.cur = self.key = m

    def at_of(self, key, settings):
        lp, ce, top = self.table["at"][key]
        return (list(lp), list(ce), top, [tuple(q) for q in settings])

    def waits(self, slur):
        """A tongued note on a release still sounding, with attack_from_rest: the last note's state would ring into this attack,
        so it fades out over DUCK control steps and the note then starts from rest."""
        return self.pending is not None or (bool(spec.S.get("attack_from_rest")) and not slur and self.releasing is not None and self.cur is not None)

    def start(self, m, v, slur):
        if self.waits(slur):
            if self.pending is None:  # a key during the fade takes the waiting note's place
                self.duck = DUCK
            self.pending, self.pending_off = (m, v), False
            return
        self.start_now(m, v, slur)

    def start_now(self, m, v, slur):
        if slur and self.glides(m, v):
            self.slide_to(m)
            return
        self.gl, self.at_b, self.at_now, self.lip_trim = None, None, None, 0.0
        base, warp = min(TABLE, key=lambda t: abs(t - m)), 0.0
        warp = float(m - base)
        fresh = self.now - self.last_off > 1.0
        self.shape = auto_player.Shape(self.count, m, v, slur, fresh, self.level, self.last_air, self.seed)
        self.level = self.shape.level
        self.count += 1
        prev_lip = self.lip
        self.key, self.v, self.slur, self.base, self.warp = m, v, slur, base, warp
        self.tau = 0.0
        self.releasing = None
        d = self.dyn_level(0.0) / 127
        dyn = self.near(d)
        n = self.table["notes"][base]
        self.note_table = n
        self.mu = spec.lip_mu(n["f"])
        # the lips' and air's setting for this key on this mute, per dynamics: followed as the air moves
        self.settings = [self.setting_of(m, base, d) for d in ("pp", "mf", "ff")]
        sp, rc = n["modes"][dyn]
        self.T = 1 / (SR * OS)
        self.sp, self.rc = sp, rc
        self.m_count = len(sp)
        self.rT = rc * self.T / 2
        if "open" in n:
            self.opened = self.opened if slur else 0.0
        self.h_fading = self.h_now  # the filter's fade still reads the one before
        self.h_x = max(d, 0.2)
        self.h = self.h_now = self.leaving(self.h_x)
        self.lib.rt_filters(self.state, ptr(self.h), len(self.h), ptr(n["r"]), len(n["r"]))
        # where each dynamics' slide puts the note's mode: more air plays sharper, the player corrects by ear
        modes_c = []
        for dd in ("pp", "mf", "ff"):
            fm = np.abs(n["modes"][dd][0].imag) / (2 * np.pi)
            modes_c.append(float(1200 * np.log2(fm[np.argmin(np.abs(np.log(fm / n["f"])))] / n["f"])))
        self.modes_c, self.mode_here, self.dyn_here = modes_c, modes_c[("pp", "mf", "ff").index(dyn)], dyn
        self.lip_from = prev_lip
        self.down = slur and self.cur is not None and m < self.cur
        self.cur = m
        vel = v / 100
        if slur:
            self.kick = SLUR_KICK * (0.6 + 0.4 * SLUR_HARD)
            self.first, self.scoop = 1.0, 0.0
            self.burst_add = 0.0
            return
        x = max(0.0, min(1.0, (d - 0.2) / 0.7))
        a = compare5.attack_for(x)
        lean = a["first"] if spec.S.get("attack_lean", True) else max(a["first"], 1.0)
        self.first, self.scoop = max(lean, self.push(m, d)), a["scoop"]
        # the pressure's rise: a few periods of the note, else a low note's bore rings louder than it then holds
        self.rise = attack_rise(spec.S.get("attack_rise_periods", 0.0), spec.S.get("attack_rise_hz", 0.0), n["f"])
        self.rise_from = attack_share(min(max(spec.S.get("attack_rise_from", 0.0), 0.0), 1.0), spec.S.get("attack_rise_periods", 0.0),
                                      spec.S.get("attack_rise_hz", 0.0), n["f"])
        if "scoop_span" in spec.S:  # lips slack by more than the way to the next mode down catch on that mode first
            fm = np.abs(n["modes"][dyn][0].imag) / (2 * np.pi)
            below = fm[fm < n["f"] * 0.97]
            if len(below):
                self.scoop = min(self.scoop, spec.S["scoop_span"] * 1200 * math.log2(n["f"] / below.max()))
        self.kick = a["kick"] * (0.4 + 0.6 * min(vel, 1.2))
        self.burst_add = a["burst"] * min(vel, 1.2)
        self.gain = GAIN * 10 ** ((MAKEUP[self.mute] + spec.S.get("gain_db", 0.0)) / 20)
        self.level_now = self.level_db(max(d, 0.2))  # a new note starts at its own level
        self.level_g = 10 ** (self.level_now / 20)

    def leaving(self, x):
        """What leaves the bell at this air: between the tables' filters, or all the brightening falls at one velocity, where a note changes table; and the
        plunger's hand between shut and open."""
        n = self.note_table
        i = 0 if x < H_X[1] else 1
        g = max(0.0, min(1.0, (x - H_X[i]) / (H_X[i + 1] - H_X[i])))
        a, b = DYNS[i], DYNS[i + 1]
        h = n["h"][a] + (n["h"][b] - n["h"][a]) * g
        if "open" in n:
            o = n["open"][a] + (n["open"][b] - n["open"][a]) * g
            h = h + (o - h) * self.opened
        return h

    def level_db(self, x):
        """The output's level for this key at this air, between the fitted anchors."""
        if not LEVEL:
            return 0.0
        k, t = min(TABLE, key=lambda t: abs(t - self.cur)), LEVEL.get(self.mute, LEVEL["open"])
        y = float(np.interp(x, LEVEL_X, t[str(k)])) + t.get(f"{k}_{self.dyn_here}", 0.0)
        if self.gl:  # a glide takes the level along with the air
            y = self.level_from + (y - self.level_from) * self.gl_g
        return y

    def setting_of(self, key, base, dyn):
        dc, air = self.note_table["settings"][dyn]
        o = rt_tables.CELLS.get(f"{'open' if self.mute == 'plunger' else self.mute}_{key}_{dyn}")  # the plunger plays as open
        air *= SPEAK.get(f"{key}_{dyn}", 1.0)
        return (dc + o[0], air * o[1]) if o else (dc, air)

    def push(self, key, x):
        """The attack's push for this key at this air: at least what it takes to speak at once."""
        a, b, c = (SPEAK.get(f"{key}_{d}_first", 0.0) for d in ("pp", "mf", "ff"))
        w = max(0.0, min(1.0, (x - 0.2) / 0.7))
        return a + (b - a) * w / 0.55 if w < 0.55 else b + (c - b) * (w - 0.55) / 0.45

    def note_on(self, m, v):
        if not plays(m):
            return
        slur = bool(self.held) and self.cur is not None
        self.held = [k for k in self.held if k != m] + [m]
        if not slur and self.next_mute != self.mute and not self.waits(slur):
            self.mute = self.next_mute
            self.table = self.load(self.mute)
        self.start(m, v, slur)

    def note_off(self, m):
        was_last = self.held and self.held[-1] == m
        self.held = [k for k in self.held if k != m]
        if was_last:
            if self.held:  # back to the key still down: a slur
                self.start(self.held[-1], self.v, True)
            elif self.pending is not None:  # the waiting note's key came up: it still sounds, and releases at once
                self.pending_off = True
            else:
                self.release()

    def release(self):
        if self.pending is not None:  # the note waiting for the fade never starts; the fade ends in silence
            self.pending, self.pending_off = None, False
            return
        if self.cur is None:
            return
        self.releasing = 0.0
        self.rel_len = 0.02 + 0.6 * ((60 if self.tau > 0.8 else 40) / 127) ** 2
        self.last_air = self.air_now if hasattr(self, "air_now") else 80.0
        self.last_off = self.now

    def dyn_level(self, t):
        """The air (0-127): a knob or pedal if one moved, else the keys; the wheel adds to that."""
        if self.pedal is not None:
            b = self.pedal
        elif self.virtual:
            b = self.shape.air(t)
        else:
            b = min(127.0, auto_player.level_of(self.v, self.key))
        return b + (127.0 - b) * self.wheel / 127.0  # the wheel's whole travel, from there to the top

    def step(self):
        """One control step: returns CTRL samples."""
        out = self.step_note()
        if not self.duck:
            return out
        done = (DUCK - self.duck) * CTRL
        out *= 0.5 + 0.5 * np.cos(np.pi * (done + np.arange(CTRL) + 1.0) / (DUCK * CTRL))
        self.duck -= 1
        if self.duck:
            return out
        self.reset()  # faded out: the state cleared, the note waiting started from rest
        if self.pending is not None:
            (m, v), self.pending = self.pending, None
            if self.next_mute != self.mute:
                self.mute = self.next_mute
                self.table = self.load(self.mute)
            self.start_now(m, v, False)
            if self.pending_off:
                self.pending_off = False
                self.release()
        else:  # the note waiting was let go: the last one ends with the fade (its release's air would blow the bore again)
            self.cur, self.releasing, self.pm = None, None, 0.0
        return out

    def step_note(self):
        while self.events:
            self.handle(self.events.popleft())
        self.now += DT
        i = self.t_idx = (self.t_idx + 1) % len(self.wp)
        pm1, lip1, burst1, ratio = 0.0, self.lip, 1.0, 1.0
        if self.cur is not None:
            t = self.tau
            L = self.dyn_level(t)
            # controllers come in steps: eased over about 10 ms (from the note's own level at its start)
            self.smooth_air = L if self.smooth_air is None or t == 0.0 else self.smooth_air + (L - self.smooth_air) * 0.14
            L = self.smooth_air
            self.air_now = L
            if not self.releasing or self.releasing < self.rel_len:
                self.last_air = L
            d = L / 127
            if self.gl:
                a, b, t0, length = self.gl
                g = min((self.now - t0) / length, 1.0)
                g = 0.5 - 0.5 * math.cos(math.pi * g)
                self.gl_g = g
                self.warp = a + (b - a) * g
                self.lip_trim = self.trim_a + (self.trim_b - self.trim_a) * g
                A, B = self.at_a, self.at_b
                self.at_now = ([p + (q - p) * g for p, q in zip(A[0], B[0])], [p + (q - p) * g for p, q in zip(A[1], B[1])], A[2] + (B[2] - A[2]) * g,
                               [(p[0] + (q[0] - p[0]) * g, p[1] + (q[1] - p[1]) * g) for p, q in zip(A[3], B[3])])
                if self.now - t0 >= length:
                    self.gl = None
            if self.at_now:
                lp, ce, top, st = self.at_now
            else:
                lp, ce, top = self.table["at"][self.base]
                st = self.settings
            x = max(d, 0.2)
            if x <= 0.55:
                w = (x - 0.2) / 0.35; l, c = lp[0] + (lp[1] - lp[0]) * w, ce[0] + (ce[1] - ce[0]) * w
            elif x <= 0.9:
                w = (x - 0.55) / 0.35; l, c = lp[1] + (lp[2] - lp[1]) * w, ce[1] + (ce[2] - ce[1]) * w
            else:
                l, c = lp[2] + math.log(top) * (x - 0.9) / 0.1, ce[2]
            w = max(0.0, min(1.0, (x - 0.2) / 0.7))
            if w < 0.55:
                g = w / 0.55; dc, air = st[0][0] + (st[1][0] - st[0][0]) * g, st[0][1] + (st[1][1] - st[0][1]) * g
            else:
                g = (w - 0.55) / 0.45; dc, air = st[1][0] + (st[2][0] - st[1][0]) * g, st[1][1] + (st[2][1] - st[1][1]) * g
            p = math.exp(l) * air * min(1.0, d / 0.2)
            self.level_to = self.level_db(x)
            if "open" in self.note_table:
                if self.plunger_cc is not None:
                    want = self.plunger_cc
                elif self.virtual:  # the hand opens with the note, wider the harder it was struck: a key alone says doo-wah; never quite shut, a soft note is soft already
                    want = (0.25 + 0.75 * (self.v / 127) ** 1.5) * min(1.0, t / 0.14)
                else:
                    want = 0.5
                self.opened += (want - self.opened) * 0.08
                self.h_x = -1.0
            if abs(x - self.h_x) > 0.004:
                self.h_x = x if "open" not in self.note_table else -1.0
                np.copyto(self.h, self.leaving(x))
            mc = self.modes_c
            tune_c = self.mode_here - (mc[0] + (mc[1] - mc[0]) * w / 0.55 if w < 0.55 else mc[1] + (mc[2] - mc[1]) * (w - 0.55) / 0.45)
            target_lip = self.note_table["f"] * (c + dc - 0.1) * 2 ** (-self.lip_trim / 12)
            # the attack or the slur
            if self.slur:
                g = min(1.0, t / 0.025)
                lip = self.lip_from + (target_lip - self.lip_from) * g
                lip *= 2 ** (-(SLACK_DOWN if self.down else SLACK_UP) * math.exp(-t / SLACK_T) / 1200)
                env = 1.0 - (DIP * (0.5 - 0.5 * math.cos(2 * math.pi * t / DIP_T))) if t < DIP_T else 1.0
                burst1 = 1.0
            else:
                rise = self.rise
                fr = self.rise_from
                env = self.first * (fr + (1 - fr) * math.sin(math.pi / 2 * t / rise) ** 2) if t < rise else self.first + (1 - self.first) * (1 - math.exp(-(t - rise) / 0.08))
                lip = target_lip * 2 ** (-self.scoop * math.exp(-t / 0.04) / 1200)
                burst1 = 1.0 + self.burst_add * math.exp(-t / 0.03)
            if self.releasing is not None:
                self.releasing += DT
                env *= max(0.0, 1.0 - self.releasing / self.rel_len)
            # vibrato
            semis = self.warp + 2.0 * self.pb - TUNE_FOLLOW * tune_c / 100
            if self.vib_knob > 0:
                depth, rate_cc = self.vib_knob, 64.0
            elif self.virtual:
                depth, rate_cc = self.shape.vibrato(t) / 127, self.shape.vibrato_rate(t)
            else:
                depth, rate_cc = 0.0, 64.0
            self.vib_phase += 2 * math.pi * (3.5 + 4.5 * rate_cc / 127) * DT
            wob = math.sin(self.vib_phase)
            lip *= 1 + 0.012 * depth * wob
            semis += 0.35 * depth * wob
            p *= 1 + 0.10 * depth * wob
            if self.virtual:
                semis += self.shape.bend_cents(t) / 100
            # growl
            gr = max(self.growl, self.at_growl)
            if gr > 0:
                b = self.band[i]
                p *= max(0.3, 1 + GROWL * gr * b)
                if b > 0.3:
                    burst1 += 4 * gr
            # flutter
            fl = max(self.flutter, self.at_flutter)
            if fl > 0:
                self.fl_phase += 2 * math.pi * 25 * (1 + 0.1 * self.jit[i]) * DT
                c1 = 0.5 + 0.5 * math.cos(self.fl_phase)
                shut = c1 * c1
                f2 = min(1.0, 1.6 * fl)
                p *= 1 - FLUTTER_DEPTH * f2 * shut
                burst1 += 6 * fl * shut
                semis -= 0.1 * f2
            p *= env * (1 + self.wp[i])
            lip *= 1 + self.wl[i]
            pm1, lip1 = p, lip
            ratio = 2 ** (semis / 12)
            self.tau += DT
            if self.releasing is not None and self.releasing > self.rel_len + 0.6:
                self.cur = None
        if LEVEL:  # eased: the air's and the key's changes must not click
            g0 = self.level_g
            self.level_now += (self.level_to - self.level_now) * 0.1
            self.level_g = 10 ** (self.level_now / 20)
        if self.m_count:
            s = self.sp * self.T * ratio
            E = np.exp(s)
            n = self.m_count
            self.E_r[:n], self.E_i[:n] = E.real, E.imag
            r = self.rT * ratio
            self.r_r[:n], self.r_i[:n] = r.real, r.imag
            self.lib.rt_process(self.state, CTRL, self.pm, pm1, self.lip * self.prev_ratio if hasattr(self, "prev_ratio") else self.lip, lip1 * ratio, self.burst, burst1, self.kick, self.mu, n,
                                ptr(self.E_r), ptr(self.E_i), ptr(self.r_r), ptr(self.r_i), 2, ptr(self.lr), ptr(self.lq), ptr(self.ls), ptr(self.lc),
                                spec.S["width"], spec.S["y0"], 1e-4, sim.RHO, float(SR), ptr(self.noise), len(self.noise), self.nl, ptr(self.dec), len(self.dec), ptr(self.out))
            self.kick = 0.0
            self.pm, self.lip, self.burst, self.prev_ratio = pm1, lip1, burst1, ratio
            if LEVEL:
                self.out *= np.linspace(g0, self.level_g, CTRL, endpoint=False)
            return self.out
        self.out[:] = 0
        return self.out

    def reset(self):
        self.state = self.lib.rt_new()
        if self.m_count:
            self.lib.rt_filters(self.state, ptr(self.h), len(self.h), ptr(self.note_table["r"]), len(self.note_table["r"]))

    def block(self, n):
        """n samples (a multiple of CTRL) of raw pressure, before the output gain."""
        return np.concatenate([self.step().copy() for _ in range(n // CTRL)])


class Section:
    """A row of Live trumpets behind one set of keys and controllers, in Auto or Solo. Each has its own seed, so its own
    noise, wobble and intonation, and its own seat. A trumpet with nothing sounding costs nothing."""

    def __init__(self, mute="open", virtual=True, seed=1, mode="auto", voices=VOICES):
        self.voices = [Live(mute, virtual, seed + 100 * i) for i in range(voices)]
        for v in self.voices[1:]:
            v.tables = self.voices[0].tables
        self.pans = []
        for p in SEATS[:voices]:  # the middle stays at 1 in both channels: the soloist's level as it was
            a = math.pi / 4 * (1 + p)
            self.pans.append((math.cos(a) * math.sqrt(2), math.sin(a) * math.sqrt(2)))
        self.mode = mode
        self.keys = {}  # a key down: (its trumpet, velocity, when)
        self.harmony = set()  # keys that stayed under a slur: nothing slurs away from them again
        self.taking = {}  # a key under a slur: (the silent trumpet starting on it, when)
        self.fade = [1.0] * voices
        self.last_on = -9.0
        self.now = 0.0
        self.events = collections.deque()
        self.map = dict(CONTROLS)
        self.learning = None
        for v in self.voices:
            v.map = self.map
        self.mix = np.zeros((CTRL, 2))
        self.trim = 1.0
        self.resets = 0
        self.log = []  # what the rule did with each key: out/live_keys.json, to tune it on real playing

    def post(self, *ev):
        self.events.append(ev)

    def load(self, mute):
        return self.voices[0].load(mute)

    def __getattr__(self, name):  # what the window reads: the first trumpet's
        return getattr(self.__dict__["voices"][0], name)

    def sounding(self):
        return sum(v.cur is not None for v in self.voices)

    def handle(self, ev):
        k = ev[0]
        if k == "on":
            self.note_on(ev[1], ev[2])
        elif k == "off":
            self.note_off(ev[1])
        elif k == "mode":
            self.quiet()
            self.mode = ev[1]
        elif k == "cc" and self.learning and ev[1] not in (120, 123):
            self.map = {n: c for n, c in self.map.items() if c != ev[1]}
            self.map[self.learning] = ev[1]
            self.learning = None
            for v in self.voices:
                v.map = self.map
        else:
            if k == "cc" and ev[1] in (120, 123):
                self.keys.clear()
            for v in self.voices:
                v.handle(ev)

    def quiet(self):
        self.keys.clear()
        self.harmony.clear()
        self.taking.clear()
        self.fade = [1.0] * len(self.voices)
        for v in self.voices:
            v.held.clear(); v.release()

    def free(self):
        return next((v for v in self.voices if not v.held), None)

    def steal(self):
        """Every trumpet busy: the oldest key's gives way (usually a chord on its way up)."""
        m = min(self.keys, key=lambda k: self.keys[k][2])
        v = self.keys[m][0]
        for k in [k for k, e in self.keys.items() if e[0] is v]:
            del self.keys[k]
        v.held.clear()
        return v

    def note_on(self, m, v):
        if not plays(m):
            return
        if self.mode == "solo":
            self.voices[0].note_on(m, v)
            self.log.append((self.now, "on", m, v, "solo"))
            return
        if m in self.keys:
            self.note_off(m)
        gap = self.now - self.last_on
        one = next(iter(self.keys)) if len(self.keys) == 1 else None
        span = GLIDE_SPAN if self.voices[0].glide or self.voices[0].glide_mode == "on" else SLUR_SPAN
        if one is not None and gap > CHORD_S and abs(m - one) <= span and one not in self.harmony:
            t, how = self.keys[one][0], "slur"
        else:
            t, how = self.free(), "chord" if self.keys and gap <= CHORD_S else ("new" if self.keys else "first")
            t = t or self.steal()
        self.log.append((self.now, "on", m, v, how, round(gap, 4), self.voices.index(t)))
        self.fade[self.voices.index(t)] = 1.0
        t.note_on(m, v)
        self.keys[m] = (t, v, self.now)
        self.last_on = self.now

    def note_off(self, m):
        self.log.append((self.now, "off", m))
        if self.mode == "solo":
            self.voices[0].note_off(m)
        elif m in self.keys:
            self.keys.pop(m)[0].note_off(m)
        self.harmony.discard(m)

    def under(self, m):
        """Is this key down but given up to a slur?"""
        t = self.keys[m][0] if m in self.keys else None
        return t is not None and len(t.held) > 1 and t.held[-1] != m

    def keep(self):
        """The weak spot of a slur: it may have been harmony. A key left down under one keeps its note."""
        for m, (w, _) in list(self.taking.items()):
            if not self.under(m):  # legato after all: nothing was heard of it
                del self.taking[m]
                w.held.clear(); w.release()
        for m, (t, v, _) in list(self.keys.items()):
            if not self.under(m):
                continue
            if m not in self.taking:
                w = self.free()
                if self.now - self.keys[t.held[-1]][2] > TAKE_S and w is not None:
                    self.fade[self.voices.index(w)] = 0.0
                    w.note_on(m, v)
                    self.taking[m] = (w, self.now)
            elif self.now - self.taking[m][1] > HIDE_S:
                w = self.taking.pop(m)[0]
                t.held.remove(m)
                self.keys[m] = (w, v, self.now)
                self.harmony.add(m)
                self.fade[self.voices.index(w)] = DT / RISE_S
                self.log.append((self.now, "on", m, v, "keep", 0.0, self.voices.index(w)))

    def step(self):
        """One control step: CTRL samples in two channels, each trumpet's output gain and seat applied."""
        started = False
        while self.events:  # a chord's trumpets start one per step: the setting up is the slow part
            if self.events[0][0] == "on" and started:
                break
            ev = self.events.popleft()
            started = started or ev[0] == "on"
            self.handle(ev)
        self.now += DT
        if self.mode == "auto" and (self.taking or len(self.keys) > 1):
            self.keep()
        self.mix[:] = 0
        # the row's level: down at once as trumpets join, back slowly; a chord still ends up louder than one trumpet
        n = sum(v.cur is not None and f > 0 and (v.releasing is None or v.releasing < v.rel_len) for v, f in zip(self.voices, self.fade))
        want = max(n, 1) ** -0.5
        trim0 = self.trim
        self.trim += (want - self.trim) * (0.1 if want < self.trim else 0.01)
        ramp = np.linspace(trim0, self.trim, CTRL, endpoint=False)
        for i, (v, (l, r)) in enumerate(zip(self.voices, self.pans)):
            if v.cur is None:
                v.now += DT
                continue
            x = v.step()
            if not math.isfinite(x.sum()):  # the model blew up: start the state again rather than the speakers
                v.reset()
                self.resets += 1
                continue
            f = self.fade[i]
            if f == 0.0:  # speaking unheard
                continue
            x = x * v.gain * ramp
            if f < 1.0:
                x = x * np.linspace(f, min(1.0, f + DT / RISE_S), CTRL, endpoint=False)
                self.fade[i] = min(1.0, f + DT / RISE_S)
            self.mix[:, 0] += x * l
            self.mix[:, 1] += x * r
        return self.mix


for _name in ("virtual", "growl", "flutter", "vib_knob", "pedal", "wheel", "aftertouch_to", "next_mute"):  # the window's: every trumpet's
    setattr(Section, _name, property(lambda s, n=_name: getattr(s.voices[0], n), lambda s, x, n=_name: [setattr(v, n, x) for v in s.voices]))


def render_events(events, dur, mute="open", virtual=True, seed=1):
    """Plays timed events ((t, kind, ...)) through the live player, as if live: for checking."""
    lv = Live(mute, virtual, seed)
    out = np.zeros(int(dur / DT) * CTRL)
    ev = sorted(events, key=lambda e: e[0])
    j = 0
    for k in range(len(out) // CTRL):
        t = k * DT
        while j < len(ev) and ev[j][0] <= t:
            lv.handle(ev[j][1:])
            j += 1
        out[k * CTRL:(k + 1) * CTRL] = lv.step()
    return out


def events_of(tr):
    ev = []
    for s, e, m, v in tr.notes:
        ev += [(s, "on", m, v), (e, "off", m)]
    for t, v in tr.pb:
        ev.append((t, "pb", v / 8192))
    for c in (11, 70, 21, 23):
        for t, v in tr.cc.get(c, []):
            ev.append((t, "cc", c, v))
    return ev
