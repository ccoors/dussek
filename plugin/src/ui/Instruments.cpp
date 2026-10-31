#include "Instruments.hpp"
#include "Params.hpp"

#include <algorithm>
#include <cctype>
#include <cstring>

namespace pe {

const InstrumentEntry& noInstrument()
{
    static const InstrumentEntry none = [] {
        InstrumentEntry e;
        e.name = "";
        e.info.recorded_lo = 21;
        e.info.recorded_hi = 108;
        e.info.registers = 1;
        return e;
    }();
    return none;
}

BodyKind entryBody(const InstrumentEntry& e) { return (BodyKind)e.info.body; }

uint32_t entryMallets(const InstrumentEntry& e) { return std::min<uint32_t>(e.info.mallet_count, kMalletCount); }

std::string malletLabel(const InstrumentEntry& e, uint32_t i)
{
    if (i >= entryMallets(e))
        return "";
    std::string s = e.info.mallets[i];
    if (!s.empty())
        s[0] = (char)std::toupper((unsigned char)s[0]);
    return s;
}

uint32_t effectiveMallet(const InstrumentEntry& e, uint32_t value) { return value < entryMallets(e) ? value : 0; }

void entryRecorded(const InstrumentEntry& e, int& lo, int& hi)
{
    lo = (int)e.info.recorded_lo;
    hi = (int)e.info.recorded_hi;
    if (lo < 21 || hi > 108 || lo > hi) {
        lo = 21;
        hi = 108;
    }
}

bool entrySoftPedal(const InstrumentEntry& e) { return e.info.soft_pedal; }

namespace {

/// the engine's report's control for a parameter (PE_CONTROL_COUNT: none)
uint32_t controlOf(uint32_t param)
{
    switch (param) {
    case kSympathetic: return PE_CONTROL_SYMPATHETIC;
    case kPedalNoise: return PE_CONTROL_PEDAL_NOISE;
    case kPedalRing: return PE_CONTROL_PEDAL_RING;
    case kKnock: return PE_CONTROL_KNOCK;
    case kRelease: return PE_CONTROL_RELEASE;
    case kHammerHardness: return PE_CONTROL_HAMMER_HARDNESS;
    case kHammerGrooves: return PE_CONTROL_HAMMER_GROOVES;
    case kDamperBuzz: return PE_CONTROL_DAMPER_BUZZ;
    case kRattle: return PE_CONTROL_RATTLE;
    case kWearHammers: return PE_CONTROL_WEAR_HAMMERS;
    case kWearVoicing: return PE_CONTROL_WEAR_VOICING;
    case kWearDampers: return PE_CONTROL_WEAR_DAMPERS;
    case kWearAction: return PE_CONTROL_WEAR_ACTION;
    case kPhantom: return PE_CONTROL_PHANTOM;
    case kUnisonWidth: return PE_CONTROL_UNISON_WIDTH;
    case kSustain: return PE_CONTROL_SUSTAIN;
    case kVariation: return PE_CONTROL_VARIATION;
    case kRestrike: return PE_CONTROL_RESTRIKE;
    case kBoardBass: return PE_CONTROL_BOARD_BASS;
    case kStretch: return PE_CONTROL_STRETCH;
    case kWearUnisons: return PE_CONTROL_WEAR_UNISONS;
    case kWearTuning: return PE_CONTROL_WEAR_TUNING;
    case kWearStrings: return PE_CONTROL_WEAR_STRINGS;
    case kWearSoundboard: return PE_CONTROL_WEAR_SOUNDBOARD;
    case kWearFalseBeats: return PE_CONTROL_WEAR_FALSE_BEATS;
    case kAudioInput: return PE_CONTROL_AUDIO_INPUT;
    case kDynamics: return PE_CONTROL_DYNAMICS;
    default: return PE_CONTROL_COUNT;
    }
}

bool offers(const InstrumentEntry& e, uint32_t prep) { return prep >= 1 && prep <= 5 && (e.info.preparations >> prep & 1); }

} // namespace

const char* displayName(uint32_t param, const InstrumentEntry& e)
{
    if (param < kParamCount)
        if (const PeLabel* l = e.label(paramInfo(param).symbol))
            return l->name;
    if (param == kHammerHardness)
        switch (e.info.exciter) {
        case PE_EXCITER_QUILL: return "Quill brightness";
        case PE_EXCITER_FINGER: return "Finger brightness";
        case PE_EXCITER_PICK: return "Pick brightness";
        case PE_EXCITER_TANGENT: return "Tangent brightness";
        case PE_EXCITER_MALLET: return "Mallet hardness";
        case PE_EXCITER_THUMB: return "Thumb brightness";
        default: break;
        }
    // the condition details: short names (the section says what they are)
    static const char* const aspects[] = { "Hammers", "Voicing", "Action noise", "Dampers", "Unisons", "Tuning",
        "False beats", "Strings", "Soundboard" }; // kWearHammers … kWearSoundboard, enum order
    static_assert(sizeof aspects / sizeof aspects[0] == kWearSoundboard - kWearHammers + 1, "one name per aspect");
    if (param >= kWearHammers && param <= kWearSoundboard)
        return aspects[param - kWearHammers];
    if (param == kWearVariant)
        return "Variant";
    return paramInfo(param).name;
}

const char* displayDescription(uint32_t param, const InstrumentEntry& e)
{
    if (param < kParamCount)
        if (const PeLabel* l = e.label(paramInfo(param).symbol); l != nullptr && l->description[0] != '\0')
            return l->description;
    const char* d = paramInfo(param).description;
    return d != nullptr ? d : "";
}

bool paramActive(uint32_t param, const InstrumentEntry& e, uint32_t micPosition, uint32_t prep)
{
    const PeInstrumentInfo& i = e.info;
    // the preparations: None 0, Plucked 1, Muted 2, Harmonic 3, Bolt 4, Paper 5
    switch (param) {
    case kPluckPosition:
    case kPluckBrightness: return prep == 1 && offers(e, 1); // a harpsichord plucks anyway
    case kMuteStrength: return prep == 2;
    case kHarmonicNumber: return prep == 3;
    case kBoltMass:
    case kBoltPosition: return prep == 4;
    case kPaperBuzz: return prep == 5;
    default: break;
    }
    // the stops that play a register (a measured second 8′ without a 4′: the 8′ and the 8′ II)
    if (param == kStop8 || param == kStop4 || param == kStop8b)
        return (i.stops >> (param == kStop8 ? 0 : param == kStop4 ? 1 : 2) & 1) != 0;
    if (param == kLute)
        return i.lute;
    if (param == kBuff)
        return i.buff;
    if (param == kMotor || param == kMotorSpeed || param == kMotorDepth)
        return i.motor;
    if (param == kMallet)
        return entryMallets(e) > 0;
    if (param == kBebung)
        return i.bebung;
    if (param == kBendRange)
        return i.bend;
    if (param == kMorphX || param == kMorphY) // the Morph tab's pad, never a row
        return false;
    if ((param == kMicDistance || param == kMicSpacing) && (micPosition == 0 || micPosition == 5))
        return false;
    if ((param == kLid || param == kLidOff) && (micPosition == 0 || !i.lid)) // no lid
        return false;
    if (param == kSpreadKeys) // a range of its own, at a microphone position (the recording's has one zone)
        return i.spread && micPosition != 0;
    if (param >= kMicLX && param <= kMicRPitch) {
        if (micPosition != 5)
            return false;
        if (param == kMicWall)
            return entryBody(e) == BodyKind::Upright;
    }
    // without dampers: no damper fall, nothing for the pedal's latch to hold
    if ((param == kDamperFall || param == kSustainLatch) && !i.dampers)
        return false;
    if (const uint32_t c = controlOf(param); c != PE_CONTROL_COUNT && (i.inert >> c & 1))
        return false;
    return true;
}

uint32_t entryPreparations(const InstrumentEntry& e, uint32_t* values)
{
    uint32_t n = 0;
    for (uint32_t p = 1; p <= 5; ++p)
        if (offers(e, p))
            values[n++] = p;
    return n;
}

uint32_t effectivePrep(const InstrumentEntry& e, uint32_t value) { return offers(e, value) ? value : 0; }

bool paramVisible(uint32_t param, const UiContext& c)
{
    const InstrumentEntry& e = c.inst != nullptr ? *c.inst : noInstrument();
    return paramActive(param, e, c.micPosition, effectivePrep(e, c.prep));
}

uint32_t exclusiveStop(uint32_t param)
{
    return param == kStop8b ? kLute : param == kLute ? kStop8b : kParamCount;
}

} // namespace pe
