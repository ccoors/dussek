/*
 * C interface of the Rust engine (engine/src/ffi.rs).
 *
 * pe_create/pe_destroy and the cache functions load and free: not real-time safe, call
 * them from a worker thread. Everything else is real-time safe and must be called from one thread
 * at a time (the audio thread). Event frames are relative to the start of the next
 * pe_process call; give events in time order.
 */
#ifndef PIANOENGINE_H
#define PIANOENGINE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PeEngine PeEngine;

/* gains linear, 1 = as fitted */
typedef struct PeParams {
    float room;
    float knock;
    float release;
    float pedal;       /* sustain-pedal noise: thumps and the strings' ring */
    float sympathetic;
    float volume;
    float hardness;       /* hammer felt stiffness factor, from the next strike */
    float hammer_wear;    /* live wear 0..1: hammers, voicing evenness, dampers, action */
    float voicing_wear;
    float damper_wear;
    float action_wear;
    float phantom;        /* phantom partials beyond the instrument's own, in the bass (live) */
    float grooves;        /* optional defects 0..1: hammer grooves, damper buzz, rattle */
    float damper_buzz;
    float rattle;
    uint32_t stops; /* registers played: bit 0 8′, bit 1 4′, bit 2 8′ II (instruments with registers) */
    float damper_fall; /* time a damper takes to fall onto the strings, x the normal: 1, 0 instant (live) */
    uint32_t prep;      /* the prepared piano: 0 none, 1 plucked, 2 muted, 3 harmonic, 4 bolt, 5 paper */
    float pluck_pos;    /* plucked: position along the string (fraction) */
    float pluck_bright; /* plucked: brightness, x a fingertip's */
    float mute;         /* muted: how hard the hand rests on the strings, 0..1 */
    uint32_t harmonic;  /* harmonic: the node touched (2 the octave, 3 the twelfth ...) */
    float paper;        /* paper: how much it buzzes, 0..1 */
    uint32_t buff;      /* the harpsichords' buff stop: 1 on (from the next plucks) */
    uint32_t lute;      /* Harpsichord B's lute stop: 1 on (from the next plucks) */
    uint32_t motor;     /* the vibraphone's motor: 1 on, 0 off (others ignore it) */
    float motor_rate;   /* its tremolo, pulses per second (0.1 ... 20) */
    float motor_depth;  /* its depth 0 ... 1 */
    uint32_t mallet;    /* the mallet's index (the instrument's own order: the marimba 0 yarn, 1 cord, 2 rubber; the xylophone 0 rosewood, 1 hard rubber; the bells 0 plastic, 1 brass; others ignore it) */
    float bebung;       /* the clavichord's Bebung: a held key's pitch rise (cents) at full pressure, 0 ... 200 (others ignore it) */
    float bend_range;   /* the pitch wheel's range, semitones 0 ... 12 (the đàn tranh, whose strings bend; others ignore it) */
    float input;        /* the audio input's level, linear 0 (off) ... 16: pe_process's input drives the strings */
    float dynamics;     /* a hammer piano's level range over the velocity, dB from velocity 1 to 127 (10 ... 70; others ignore it) */
    float variation;    /* each strike slightly different: 0 none, 1 as measured ... 4 (struck instruments) */
    float restrike;     /* a re-strike meets the strings or the bar where they ring: 0 off (as from rest), 1 on (hammer pianos, mallet instruments) */
} PeParams;

typedef struct PeMicSetup {
    double pos[2][3];     /* left, right: x, y, z (m) */
    double pattern[2];    /* 0 omni … 0.5 cardioid … 1 figure-8 */
    double yaw[2];        /* degrees; 0 along +y, positive towards +x */
    double pitch[2];      /* degrees above the horizon */
    double wall;          /* upright: the wall's y (m) */
} PeMicSetup;             /* 13 doubles = 104 bytes */

