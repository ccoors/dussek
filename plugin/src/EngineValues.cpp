#include "EngineValues.hpp"
#include "Catalog.hpp"

#include "Instruments.hpp"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

namespace pe {

namespace {

long rounded(float v) { return std::lround(v); }

bool on(float v) { return v >= 0.5f; }

/// the zones: each one's upper edge (slider %), the voicing's wear there and every other aspect's
/// (Normal is the keys' unevenness alone: detuning, worn dampers and the rest begin in Worn)
struct Zone {
    const char* name;
    double percent;
    double voicing;
    double others;
};
const Zone kZones[] = { { "Mint", 0.0, 0.0, 0.0 }, { "Normal", 10.0, 0.05, 0.0 }, { "Worn", 35.0, 0.2, 0.2 },
    { "Broken", 100.0, 1.0, 1.0 } };
const int kZoneCount = (int)(sizeof kZones / sizeof kZones[0]);

/// effective wear of one aspect: the condition's zones, then its offset on the wear scale, 0 … 1
double wear(const float* values, uint32_t index)
{
    // the whole percent shown (a host may still send a fraction): Mint is wear 0 exactly
    return std::max(0.0, std::min(1.0, conditionWear(std::round(values[kCondition]), index) + values[index] / 100.0));
}

/// the stored preparation (one the instrument does not offer does nothing on it: engine/tests/no_effect.rs)
uint32_t prepOf(const float* values)
{
    return (uint32_t)std::max(0L, rounded(values[kPrepMode]));
}

/// one word for the shell: in single quotes unless it needs none
std::string quoted(const std::string& s)
{
    if (!s.empty() && s.find_first_not_of("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/._-+,=:") == std::string::npos)
        return s;
    std::string out = "'";
    for (char c : s)
        out += c == '\'' ? std::string("'\\''") : std::string(1, c);
    return out + "'";
}

} // namespace

double conditionWear(double percent, uint32_t index)
{
    const auto at = [index](const Zone& z) { return index == kWearVoicing ? z.voicing : z.others; };
    if (!(percent > kZones[0].percent))
        return at(kZones[0]);
    for (int i = 1; i < kZoneCount; ++i)
        if (percent <= kZones[i].percent) {
            const Zone& a = kZones[i - 1];
            const Zone& b = kZones[i];
            return at(a) + (at(b) - at(a)) * (percent - a.percent) / (b.percent - a.percent);
        }
    return at(kZones[kZoneCount - 1]);
}

const char* conditionZone(double percent)
{
    if (percent < 1.0) // Mint is the exports as they are: only the slider's bottom
        return kZones[0].name;
    for (int i = 1; i < kZoneCount - 1; ++i)
        if (percent <= kZones[i].percent)
            return kZones[i].name;
    return kZones[kZoneCount - 1].name;
}

bool conditionZoneValue(const std::string& name, float& out)
{
    for (const Zone& z : kZones)
        if (name.size() == std::strlen(z.name)
            && std::equal(name.begin(), name.end(), z.name,
                [](char a, char b) { return std::tolower((unsigned char)a) == std::tolower((unsigned char)b); })) {
            out = (float)z.percent;
            return true;
        }
    return false;
}

float dbGain(float db) { return db <= kOffDb ? 0.0f : std::pow(10.0f, db / 20.0f); }

PeParams engineParams(const float* values)
{
    const float* v = values;
    return PeParams {
        dbGain(v[kRoom]),
        dbGain(v[kKnock]),
        dbGain(v[kRelease]),
        dbGain(v[kPedalNoise]),
        v[kSympathetic] / 100.0f,
        dbGain(v[kVolume]),
        std::pow(10.0f, v[kHammerHardness] / 20.0f),
        (float)wear(v, kWearHammers),
        (float)wear(v, kWearVoicing),
        (float)wear(v, kWearDampers),
        (float)wear(v, kWearAction),
        v[kPhantom] / 100.0f,
        v[kHammerGrooves] / 100.0f,
        v[kDamperBuzz] / 100.0f,
        v[kRattle] / 100.0f,
        (on(v[kStop8]) ? 1u : 0u) | (on(v[kStop4]) ? 2u : 0u) | (on(v[kStop8b]) ? 4u : 0u),
        v[kDamperFall] / 100.0f,
        prepOf(v),
        v[kPluckPosition] / 100.0f,
        v[kPluckBrightness] / 100.0f,
        v[kMuteStrength] / 100.0f,
        (uint32_t)std::max(2L, std::min(12L, rounded(v[kHarmonicNumber]))),
        v[kPaperBuzz] / 100.0f,
        on(v[kBuff]) ? 1u : 0u,
        on(v[kLute]) ? 1u : 0u,
        on(v[kMotor]) ? 1u : 0u,
        v[kMotorSpeed],
        v[kMotorDepth] / 100.0f,
        (uint32_t)std::max(0L, rounded(v[kMallet])), // the instrument's own (the engine plays its first past its count)
        v[kBebung],
        v[kBendRange],
        dbGain(v[kAudioInput]),
        v[kDynamics],
        v[kVariation] / 100.0f,
        on(v[kRestrike]) ? 1.0f : 0.0f,
    };
}

double engineLid(const float* v)
{
    if (on(v[kLidOff]))
        return 2.0;
    return std::isfinite(v[kLid]) ? std::max(0.0, std::min(1.0, v[kLid] / 100.0)) : 1.0;
}

PeTuning engineTuning(const float* values)
{
    const float* v = values;
    PeTuning t {};
    t.a4_hz = v[kConcertPitch];
    t.unison_width = v[kUnisonWidth] / 100.0;
    t.sustain = v[kSustain] / 100.0;
    t.board_bass_db = v[kBoardBass];
    double* const compiled[5] = { &t.unison_wear, &t.tuning_wear, &t.string_wear, &t.board_wear, &t.falsebeat_wear };
    for (int i = 0; i < 5; ++i)
        *compiled[i] = wear(v, kCompiledWear[i]);
    t.wear_seed = (uint32_t)std::max(0L, rounded(v[kWearVariant]));
    t.mic_position = (uint32_t)std::max(0L, std::min(5L, rounded(v[kMicPosition])));
    t.mic_spacing = std::max(0.0, std::min(10.0, v[kMicSpacing] / 100.0));
    t.mic_distance = std::max(0.0, std::min(20.0, (double)v[kMicDistance]));
    t.distance_level = on(v[kDistanceLevel]) ? 1 : 0;
    t.temperament = (uint32_t)std::max(0L, std::min((long)pe_temperament_count(), rounded(v[kTemperament]))); // the last: a Scala file
    t.temperament_key = (uint32_t)std::max(0L, std::min(11L, rounded(v[kTemperamentKey])));
    t.stretch = v[kStretch];
    // the bolt only with the preparation (every instrument has it: ui_tests); its position only with a bolt
    t.bolt_mass = prepOf(v) == 4 ? v[kBoltMass] / 100.0 : 0.0;
    t.bolt_pos = (t.bolt_mass > 0.0 ? v[kBoltPosition] : paramInfo(kBoltPosition).def) / 100.0;
    // the free pair only at its position (the parameters' defaults elsewhere)
    const bool custom = t.mic_position == 5;
    const auto setup = [&](uint32_t index) { return custom ? (double)v[index] : (double)paramInfo(index).def; };
    for (int i = 0; i < 2; ++i) {
        for (int k = 0; k < 3; ++k)
            t.mic_setup.pos[i][k] = setup(kMicLX + 3 * i + k);
        t.mic_setup.pattern[i] = setup(kMicLPattern + 3 * i) / 100.0; // percent in the parameter
        t.mic_setup.yaw[i] = setup(kMicLYaw + 3 * i);
        t.mic_setup.pitch[i] = setup(kMicLPitch + 3 * i);
    }
    t.mic_setup.wall = setup(kMicWall);
    // the lid and the spread keys only at a microphone position (the recording's has neither)
    const bool placed = t.mic_position != 0;
    t.lid = placed ? engineLid(v) : 1.0;
    t.pillow = std::isfinite(v[kPillow]) ? std::max(0.0, std::min(1.0, v[kPillow] / 100.0)) : 0.0;
    t.spread_keys = placed && on(v[kSpreadKeys]) ? 1 : 0;
    return t;
}

bool sameTuning(const PeTuning& a, const PeTuning& b)
{
    const PeMicSetup &s = a.mic_setup, &r = b.mic_setup;
    for (int i = 0; i < 2; ++i) {
        for (int k = 0; k < 3; ++k)
            if (s.pos[i][k] != r.pos[i][k])
                return false;
        if (s.pattern[i] != r.pattern[i] || s.yaw[i] != r.yaw[i] || s.pitch[i] != r.pitch[i])
            return false;
    }
    return s.wall == r.wall && a.a4_hz == b.a4_hz && a.unison_width == b.unison_width && a.sustain == b.sustain
        && a.board_bass_db == b.board_bass_db
        && a.unison_wear == b.unison_wear
        && a.tuning_wear == b.tuning_wear && a.string_wear == b.string_wear && a.board_wear == b.board_wear
        && a.falsebeat_wear == b.falsebeat_wear && a.wear_seed == b.wear_seed && a.mic_position == b.mic_position
        && a.mic_spacing == b.mic_spacing && a.mic_distance == b.mic_distance && a.distance_level == b.distance_level
        && a.temperament == b.temperament && a.temperament_key == b.temperament_key && a.stretch == b.stretch
        && a.bolt_mass == b.bolt_mass && a.bolt_pos == b.bolt_pos && a.lid == b.lid && a.pillow == b.pillow
        && a.spread_keys == b.spread_keys;
}

std::string instrumentDir(const char* resources, const char* id)
{
    if (const InstrumentEntry* e = Catalog::outside(id != nullptr ? id : ""))
        return e->dir; // kept anywhere: its own path
    // the catalog's test, path by path: the entry the catalog lists for the id
    for (const std::string& c : Catalog::searchPaths(resources)) {
        InstrumentEntry e;
        std::string message;
        if (Catalog::entryAt(c, id != nullptr ? id : "", e, message))
            return e.dir;
    }
    return std::string();
}

std::string midi2wavLine(const float* values, const std::string& dir, double rate, const PeVelocityCurve* curve,
    const std::vector<std::pair<std::string, float>>& morph, const std::string& felt, const std::string& scale)
{
    const PeParams p = engineParams(values);
    PeTuning t = engineTuning(values);
    t.scale_file = scale.c_str();
    std::vector<char> spec(1024);
    size_t n = pe_format_spec(&p, &t, curve, spec.data(), spec.size());
    if (n >= spec.size()) {
        spec.resize(n + 1);
        n = pe_format_spec(&p, &t, curve, spec.data(), spec.size());
    }
    char r[32];
    std::snprintf(r, sizeof(r), "%.17g", rate);
    std::string line = "pianoengine render " + quoted(dir.empty() ? std::string("<instrument>") : dir)
        + " in.mid out.wav --rate " + r;
    if (n > 0)
        line += " " + std::string(spec.data(), n);
    if (on(values[kSustainLatch]))
        line += " --latch";
    for (const auto& [d, w] : morph) {
        if (w <= 0)
            continue;
        char ws[16];
        std::snprintf(ws, sizeof(ws), "%.2f", (double)w);
        std::string s = ws; // up to two decimals: "0.5", "0.25"
        while (s.back() == '0') s.pop_back();
        if (s.back() == '.') s.pop_back();
        line += " --morph " + quoted(d + "=" + s);
    }
    if (!felt.empty())
        line += " --felt " + quoted(felt);
    return line;
}

} // namespace pe
