#include "Params.hpp"
#include "pianoengine.h"
#include <cstring>

namespace pe {

#define PE_WEAR "Shifts the overall Condition for this aspect (-100 % newer, +100 % older): "
const WearAspect kWearAspects[9] = {
    { kWearHammers, "Wear: hammers", "wear_hammers", false, PE_WEAR "the felt compressed and harder: brighter, harsher strikes." },
    { kWearVoicing, "Wear: voicing", "wear_voicing", false,
        PE_WEAR "hammers worn unevenly: neighboring notes differ in loudness and color." },
    { kWearAction, "Wear: action noise", "wear_action", false, PE_WEAR "a noisier action: louder knocks and releases." },
    { kWearDampers, "Wear: dampers", "wear_dampers", false, PE_WEAR "worn damper felt: notes ring on after the key is let go." },
    { kWearUnisons, "Wear: unisons", "wear_unisons", true,
        PE_WEAR "the strings of a note drift apart: stronger beating. Rebuilds the instrument." },
    { kWearTuning, "Wear: tuning", "wear_tuning", true, PE_WEAR "the notes drift off pitch. Rebuilds the instrument." },
    { kWearFalseBeats, "Wear: false beats", "wear_false_beats", true,
        PE_WEAR "single strings beat by themselves, as uneven old strings do. Rebuilds the instrument." },
    { kWearStrings, "Wear: strings", "wear_strings", true,
        PE_WEAR "old strings: overtones slightly off and dying faster. Rebuilds the instrument." },
    { kWearSoundboard, "Wear: soundboard", "wear_soundboard", true,
        PE_WEAR "a tired soundboard: notes die away faster. Rebuilds the instrument." },
};
#undef PE_WEAR

const WearAspect* wearAspect(uint32_t index)
{
    for (const WearAspect& w : kWearAspects)
        if (w.param == index)
            return &w;
    return nullptr;
}

const Param kCompiledWear[5] = { kWearUnisons, kWearTuning, kWearStrings, kWearSoundboard, kWearFalseBeats };
namespace {

Group groupOf(uint32_t i)
{
    return i <= kMorphY ? kGroupInstrument
        : i <= kRoom ? kGroupMics
        : i <= kUnisonWidth ? kGroupTuning
        : i <= kDamperFall ? kGroupNoises
        : i <= kWearSoundboard ? kGroupCondition
        : i <= kPhantom ? kGroupDefects
                       : kGroupPrepared;
}

ParamInfo make(uint32_t i)
{
    // defaults: automatable, no unit
    ParamInfo p { nullptr, nullptr, nullptr, nullptr, 0, 0, 1, true, false, false, groupOf(i) };
    if (const WearAspect* w = wearAspect(i)) {
        p.name = w->name; p.symbol = w->symbol; p.unit = "%";
        p.def = 0; p.min = -100; p.max = 100;
        p.automatable = !w->compiled;
        p.description = w->description;
        return p;
    }
    auto set = [&](const char* name, const char* symbol, const char* unit, float def, float min, float max) {
        p.name = name; p.symbol = symbol; p.unit = unit; p.def = def; p.min = min; p.max = max;
    };
    switch (i) {
    case kVolume: set("Volume", "volume", "dB", 0, kOffDb, 24);
        p.description = "The output level, after everything else. The left end is off.";
        break;
    case kRoom: set("Room", "room", "dB", 0, kOffDb, 12);
        p.description = "The recording room's reverberation against the instrument's own sound. The left end is off: "
                        "the dry instrument, for an external reverb.";
        break;
    case kKnock: set("Action knock", "knock", "dB", 0, kOffDb, 12);
        p.description = "The mechanism's noise at each note (a piano's key and hammer thump, a jack's knock), against "
                        "the recording. The left end is off.";
        break;
    case kRelease: set("Release noise", "release", "dB", 0, kOffDb, 24);
        p.description = "The noise when a key is let go (a damper landing, a jack falling back), against the recording. "
                        "The left end is off.";
        break;
    case kStop8:
    case kStop4:
        set(i == kStop8 ? "Stop 8'" : "Stop 4'", i == kStop8 ? "stop_8" : "stop_4", nullptr, i == kStop8 ? 1 : 0, 0, 1);
        p.description = "Register on or off, for the next notes (instruments with registers: Harpsichords B, E and F)";
        p.boolean = p.integer = true;
        break;
    case kStop8b:
        set("8' II", "stop_8b", nullptr, 0, 0, 1);
        p.description = "The second 8' choir, for the next notes (Harpsichord B, derived; Harpsichords E and F, recorded); off "
                        "while the lute plucks its strings";
        p.boolean = p.integer = true;
        break;
    case kLute:
        set("Lute", "lute", nullptr, 0, 0, 1);
        p.description = "The lute row: the 8' II strings plucked near the nut, nasal, for the next notes (Harpsichord B)";
        p.boolean = p.integer = true;
        break;
    case kBuff:
        set("Buff", "buff", nullptr, 0, 0, 1);
        p.description = "Pads on the strings for the next notes: shorter and darker (the harpsichords)";
        p.boolean = p.integer = true;
        break;
    case kMotor:
        set("Motor", "motor", nullptr, 1, 0, 1);
        p.description = "The vibraphone's motor: discs in the resonators turn and the fundamentals swell and fade "
                        "together (the vibraphone)";
        p.boolean = p.integer = true;
        break;
    case kMotorSpeed:
        set("Motor speed", "motor_speed", "Hz", 7, 1, 12);
        p.description = "Pulses per second of the motor's tremolo (the vibraphone)";
        break;
    case kMotorDepth:
        set("Motor depth", "motor_depth", "%", 70, 0, 100);
        p.description = "How far the fundamentals fade while the discs close the resonators: 100 % is about 12 dB "
                        "(the vibraphone)";
        break;
    case kMallet:
        set("Mallet", "mallet", nullptr, 0, 0, kMalletCount - 1); p.integer = true;
        p.description = "The mallet, from the next strike: the instrument's own (a marimba's yarn, cord or rubber, a "
                        "xylophone's rosewood or hard rubber …)";
        break;
    case kMorphX:
        set("Morph X", "morph_x", nullptr, 0, -1, 1);
        p.description = "Across the Morph tab's pad: with Morph Y, the point whose distance to each corner sets "
                        "that instrument's share of the morph.";
        break;
    case kMorphY:
        set("Morph Y", "morph_y", nullptr, 1, -1, 1);
        p.description = "Up the Morph tab's pad: with Morph X, the point whose distance to each corner sets that "
                        "instrument's share of the morph. The top corner is the instrument chosen.";
        break;
    case kBendRange:
        set("Bend range", "bend_range", "st", 2, 0, 12);
        p.description = "How far the pitch wheel bends the sounding notes, up or down: the left hand pressing the string "
                        "behind its bridge. The dan tranh";
        break;
    case kBebung:
        set("Bebung", "bebung", "ct", 30, 0, 100);
        p.description = "The clavichord's vibrato: a held key's pitch rises with the pressure on its tangent, by this much "
                        "at full pressure. Played with the mod wheel, or with aftertouch (channel or per key; the highest "
                        "counts). The clavichord";
        break;
    case kSustainLatch:
        set("Sustain latch", "sustain_latch", nullptr, 0, 0, 1);
        p.description = "The sustain pedal held down while on (with MIDI CC64 and the on-screen pedal: the deepest "
                        "counts): for MIDI files without pedal on the vibraphone and the harps, whose notes otherwise "
                        "stop at the key's release";
        p.boolean = p.integer = true;
        break;
    case kSympathetic: set("Sympathetic resonance", "sympathetic", "%", 100, 0, 300);
        p.description = "How strongly the undamped strings ring along with the played ones: 100 % as measured, 0 off, "
                        "more exaggerates it. With the pedal down every string joins in.";
        break;
    case kAudioInput: set("Audio input", "audio_input", "dB", kOffDb, kOffDb, 12);
        p.description = "Sound from the plugin's audio input reaches the strings: with the pedal down they ring along "
                        "with it, as when singing into a grand; with it up only the strings without dampers do. The input "
                        "itself is not played back. 0 dB drives the strings about as hard as their own sound at the same "
                        "level. The left end is off. Beware of acoustic feedback with a microphone near the speakers.";
        break;
    case kPrepMode: set("Preparation", "preparation", nullptr, 0, 0, 5); p.automatable = false; p.integer = true;
        p.description = "How the strings are prepared, the whole piano at once: plucked with a finger inside the piano, "
                        "muted by a hand or felt, played as harmonics, with a bolt on the strings, or with buzzing paper.";
        break;
    case kPluckPosition: set("Pluck position", "pluck_position", "%", 20, 5, 50);
        p.description = "Where along the string it is plucked: near the end it is bright and thin, in the middle hollow.";
        break;
    case kPluckBrightness: set("Pluck brightness", "pluck_brightness", "%", 100, 25, 400);
        p.description = "How hard the finger or nail is: 100 % is a fingertip, more is a nail or a pick.";
        break;
    case kMuteStrength: set("Mute", "mute", "%", 60, 0, 100);
        p.description = "How hard the hand rests on the strings while a key is down: 100 % is a dead thud. "
                        "The highest notes have no dampers to rest on and are not muted.";
        break;
    case kHarmonicNumber: set("Harmonic", "harmonic", nullptr, 2, 2, 8); p.integer = true;
        p.description = "The node a finger touches while the string is struck: 2 sounds the octave, 3 the twelfth, "
                        "4 the double octave, and so on.";
        break;
    case kBoltMass: set("Bolt mass", "bolt_mass", "%", 50, 0, 100); p.automatable = false;
        p.description = "A bolt or screw on every string: it bends the pitch and makes the overtones inharmonic.";
        break;
    case kBoltPosition: set("Bolt position", "bolt_position", "%", 31, 5, 50); p.automatable = false;
        p.description = "Where along the strings the bolt sits: the partials with a node there are not affected.";
        break;
    case kPaperBuzz: set("Paper buzz", "paper_buzz", "%", 50, 0, 100);
        p.description = "Paper or a loose part on the strings: how much it buzzes with every note.";
        break;
    case kDamperFall: set("Damper fall", "damper_fall", "%", 100, 0, 400);
        p.description = "How long a damper takes to land on the strings when a key is released, 100 % = about 25 ms "
                        "at a normal release (faster for a fast release, slower for a slow one when the keyboard "
                        "sends the release velocity). 0 = instant. A quick re-strike catches a damper in mid-fall.";
        break;
    case kSustain: set("Sustain", "sustain", "%", 100, 50, 400); p.automatable = false;
        p.description = "How long the notes ring: 100 % as recorded, 200 % twice as long, 50 % half. Every partial "
                        "decays that much slower or faster, its pitch and the attack kept. Not the sustain pedal. "
                        "Rebuilds the instrument.";
        break;
    case kBoardBass: set("Bass", "board_bass", "dB", 0, -12, 24);
        p.description = "The strings' bass, as an equalizer on them: more or less of their sound below about 180 Hz, "
                        "louder with it; the hammer's thud through the soundboard stays as it is. 0 is the instrument's "
                        "own. Rebuilds the instrument.";
        break;
    case kVariation: set("Strike variation", "variation", "%", 0, 0, 400);
        p.description = "How much each strike differs from the last, as a real hammer never meets the strings quite "
                        "the same way twice: the strike point, the moment and the felt per string. 100 % as measured "
                        "on recorded takes, 0 every strike alike. From the next note.";
        break;
    case kRestrike:
        set("Live re-strike", "restrike", nullptr, 0, 0, 1);
        p.description = "A note struck again while it still rings: the hammer meets the strings, the mallet the bar, "
                        "where they are in their swing, so each repeat comes out a little different, harder where they "
                        "swing towards it. Off: every strike as into still strings or a still bar. From the next note.";
        p.boolean = p.integer = true;
        break;
    case kDynamics: set("Dynamics", "dynamics", "dB", 45, 10, 70);
        p.description = "How much softer the softest key is than the hardest (the range from velocity 1 to 127): the "
                        "loudest notes stay as recorded, less brings the soft ones up (struck harder, so also brighter), "
                        "more takes them down. 45 dB as a recorded piano's sample set plays its layers. Pianos only.";
        break;
    case kHammerHardness: set("Hammer hardness", "hammer_hardness", "dB", 0, -24, 12);
        p.description = "How hard what strikes or plucks the string is (the hammer's felt, a mallet, a quill, a finger), "
                        "from the next note: up brighter, down softer and rounder. 0 dB as recorded. On the pianos "
                        "every key keeps its loudness: the felt is measured again in the background (a few seconds).";
        break;
    case kConcertPitch: set("Concert pitch", "concert_pitch", "Hz", 440, 415, 466); p.automatable = false;
        p.description = "The pitch of A4. The strings are retuned: the instrument is rebuilt in the background and the "
                        "sounding notes glide to the new tuning.";
        break;
    case kStretch: set("Stretch", "stretch", nullptr, 0, -1, 10); p.automatable = false;
        p.description = "Tuner's stretch on top of the recording's own tuning, computed from the strings' "
                        "inharmonicity: bass flatter, treble sharper. 0 = as recorded, 1 adds a full tuner's "
                        "stretch, -1 takes one off (pure octaves), more exaggerates it.";
        break;
    case kUnisonWidth: set("Unison width", "unison_width", "%", 100, 0, 400); p.automatable = false;
        p.description = "How far the strings of one note are detuned against each other: 100 % as recorded, 0 all alike "
                        "(a steady tone), more a wider beating. Rebuilds the instrument.";
        break;
    case kCondition: set("Condition", "condition", "%", 10, 0, 100); p.integer = true; // the number, its zone and the wear agree
        p.description = "The instrument's condition, in zones: Mint (0 %, as exported), Normal (to 10 %, the default: "
                        "the keys a little uneven in loudness), Worn (to 35 %), Broken (to 100 %): harder hammers, worn "
                        "dampers, a noisier action, drifting tuning and unisons, old strings and soundboard. The "
                        "Condition tab shifts each aspect from here.";
        break;
    case kHammerGrooves: set("Hammer grooves", "hammer_grooves", "%", 0, 0, 100);
        p.description = "Grooves the strings have worn into the hammer felt: a short, gritty noise at each strike, "
                        "stronger when playing harder. 100 % is the most it should be.";
        break;
    case kDamperBuzz: set("Damper buzz", "damper_buzz", "%", 0, 0, 100);
        p.description = "A damper that buzzes for a moment as it lands on the strings. 100 % is the most it should be.";
        break;
    case kRattle: set("Rattle", "rattle", "%", 0, 0, 100);
        p.description = "A loose part on the soundboard that rattles on loud low notes. 100 % is the most it should be.";
        break;
    case kPhantom: set("Phantom partials", "phantom", "%", 0, 0, 100);
        p.description = "More of the extra partials old, stiff strings make in loud bass notes (in full to C2, "
                        "fading out by C3), beyond the instrument's own: a metallic, growling edge. 100 % is the "
                        "most it should be.";
        break;
    case kMicPosition: set("Mic position", "mic_position", nullptr, 1, 0, 5); p.automatable = false; p.integer = true;
        p.description = "Where the two microphones stand: Recording (the recording's own sound), Player (at the "
                        "keyboard, the default), Audience (in front, at a distance), Close 1 and Close 2 (near the instrument, where "
                        "the names say) or Custom (both placed freely). Click a marker in the drawing to choose one.";
        break;
    case kMicLX: set("Left mic x", "mic_l_x", "m", -0.25f, -20, 20); p.automatable = false;
        p.description = "x across the piano (bass -, treble +), y along it (keyboard -, tail +), z up from the soundboard. "
                        "Dragging a microphone in the drawing sets these.";
        break;
    case kMicLY: set("Left mic y", "mic_l_y", "m", -1.25f, -20, 20); p.automatable = false;
        p.description = "Left microphone, y: along the piano (keyboard -, tail +).";
        break;
    case kMicLZ: set("Left mic z", "mic_l_z", "m", 0.45f, -0.7f, 3); p.automatable = false;
        p.description = "Left microphone, z: the height above the soundboard.";
        break;
    case kMicRX: set("Right mic x", "mic_r_x", "m", 0.25f, -20, 20); p.automatable = false;
        p.description = "Right microphone, x: across the piano (bass -, treble +). Dragging a microphone in the drawing "
                        "sets x, y and z.";
        break;
    case kMicRY: set("Right mic y", "mic_r_y", "m", -1.25f, -20, 20); p.automatable = false;
        p.description = "Right microphone, y: along the piano (keyboard -, tail +).";
        break;
    case kMicRZ: set("Right mic z", "mic_r_z", "m", 0.45f, -0.7f, 3); p.automatable = false;
        p.description = "Right microphone, z: the height above the soundboard.";
        break;
    case kMicWall: set("Wall distance", "mic_wall", "m", 0.15f, 0.15f, 2); p.automatable = false;
        p.description = "Upright only: how far behind the board the wall stands.";
        break;
    case kMicLPattern: set("Left mic pattern", "mic_l_pattern", "%", 0, 0, 100); p.automatable = false;
        p.description = "0 omni, 50 cardioid, 75 hypercardioid, 100 figure-8. A pattern hears what it points at; "
                        "omni ignores the aim.";
        break;
    case kMicLYaw: set("Left mic aim", "mic_l_yaw", "°", 0, -180, 180); p.automatable = false;
        p.description = "Direction in the floor plane: 0 into the piano from the player's seat, positive towards the treble side.";
        break;
    case kMicLPitch: set("Left mic tilt", "mic_l_pitch", "°", 0, -90, 90); p.automatable = false;
        p.description = "Up or down from level, in degrees (positive: up). Omni ignores the aim.";
        break;
    case kMicRPattern: set("Right mic pattern", "mic_r_pattern", "%", 0, 0, 100); p.automatable = false;
        p.description = "As for the left microphone.";
        break;
    case kMicRYaw: set("Right mic aim", "mic_r_yaw", "°", 0, -180, 180); p.automatable = false;
        p.description = "As for the left microphone.";
        break;
    case kMicRPitch: set("Right mic tilt", "mic_r_pitch", "°", 0, -90, 90); p.automatable = false;
        p.description = "As for the left microphone.";
        break;
    case kLid: set("Lid", "lid", "%", 100, 0, 100); p.automatable = false;
        p.description = "How far the lid is open: 100 % is the full stick (grand) or the open top lid (upright), "
                        "about 50 % a half stick, 0 % closed, which muffles the sound outside the case. "
                        "Needs a microphone position (the recording stands for the open lid).";
        break;
    case kLidOff:
        set("Lid removed", "lid_off", nullptr, 0, 0, 1);
        p.boolean = p.integer = true;
        p.automatable = false;
        p.description = "The lid taken off: nothing reflects the sound above the strings and nothing muffles it, "
                        "as for a studio's microphones or an even sound in a small room; the Lid setting does "
                        "nothing then. Needs a microphone position (the recording stands for the open lid).";
        break;
    case kPillow: set("Pillow", "pillow", "%", 0, 0, 100); p.automatable = false;
        p.description = "A pillow or blanket against the soundboard: the body sound gets duller, quieter and shorter.";
        break;
    case kMicSpacing: set("Mic spacing", "mic_spacing", "cm", 50, 0, 1000); p.automatable = false;
        p.description = "How far apart the two microphones stand, across the line of sight: 0 is mono, more widens the "
                        "stereo image. Dragging the pair in the drawing sets it.";
        break;
    case kTemperament: set("Temperament", "temperament", nullptr, 0, 0, 9); p.automatable = false; p.integer = true;
        p.description = "Equal (every key alike) or a historical temperament: keys near the temperament key sound purer, "
                        "far ones more colorful. As recorded: each recorded key at its own pitch, as the instrument was "
                        "tuned in the recordings. Scala file: a scale from a .scl file, any number of notes (a .kbm "
                        "key map of the same name beside it lays it on the keys). A stays at the concert pitch.";
        break;
    case kTemperamentKey: set("Temperament key", "temperament_key", nullptr, 0, 0, 11); p.automatable = false; p.integer = true;
        p.description = "The key a historical temperament or a Scala file's scale is centered on (its \"C\"). No effect "
                        "with Equal temperament, As recorded or a Scala file with its own key map.";
        break;
    case kSpreadKeys:
        set("Spread all keys", "spread_keys", nullptr, 1, 0, 1);
        p.boolean = p.integer = true;
        p.automatable = false;
        p.description = "Bars, harps, guitars and zithers: all 88 keys spread along the instrument in front of the "
                        "microphones (on, the default), or only its own range, with the keys beyond it sounding from its "
                        "ends (off, as the real instrument)";
        break;
    case kDistanceLevel: set("Distance level", "distance_level", nullptr, 0, 0, 1); p.automatable = false; p.integer = true;
        p.description = "Total: the level stays the same as you move away (far positions change little, the room "
                        "dominates). Direct sound: the direct sound stays the same and the room grows with the "
                        "distance; for an external reverb, set Room to Off.";
        break;
    case kMicDistance: set("Mic distance", "mic_distance", "m", 0, 0, 20); p.automatable = false;
        p.description = "0 = the position's default. Far positions (Audience) are mostly room: the distance "
                        "changes little there.";
        break;
    case kWearVariant: set("Wear: variant", "wear_variant", nullptr, 1, 1, 99); p.automatable = false; p.integer = true;
        p.description = "Another instrument of the same age: each variant draws other random deviations (which notes are "
                        "out of tune, which hammers are worn). Rebuilds the instrument.";
        break;
    case kPedalNoise: set("Pedal noise", "pedal_noise", "dB", -20, kOffDb, 12);
        p.description = "The sustain pedal's thump going down and up. -20 dB by default, 0 dB as recorded; the left "
                        "end is off.";
        break;
    case kPedalRing: set("Pedal ring", "pedal_ring", "dB", -20, kOffDb, 12);
        p.description = "The strings set ringing as the sustain pedal lifts their dampers. -20 dB by default, 0 dB as "
                        "fitted; the left end is off.";
        break;
    }
    return p;
}

struct Table {
    ParamInfo p[kParamCount];
    Table() { for (uint32_t i = 0; i < kParamCount; ++i) p[i] = make(i); }
};

const char* const kMicNames[6] = { "Recording", "Player", "Audience", "Close 1: hammers / open top / soundboard / bars",
    "Close 2: tail / back / sound holes / resonators", "Custom" };
const char* const kKeys[12] = { "C", "C#", "D", "Eb", "E", "F", "F#", "G", "G#", "A", "Bb", "B" };
const char* const kLevelNames[2] = { "Total (direct + room)", "Direct sound" };
const char* const kPrepNames[6] = { "None", "Plucked", "Muted", "Harmonic", "Bolt", "Paper" };
// the Mallet parameter's values for a host (the UI shows the instrument's own names)
const char* const kMalletNumbers[kMalletCount] = { "1", "2", "3", "4", "5", "6", "7", "8" };

/// the Instrument menu's row (kInstrumentRow): no parameter
const ParamInfo kInstrumentInfo { "Instrument", "instrument", nullptr,
    "The instrument, by group: every instrument found in the search paths ($PIANOENGINE_INSTRUMENTS, the "
    "bundle's own, ~/.local/share/pianoengine/instruments, then .../snapshots for git snapshots), each modeled "
    "from its own recordings. "
    "Switching loads it (a moment).",
    0, 0, 0, false, true, false, kGroupInstrument };

/// the Preset rows (kMicPresetRow, kPresetRow): no parameters
const ParamInfo kMicPresetInfo { "Mic preset", "mic_preset", nullptr,
    "Named microphone setups for this kind of instrument: the shipped ones, then your own. Picking one sets the "
    "microphones only (position, distance, spacing, the Custom pair, Distance level); the lid, the room and the rest "
    "stay. Save as… keeps the current microphones under a name, for every instrument of this kind.",
    0, 0, 0, false, true, false, kGroupMics };
const ParamInfo kPresetInfo { "Preset", "preset", nullptr,
    "Whole configurations: the instrument and every setting (the velocity curve, the felt, the tuning file "
    "among them). Picking one switches to it. Save as… keeps the current one under a name.",
    0, 0, 0, false, true, false, kGroupInstrument };

} // namespace

const ParamInfo& paramInfo(uint32_t index)
{
    static const Table t;
    return index == kInstrumentRow ? kInstrumentInfo
         : index == kMicPresetRow  ? kMicPresetInfo
         : index == kPresetRow     ? kPresetInfo
                                   : t.p[index < kParamCount ? index : 0];
}

uint32_t paramLabelCount(uint32_t index)
{
    switch (index) {
    case kInstrumentRow:
    case kMicPresetRow:
    case kPresetRow: return 1; // a dropdown (the UI fills its list)
    case kMicPosition: return 6;
    case kTemperament: return pe_temperament_count() + 1; // the named ones, then "Scala file"
    case kTemperamentKey: return 12;
    case kDistanceLevel: return 2;
    case kPrepMode: return 6;
    case kMallet: return kMalletCount;
    default: return 0;
    }
}

const char* paramLabel(uint32_t index, uint32_t value)
{
    if (value >= paramLabelCount(index))
        return nullptr;
    switch (index) {
    case kInstrumentRow:
    case kMicPresetRow:
    case kPresetRow: return "";
    case kMicPosition: return kMicNames[value];
    case kTemperament: return value == pe_temperament_count() ? "Scala file" : pe_temperament_name(value);
    case kTemperamentKey: return kKeys[value];
    case kDistanceLevel: return kLevelNames[value];
    case kPrepMode: return kPrepNames[value];
    case kMallet: return kMalletNumbers[value];
    default: return nullptr;
    }
}

namespace {
struct DiagTable {
    static constexpr uint32_t kFirst = (uint32_t)kParamCount; // an explicit integer: no enum arithmetic (C++20 deprecates mixing enums)
    ParamInfo p[kTotalParams - kFirst];
    DiagTable()
    {
        auto set = [&](uint32_t i, const char* name, const char* symbol, float def, float max, bool integer) {
            p[i - kFirst] = { name, symbol, nullptr, nullptr, def, 0.0f, max, false, integer, false, kGroupInstrument };
        };
        set(kDiagPedalIn, "UI sustain pedal", "diag_pedal_in", 0.0f, 1.0f, false);
        set(kDiagSostenutoIn, "UI sostenuto pedal", "diag_sostenuto_in", 0.0f, 1.0f, false);
        set(kDiagSoftIn, "UI soft pedal", "diag_soft_in", 0.0f, 1.0f, false);
        set(kDiagPanicIn, "UI panic", "diag_panic_in", 0.0f, 1.0f, false);
        set(kDiagLearnIn, "UI MIDI learn", "diag_learn_in", 0.0f, (float)(kParamCount + 3), true); // every MIDI learn target + 1 (MidiMap.hpp kTargetCount)
        set(kDiagMapSeqIn, "UI MIDI map", "diag_map_seq_in", 0.0f, 16777215.0f, true);
        set(kDiagFeltOriginalIn, "UI felt original", "diag_felt_original_in", 0.0f, 1.0f, true);
        static const char* const names[8] = { "Keys 1", "Keys 2", "Keys 3", "Keys 4", "Keys 5", "Keys 6", "Keys 7", "Keys 8" };
        static const char* const symbols[8] = { "diag_keys_0", "diag_keys_1", "diag_keys_2", "diag_keys_3", "diag_keys_4", "diag_keys_5", "diag_keys_6", "diag_keys_7" };
        for (uint32_t i = 0; i < 8; ++i)
            set(kDiagKeys0 + i, names[i], symbols[i], 0.0f, 16777215.0f, true);
        set(kDiagPedalOut, "Pedal depth", "diag_pedal", 0.0f, 1.0f, false);
        set(kDiagSostenutoOut, "Sostenuto", "diag_sostenuto", 0.0f, 1.0f, false);
        set(kDiagSoftOut, "Soft pedal", "diag_soft", 0.0f, 1.0f, false);
        set(kDiagVoices, "Voices", "diag_voices", 0.0f, 176.0f, true);
        set(kDiagToken, "Diagnostics token", "diag_token", 0.0f, 16777215.0f, true);
        set(kDiagLearnedOut, "MIDI learned", "diag_learned", 0.0f, 4096.0f, true);
        set(kDiagInstrumentOut, "MIDI instrument", "diag_instrument", 0.0f, 16777215.0f, true);
        set(kDiagBuildingOut, "Building", "diag_building", 0.0f, 1.0f, true);
        set(kDiagFeltOriginalOut, "Felt original", "diag_felt_original", 0.0f, 1.0f, true);
    }
};
} // namespace

const ParamInfo& diagInfo(uint32_t index)
{
    static const DiagTable t;
    const uint32_t first = (uint32_t)kParamCount;
    return t.p[(index >= first && index < kTotalParams ? index : first) - first];
}

bool diagIsOutput(uint32_t index) { return index >= kDiagKeys0 && index < kTotalParams; }

} // namespace pe