/* string tuning, compiled into the engine: changing it means a new engine */
typedef struct PeTuning {
    double a4_hz;        /* concert pitch (the instruments are measured re 440 Hz) */
    double unison_width; /* unison detuning re the measured one: 1 as measured, 0 pure */
    double unison_wear;  /* compiled wear 0..1: unisons, tuning, strings, soundboard */
    double tuning_wear;
    double string_wear;
    double board_wear;
    double falsebeat_wear;
    uint32_t wear_seed;  /* variant: seed of the wear's random draws */
    uint32_t mic_position; /* 0 recording, 1 player, 2 audience, 3 close 1 (grand: hammers,
                              upright: open top), 4 close 2 (grand: tail, upright: back), 5 free */
    double mic_spacing;    /* spacing of the microphone pair, m */
    double mic_distance;   /* m, 0: the position's default. Grand: from the soundboard's center
                              (close: the height above it); upright: from the board (close 1:
                              the height above the open top, close 2: behind the board) */
    uint32_t distance_level; /* level kept over the distance: 0 total, 1 direct sound */
    uint32_t temperament;     /* 0 equal … pe_temperament_count() − 1 */
    uint32_t temperament_key; /* the key it is centered on: 0 C … 11 B (A stays at a4_hz) */
    double stretch;           /* tuner's stretch on top of the recording's: 0 as recorded, 1 adds the
                                 stretch computed from the inharmonicity, -1 removes it (-1 … 10) */
    double bolt_mass;         /* the prepared piano's bolt: its mass 0..1 (0 none) */
    double bolt_pos;          /* and its position along the string (fraction) */
    PeMicSetup mic_setup;     /* free microphones, used when mic_position is 5 */
    double lid;               /* 0 closed … 1 open (grand: full stick; upright: top lid open), above 1 taken off; zero-initialized = closed lid, set 1.0 for the previous behavior */
    double pillow;            /* 0 none … 1 */
    uint32_t spread_keys;     /* bars: 1 all 88 keys spread along the frame, 0 its own range (the keys beyond at its ends) */
    double sustain;           /* the strings' ring re the fitted one: their internal loss divided by it, 0.25 … 8;
                                 0 (zero-initialized): 1, as fitted */
    double board_bass_db;     /* the board's bass on hammer pianos' strings, −12 … 24 dB; 0 (zero-initialized): as shipped */
    const char* scale_file;   /* a Scala .scl file (UTF-8 path; null or empty: none), played where temperament is
                                 pe_temperament_count() ("Scala file"), centered on temperament_key; a key map beside
                                 it of the same name (.kbm) lays it on the keys. Unreadable: equal temperament */
} PeTuning;

/* at most n threads per build (an engine, a morph, a felt's measurement) from now on, for the whole
 * process; 0: every core. Any thread */
void pe_set_build_threads(uint32_t n);

/* kept between pe_create calls (one thread): the last few loaded or morphed instruments and the
 * radiation, so an engine for another string tuning of the same instrument builds quickly */
typedef struct PeCache PeCache;
PeCache* pe_cache_new(void);
void pe_cache_free(PeCache* c);
/* quick builds: a felt edit's leveling (measured in seconds) taken as last measured for that felt, on
 * whatever morph point, instead of measured; pe_cache_provisional says whether the last build took it.
 * For a morph's pad dragged with a felt set: each point plays at once, the last one measured after */
void pe_cache_set_quick(PeCache* c, bool quick);
bool pe_cache_provisional(const PeCache* c);

/* the player's felt (engine/src/felt.rs, design 2026-10-22-felt-controls): per quantity up to 8 points
 * (keys strictly rising 21..108) of bends on the shipped felt, linear between them, held beyond:
 * curve[0] the stiffness (dB re the force at 0.5 mm), [1] the exponent (+), [2] the core's force
 * (N at 0.5 mm into it, +), [3] the core's depth (mm, +); hardness_db Hammer hardness on a hammer
 * piano (its live hardness then stays 1). A curve breaking the rules has no points. */
