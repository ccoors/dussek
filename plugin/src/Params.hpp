/*
 * The plugin's parameters: one table for the DSP (initParameter) and the UI.
 * Order, symbols, ranges and hints are the plugin's interface: do not change them here
 * without meaning to (parameter indices are not stable across versions yet).
 */
#pragma once
#include <cstdint>

namespace pe {

// in display order, by group (the groups in the order of the UI's sections)
enum Param : uint32_t {
    // instrument
    kStop8, kStop4, kStop8b, kLute, kBuff, kMotor, kSustainLatch, kVolume, kHammerHardness,
    kMotorSpeed, kMotorDepth, kMallet, kBebung, kBendRange, kDynamics, kSustain, kBoardBass, kVariation, kRestrike,
    // the morph: the pad's point (the Morph tab; Morph.hpp)
    kMorphX, kMorphY,
    // microphones
    kMicPosition, kMicDistance, kMicSpacing,
    kMicLX, kMicLY, kMicLZ, kMicRX, kMicRY, kMicRZ, kMicWall,
    kMicLPattern, kMicLYaw, kMicLPitch, kMicRPattern, kMicRYaw, kMicRPitch, kLid, kLidOff, kPillow, kSpreadKeys, kDistanceLevel, kRoom,
    // tuning
    kConcertPitch, kTemperament, kTemperamentKey, kStretch, kUnisonWidth,
    // noises & resonance
    kKnock, kRelease, kPedalNoise, kPedalRing, kSympathetic, kAudioInput, kDamperFall,
    // condition: the macro, the variant, per-aspect offsets
    kCondition, kWearVariant, kWearHammers, kWearVoicing, kWearAction, kWearDampers, kWearUnisons,
    kWearTuning, kWearFalseBeats, kWearStrings, kWearSoundboard,
    // optional defects
    kHammerGrooves, kDamperBuzz, kRattle, kPhantom,
    // the prepared piano: the preparation and each one's own parameters
    kPrepMode, kPluckPosition, kPluckBrightness, kMuteStrength, kHarmonicNumber, kBoltMass, kBoltPosition, kPaperBuzz,
    kParamCount
};
// per microphone pattern, yaw, pitch are consecutive: kMicLPattern + 3 * i + { 0, 1, 2 }
// MIDI learn's per-parameter tables hold 128 (the telemetry region's value mirror, PE_TELEMETRY_VALUES;
// UiContext::tied)
static_assert(kParamCount <= 128, "MIDI learn's tables hold 128 parameters");
static_assert(kMicLYaw == kMicLPattern + 1 && kMicLPitch == kMicLPattern + 2 && kMicRPattern == kMicLPattern + 3
        && kMicRYaw == kMicLPattern + 4 && kMicRPitch == kMicLPattern + 5,
    "pattern, yaw, pitch per microphone");

enum Group : uint32_t { kGroupInstrument, kGroupMics, kGroupTuning, kGroupNoises, kGroupCondition, kGroupDefects, kGroupPrepared, kGroupCount };

struct ParamInfo {
    const char* name;
    const char* symbol;
    const char* unit;        // nullptr: none
    const char* description; // nullptr: none
    float def, min, max;
    bool automatable, integer, boolean;
    Group group;
};

// diagnostics: hidden parameters appended after the user-visible table (kParamCount stays
// the number of user parameters). Keys: 8 words, 2 bits per key (held, ringing), 12 keys per word.
enum DiagParam : uint32_t {
    kDiagPedalIn = kParamCount, // input: the UI's sustain pedal (UI cannot send CC), 0 ... 1
    kDiagSostenutoIn,           // input: the UI's sostenuto pedal, 0 (up) or 1 (down)
    kDiagSoftIn,                // input: the UI's soft pedal, 0 ... 1
    kDiagPanicIn,               // input: the UI's Panic button, flipped 0 <-> 1 at each click (PanicTrigger)
    kDiagLearnIn,               // input: MIDI learn's waiting control, target + 1 (MidiMap.hpp; 0: none)
    kDiagMapSeqIn,              // input: raised by the UI after it wrote the MIDI map's file (the DSP reads it again)
    kDiagFeltOriginalIn,        // input: the Felt tab's Original toggle, flipped 0 <-> 1 at each click (OriginalToggle)
    kDiagKeys0, kDiagKeys1, kDiagKeys2, kDiagKeys3, kDiagKeys4, kDiagKeys5, kDiagKeys6, kDiagKeys7, // outputs
    kDiagPedalOut, kDiagSostenutoOut, kDiagSoftOut, kDiagVoices, kDiagToken,
    kDiagLearnedOut,    // output: the CC caught for kDiagLearnIn (packLearned; 0: none yet)
    kDiagInstrumentOut, // output: an instrument MIDI chose, 1 + its menu position + 1024 × a count (0: none yet)
    kDiagBuildingOut,   // output: 1 while the worker builds an engine (the Morph tab's "building…")
    kDiagFeltOriginalOut, // output: 1 while the shipped felt plays (the Original toggle shows it)
    kDiagEnd
};
constexpr uint32_t kTotalParams = kDiagEnd;
/// the Instrument menu's row in the UI: no parameter (the "instrument" state, Catalog.hpp); paramInfo,
/// paramLabelCount know it (a dropdown), its value is the UI's
constexpr uint32_t kInstrumentRow = kTotalParams;
/// the Preset rows (Presets.hpp): the Mics tab's (microphone presets) and the Sound tab's (everything);
/// no parameters, dropdowns the UI fills
constexpr uint32_t kMicPresetRow = kTotalParams + 1;
constexpr uint32_t kPresetRow = kTotalParams + 2;
/// the UI's rows: the parameters, then the rows that are none
constexpr uint32_t kRowCount = kTotalParams + 3;
const ParamInfo& diagInfo(uint32_t index); // kParamCount <= index < kTotalParams
bool diagIsOutput(uint32_t index);

const ParamInfo& paramInfo(uint32_t index); // index < kParamCount, kInstrumentRow, kMicPresetRow or kPresetRow
/// enumerations: the number of labels (values min … max), else 0
uint32_t paramLabelCount(uint32_t index);
/// an enumeration's label for value (0-based from min), nullptr out of range
const char* paramLabel(uint32_t index, uint32_t value);

/// the wear offsets: name, symbol, compiled into the strings (a change rebuilds the engine)
struct WearAspect {
    Param param;
    const char* name;
    const char* symbol;
    bool compiled;
    const char* description;
};
extern const WearAspect kWearAspects[9];
const WearAspect* wearAspect(uint32_t index);
/// the compiled wear in PeTuning order
extern const Param kCompiledWear[5];

/// the Mallet parameter's values: an instrument's own mallets, at most this many (dussek.h PE_INFO_MALLETS)
constexpr uint32_t kMalletCount = 8;
constexpr float kOffDb = -40.0f; // levels at or below this are off

} // namespace pe