typedef struct PeFeltCurve {
    uint32_t n;
    uint8_t key[8];
    float bend[8];
} PeFeltCurve;
typedef struct PeFelt {
    PeFeltCurve curve[4];
    float hardness_db;
} PeFelt;
/* each key's felt, A0 first: the force at 0.5 mm (N) and exponent, the core's force at 0.5 mm into
 * it (N) and depth (mm; 1 where there is none), whether a hammer strikes the key, whether it has a core */
typedef struct PeFeltKeys {
    float f05[88];
    float p[88];
    float core05[88];
    float core_depth_mm[88];
    uint8_t hammer[88];
    uint8_t core[88];
} PeFeltKeys;
/* the "felt" state's text into out (the hardness 0); false for unreadable text (out: no bends). Any thread */
bool pe_felt_parse(const char* text, PeFelt* out);
/* its text in buf (len bytes, may be 0); returns its length (>= len: cut). Any thread */
size_t pe_felt_format(const PeFelt* f, char* buf, size_t len);
/* the shipped felt of the instrument between dirs at weights, as pe_create_morph builds it; not
 * real-time safe, any thread; false (out untouched): not loaded, a message in err */
bool pe_felt_shipped(const char* const* dirs, const float* weights, uint32_t count, PeFeltKeys* out,
    char* err, size_t err_len);
/* felt bent onto shipped: what the build plays (for drawing it). Any thread */
void pe_felt_apply(const PeFeltKeys* shipped, const PeFelt* felt, PeFeltKeys* out);

/* cache may be null */
PeEngine* pe_create(const char* instrument_dir, double sample_rate, const PeTuning* tuning, PeCache* cache,
    char* err, size_t err_len);
/* an instrument between the instruments in dirs at weights (count of each; the first is the main
 * instrument, a single one at weight 1 is that instrument); the cache keeps the last four by recipe;
 * felt (null or no bends at 0 dB: the shipped felt) on a hammer piano: each key's felt bent, its
 * level table and ff levels measured again (a few seconds), so every key keeps its loudness; the
 * cache keeps the last four edits. Other instruments ignore it. */
PeEngine* pe_create_morph(const char* const* dirs, const float* weights, uint32_t count, double sample_rate,
    const PeTuning* tuning, const PeFelt* felt, PeCache* cache, char* err, size_t err_len);
/* true when e plays a hammer piano (its felt and Hammer hardness go through pe_create_morph). Any thread */
bool pe_hammer_piano(const PeEngine* e);
void pe_destroy(PeEngine* e);

/* temperaments (PeTuning.temperament): count and names (static UTF-8, null out of range) */
uint32_t pe_temperament_count(void);
const char* pe_temperament_name(uint32_t index);
/* what the Scala file at path (UTF-8) holds, for the UI: its note count (> 0) with "description · N notes ·
 * key map NAME.kbm" in buf, or 0 with the error there (len bytes, may be 0). Any thread but the audio thread */
uint32_t pe_scale_info(const char* path, char* buf, size_t len);

typedef struct PeMicGeometry {
    uint32_t body;           /* 0 grand, 1 upright, 2 harp, 3 lever harp */
    uint32_t has_pair;       /* 0 at "Recording" */
    double mics[2][3];       /* left, right: x across (bass −), y along (keys −), z up from the board (m) */
    double axis[3];          /* distance = (pair center − origin) · axis */
    double origin[3];
    double distance;         /* effective (default and limits applied), m */
    double distance_min, distance_max;
    double spread[3];        /* unit vector from the left microphone to the right */
    double spacing;          /* effective, m */
    double spacing_max;
    uint32_t height_drag;    /* 1: the distance is a height (dragged in the front view) */
    uint32_t case_top_count;
    double case_top[24][2];  /* outline from above (x, y), closed */
    double keys_top[4];      /* x0, y0, x1, y1 */
    double case_front[4];    /* x0, z0, x1, z1 */
    double keys_front[4];    /* harp: NaN (none) */
    double lid_front[2][2];  /* grand: hinge, far edge (x, z); upright: the lid's corners (x0, z0), (x1, z1); harp: NaN */
    double floor_z;
    double wall_y;           /* upright: the wall behind (y); grand: NaN */
    double markers[5][2];    /* each position's pair center from above at its default distance; [0]: NaN */
    uint32_t front_axis;     /* the second view's horizontal axis: 0 x (from the front), 1 y (from the side: the harp) */
    uint32_t outline_front_count; /* 0: draw case_front; else the outline (a, z), polygons split by a NaN point */
    double outline_front[32][2];
    uint32_t custom;         /* 1: the free pair (position 5) */
    double pattern[2];
    double yaw[2];
    double pitch[2];
} PeMicGeometry;
/* stateless, any thread: the drawing and the pair where the engine puts it.
 * body 0 grand, 1 upright, 2 harp, 3 lever harp, 4 vibraphone, 5 marimba, 6 xylophone, 7 bells, 8 crotales, 9 guitar, 10 chimes, 11 hand chimes, 12 kalimba, 13 balafon, 14 psaltery, 15 đàn tranh, 16 strumstick; position as PeTuning.mic_position; spacing and distance as in
 * PeTuning. false: body or position out of range (out untouched) */
bool pe_mic_geometry(uint32_t body, uint32_t position, double spacing, double distance,
                     double lid, const PeMicSetup* custom, PeMicGeometry* out);   /* custom: used when position == 5, may be null; lid: 0 closed … 1 open, above 1 taken off */
/* clamps a free setup to what the instrument allows; true if anything changed */
bool pe_mic_clamp(uint32_t body, double lid, PeMicSetup* in_out);   /* lid: 0 closed … 1 open, above 1 taken off */
/* a body's name, as an instrument.json names it ("grand", "upright", …); NULL out of range */
const char* pe_body_name(uint32_t body);

/* the controls of PeInstrumentInfo.inert (engine/src/info.rs Control, in order) */
enum {
    PE_CONTROL_SYMPATHETIC, PE_CONTROL_PEDAL_NOISE, PE_CONTROL_KNOCK, PE_CONTROL_RELEASE, PE_CONTROL_HAMMER_HARDNESS,
    PE_CONTROL_HAMMER_GROOVES, PE_CONTROL_DAMPER_BUZZ, PE_CONTROL_RATTLE, PE_CONTROL_WEAR_HAMMERS, PE_CONTROL_WEAR_VOICING,
    PE_CONTROL_WEAR_DAMPERS, PE_CONTROL_WEAR_ACTION, PE_CONTROL_PHANTOM, PE_CONTROL_UNISON_WIDTH, PE_CONTROL_STRETCH,
    PE_CONTROL_WEAR_UNISONS, PE_CONTROL_WEAR_TUNING, PE_CONTROL_WEAR_STRINGS, PE_CONTROL_WEAR_SOUNDBOARD,
    PE_CONTROL_WEAR_FALSE_BEATS, PE_CONTROL_AUDIO_INPUT, PE_CONTROL_DYNAMICS, PE_CONTROL_SUSTAIN, PE_CONTROL_VARIATION,
    PE_CONTROL_BOARD_BASS, PE_CONTROL_RESTRIKE, PE_CONTROL_COUNT
};
/* what plucks or strikes the strings (PeInstrumentInfo.exciter) */
enum { PE_EXCITER_HAMMER, PE_EXCITER_QUILL, PE_EXCITER_FINGER, PE_EXCITER_PICK, PE_EXCITER_TANGENT, PE_EXCITER_MALLET,
    PE_EXCITER_THUMB };

#define PE_INFO_MALLETS 8
#define PE_INFO_LABELS 8
/* a control's label in the instrument's own words (the menu block's "labels") */
typedef struct PeLabel {
    char symbol[32];         /* the parameter's symbol */
    char name[64];
    char description[1024];  /* empty: the control's own */
} PeLabel;

/* What an instrument has (engine/src/info.rs): from instrument.json alone. Strings are
 * NUL-terminated, truncated on a UTF-8 character boundary; past 8 mallets or labels, the first 8. */
typedef struct PeInstrumentInfo {
    uint32_t body;          /* pe_mic_geometry's body */
    uint32_t exciter;       /* PE_EXCITER_* */
    uint32_t recorded_lo, recorded_hi;
    uint32_t registers;     /* 1 without registers */
    uint32_t stops;         /* the stops that play a register, as bits of PeParams' stops (0 with one register) */
    bool buff, lute, motor, bend, bebung;
    bool dampers;           /* some note has a damper (the damper fall, the pedal's latch) */
    bool bars;              /* bars on a frame */
    bool spread;            /* a range of its own (bars, a harp's or zither's bridge, a guitar's strings): Spread all keys acts */
    bool lid;               /* the body has a lid */
    bool soft_pedal;        /* the soft pedal acts (some note is struck) */
    uint32_t mallet_count;
    char mallets[PE_INFO_MALLETS][32]; /* the menu block's display names where it gives one per mallet, else the file's */
    uint32_t preparations;  /* bit k: Prep k (1 Plucked … 5 Paper) acts */
    uint32_t inert;         /* bit PE_CONTROL_*: the control has no effect */
    bool has_menu, hidden;  /* the menu block, and its "hidden" */
    bool reversed_keys;     /* the menu block's "reversed_keys": dark naturals, light sharps (the strip) */
    char name[64];
    char group[64];
    uint32_t label_count;
    PeLabel labels[PE_INFO_LABELS];
    uint32_t morph_family;  /* which instruments morph together (equal, nonzero); 0: this one does not */
    bool hammer_piano;      /* a hammer piano: the felt bends (pe_create_morph's felt) */
} PeInstrumentInfo;

/* fills `out` from instrument_dir/instrument.json (any thread, not real-time safe); false (out
 * untouched): missing or rejected, with a message in `err` (err_len bytes, may be null) */
bool pe_instrument_info(const char* instrument_dir, PeInstrumentInfo* out, char* err, size_t err_len);

/* the standalone's command line after its program name: "render" and midi2wav's arguments
 * (pianoengine render instruments/grand_a in.mid out.wav ...); renders as midi2wav does, printing to
 * stdout and stderr, and returns the exit status (2 for anything but "render") */
int32_t pe_cli(int32_t argc, const char* const* argv);
/* e (fresh, not yet processed) takes over from old, the engine it replaces: sounding
 * strings, keys, pedal and output go on (at the new tuning; a new microphone setup
 * crossfades). false (nothing taken): the engines do not match. old may be destroyed
 * afterwards. */
bool pe_adopt(PeEngine* e, PeEngine* old);
uint32_t pe_latency(const PeEngine* e);
void pe_set_params(PeEngine* e, const PeParams* p);
/* the velocity curve: incoming MIDI velocity -> the velocity played, through n points (2..8; the
 * ends at vin 1 and 127, vin strictly rising, vout not falling, every value 1..127; any other: the
 * straight line); a smooth curve that never falls (vin/vout: "in" is a Rust keyword) */
typedef struct PeVelocityCurve {
    uint32_t n;
    uint8_t vin[8];
    uint8_t vout[8];
} PeVelocityCurve;
/* set it (real-time safe); kept by pe_adopt */
void pe_set_velocity_curve(PeEngine* e, const PeVelocityCurve* c);
/* read its text ("in:out" pairs between commas or spaces; empty: straight) into out; false for
 * unreadable text (out: straight). Any thread */
bool pe_velocity_curve_parse(const char* text, PeVelocityCurve* out);
/* its text ("1:1,64:80,127:127") in buf (len bytes, may be 0); returns its length (>= len: cut) */
size_t pe_velocity_curve_format(const PeVelocityCurve* c, char* buf, size_t len);
/* the velocity played at velocity 1..127 (clamped; a real number: for drawing the curve) */
float pe_velocity_curve_eval(const PeVelocityCurve* c, float velocity);
/* midi2wav's options for these settings (--velocity-curve, --param, --tuning; only what differs
 * from the defaults; curve null: straight), a C string in buf (len bytes, may be 0); returns its
 * length without the terminator (>= len: cut). Stateless, any thread */
size_t pe_format_spec(const PeParams* p, const PeTuning* t, const PeVelocityCurve* curve, char* buf, size_t len);
void pe_note_on(PeEngine* e, uint32_t frame, uint8_t key, uint8_t velocity);
void pe_note_off(PeEngine* e, uint32_t frame, uint8_t key);
/* with the MIDI release velocity (1..127, 64 normal, 0 unknown): the damper's fall and the release thump */
void pe_note_off_velocity(PeEngine* e, uint32_t frame, uint8_t key, uint8_t velocity);
void pe_pedal(PeEngine* e, uint32_t frame, bool down);
/* continuous pedal position 0 (up) … 1 (down), e.g. CC64 / 127: half-pedalling */
void pe_pedal_depth(PeEngine* e, uint32_t frame, float depth);
void pe_sostenuto(PeEngine* e, uint32_t frame, bool down);
/* soft pedal 0 … 1, e.g. CC67 / 127: una corda (grand) or the hammer rail (upright), for the next notes */
void pe_soft_pedal(PeEngine* e, uint32_t frame, float depth);
/* the pitch wheel, −1 … 1 (14 bits re the middle / 8192): the sounding notes bent by ± bend_range
 * (the đàn tranh; others ignore it) */
void pe_pitch_bend(PeEngine* e, uint32_t frame, float value);
/* the clavichord's Bebung (others ignore them), 0 … 1: the mod wheel (CC1 / 127) and the channel
 * pressure on every held key, the key pressure (polyphonic aftertouch) on its key; the highest counts */
void pe_mod_wheel(PeEngine* e, uint32_t frame, float value);
void pe_channel_pressure(PeEngine* e, uint32_t frame, float value);
void pe_key_pressure(PeEngine* e, uint32_t frame, uint8_t key, float value);
void pe_all_keys_up(PeEngine* e);
/* panic: every key and pedal released, the output faded to nothing over 20 ms, then every
 * string, the sympathetic ring and the radiation's and room's tails cleared */
void pe_silence(PeEngine* e);
/* input: mono, frames floats, exciting the strings at PeParams.input (null: none); it may not
 * alias left or right */
void pe_process(PeEngine* e, const float* input, float* left, float* right, uint32_t frames);

/* ---- diagnostics (engine/src/telemetry.rs) ---- */
#define PE_TELEMETRY_VOICES 24
#define PE_TELEMETRY_PARTIALS 48
#define PE_KEY_WORDS 8
#define PE_VIZ_PARTIALS 32
#define PE_KEY_DAMPER 1u  /* PeTelemetryKey.flags: the strings have a damper */
#define PE_KEY_HAMMER 2u  /* struck by a hammer (else plucked, or a bar) */

typedef struct PeTelemetryPartial {
    float freq;    /* Hz */
    float decay;   /* 1/s (σ) */
    float amp;     /* model units × output calibration */
    uint32_t kind; /* bit 0: horizontal polarization, bit 1: false-beat copy */
} PeTelemetryPartial;
typedef struct PeTelemetryVoice {
    uint8_t key;   /* 0 = A0 … 87 = C8 */
    uint8_t reg;   /* register (0 the main one) */
    uint16_t count;
    float speed;   /* hammer speed of the last strike, m/s */
    float level;
    uint32_t modes;
    PeTelemetryPartial partials[PE_TELEMETRY_PARTIALS]; /* strongest first */
} PeTelemetryVoice;
/* one key for the Visualization panel: its first register's strings (or bar) */
typedef struct PeTelemetryKey {
    uint64_t struck_at; /* engine position of the last strike or pluck (0: none) */
    float speed;        /* its hammer speed, m/s */
    float engage;       /* the damper, 0 lifted … 1 resting */
    float length;       /* speaking length, m */
    float point;        /* strike or pluck point re the length */
    uint32_t flags;     /* PE_KEY_* */
    float amp[PE_VIZ_PARTIALS];  /* the strings' displacement per partial, m */
    float ring[PE_VIZ_PARTIALS]; /* the sympathetic ring per partial, the same scale */
} PeTelemetryKey;
typedef struct PeTelemetryFrame {
    uint64_t counter;  /* frames built since the engine attached the region: restarts at every
                          attach, for debugging only (nothing may rely on it) */
    uint64_t position; /* engine sample position */
    float rate;
    float pedal;
    uint32_t voices;
    uint32_t keys[PE_KEY_WORDS]; /* key i: word i/12, bits 2·(i%12): held, ringing */
    PeTelemetryVoice voice[PE_TELEMETRY_VOICES]; /* loudest first */
    PeTelemetryKey key[88];
} PeTelemetryFrame;

typedef struct PeDiag {
    uint32_t keys[PE_KEY_WORDS];
    uint32_t voices;
    float pedal;
    float sostenuto; /* 1 while the sostenuto pedal is down */
    float soft;      /* the soft pedal's depth 0 … 1 */
} PeDiag;

size_t pe_telemetry_region_size(void);
/* the region's creator, once, off the audio thread: zero and mark the mapped bytes (8-aligned) */
bool pe_telemetry_init(void* base, size_t size);
/* real-time safe; base NULL detaches. The region must outlive the attachment. */
bool pe_telemetry_attach(PeEngine* e, void* base, size_t size, double rate);
/* real-time safe (audio thread) */
void pe_diag(const PeEngine* e, PeDiag* out);
/* reader side (UI) */
bool pe_telemetry_set_enabled(void* base, size_t size, bool on);
/* the newest frame not yet taken. false: none, or out NULL / not 8-byte aligned (nothing written) */
bool pe_telemetry_read_frame(void* base, size_t size, PeTelemetryFrame* out);
/* interleaved stereo frames after *cursor into out (2 * max_frames floats); returns the frames
 * written. cursor must be 8-byte aligned (start it at pe_telemetry_audio_head); NULL or
 * misaligned cursor/out: 0 */
uint32_t pe_telemetry_read_audio(void* base, size_t size, uint64_t* cursor, float* out, uint32_t max_frames);
/* the writer's published audio position: a reader starting its cursor here gets only newer audio */
uint64_t pe_telemetry_audio_head(void* base, size_t size);
/* reader: the incoming note-on velocities (before the velocity curve; written whether or not the
 * telemetry is enabled) after *cursor, oldest first, into out (<= max); returns how many. Behind by
 * more than the ring (64 slots): its newest 63. Start the cursor at pe_telemetry_notes_head; 0
 * for an invalid region, a NULL or misaligned cursor, a NULL out */
uint64_t pe_telemetry_notes_head(void* base, size_t size);
uint32_t pe_telemetry_read_notes(void* base, size_t size, uint64_t* cursor, uint8_t* out, uint32_t max);
/* the value mirror: the plugin's DSP side writes the parameter values MIDI set where the host takes
 * no change request, the UI passes them on to the host. Each slot holds a sequence (0: never set,
 * one up per write) and the newest value; written whether or not the telemetry is enabled. false
 * for an invalid region, index >= PE_TELEMETRY_VALUES or a NULL output */
#define PE_TELEMETRY_VALUES 128
bool pe_telemetry_set_value(void* base, size_t size, uint32_t index, float value);
bool pe_telemetry_read_value(void* base, size_t size, uint32_t index, uint32_t* seq, float* value);
/* frames the engine skipped or dropped because the ring was full (the reader was behind) */
uint32_t pe_telemetry_dropped(void* base, size_t size);
/* owner pid protocol: the creator records its pid after pe_telemetry_init; a later instance sweeps
 * regions whose recorded owner is dead. owner: 0 = none or not a valid region */
bool pe_telemetry_set_owner(void* base, size_t size, uint32_t pid);
uint32_t pe_telemetry_owner(void* base, size_t size);

#ifdef __cplusplus
}
#endif

#endif
