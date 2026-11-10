#include "Ports.hpp"

#include <algorithm>
#include <array>

namespace pe {

namespace {

constexpr uint32_t kReleasedKeys = 78; // v0.3.2's table: kStop8 … kPaperBuzz
static_assert(kPaperBuzz + 1 == kReleasedKeys, "Dussek's ports end where v0.3.2's table did");
constexpr uint32_t kDiagCount = kTotalParams - kParamCount;

struct Map {
    std::vector<uint32_t> ports;
    std::array<uint32_t, kParamCount> host;
};

Map build(PluginKind k)
{
    Map m;
    if (k == PluginKind::Keys) {
        for (uint32_t i = 0; i < kReleasedKeys; ++i)
            m.ports.push_back(i);
    } else {
        // what the brass plays (the controls its instruments show) and its own rows
        m.ports = { kVolume, kDynamics, kVariation, kMicPosition, kMicDistance, kMicSpacing, kMicLX, kMicLY, kMicLZ,
            kMicRX, kMicRY, kMicRZ, kMicLPattern, kMicLYaw, kMicLPitch, kMicRPattern, kMicRYaw, kMicRPitch,
            kDistanceLevel, kRoom, kConcertPitch, kTemperament, kTemperamentKey };
        for (uint32_t i = kBrassMute; i <= kAirAdded; ++i)
            m.ports.push_back(i);
        std::sort(m.ports.begin(), m.ports.end());
    }
    m.host.fill(kNoPort);
    for (uint32_t h = 0; h < m.ports.size(); ++h)
        m.host[m.ports[h]] = h;
    return m;
}

const Map& map(PluginKind k)
{
    static const Map keys = build(PluginKind::Keys), winds = build(PluginKind::Winds);
    return k == PluginKind::Winds ? winds : keys;
}

} // namespace

const std::vector<uint32_t>& ports(PluginKind k) { return map(k).ports; }

uint32_t hostCount(PluginKind k) { return (uint32_t)map(k).ports.size() + kDiagCount; }

uint32_t toHost(PluginKind k, uint32_t internal)
{
    const Map& m = map(k);
    if (internal < kParamCount)
        return m.host[internal];
    if (internal < kTotalParams)
        return (uint32_t)m.ports.size() + (internal - kParamCount);
    return kNoPort;
}

uint32_t toInternal(PluginKind k, uint32_t host)
{
    const Map& m = map(k);
    if (host < m.ports.size())
        return m.ports[host];
    if (host < hostCount(k))
        return kParamCount + (host - (uint32_t)m.ports.size());
    return kNoPort;
}

float defaultOf(PluginKind k, uint32_t internal)
{
    if (k == PluginKind::Winds) {
        if (internal == kMicPosition)
            return 2.0f; // Audience: a player arriving must not land behind the bell
        if (internal == kVariation)
            return 100.0f; // the player's as it was fitted
    }
    return paramInfo(internal).def;
}

namespace {

struct Text {
    uint32_t param;
    const char* name; // nullptr: the table's
    const char* description;
};

// Weidinger's where the table's speak of a piano: the bell's frame has its mouth at the origin, x to the player's right,
// y to the player's front, z up (mic.rs, `bell_placement`)
constexpr Text kWindsTexts[] = {
    { kDynamics, nullptr,
      "How the velocity maps to the air: less brings the soft notes up, with more air, so also brighter; more takes "
      "them down. 45 dB plays the velocity as it comes." },
    { kVariation, "Player's variation",
      "How much each note differs from the last in the virtual player's hands: its air, vibrato and intonation. "
      "100 % as it was fitted, 0 every note alike. From the next note." },
    { kMicPosition, nullptr,
      "Where the two microphones stand: Recording (the recording's own sound, 1.5 m on the bell's axis), Player (at the "
      "player's ears), Audience (the default: 6 m in front, at the player's height), Close 1 (30 cm on the bell's axis), "
      "Close 2 (30 cm from the bell, well off its axis) or Custom (both placed freely). Click a marker in the drawing to "
      "choose one." },
    { kMicLX, nullptr,
      "From the bell's mouth: x to the player's right (+) or left (-), y to the player's front (+) or back (-), z up. "
      "Dragging a microphone in the drawing sets these." },
    { kMicLY, nullptr, "Left microphone, y: to the player's front (+) or back (-) of the bell's mouth." },
    { kMicLZ, nullptr, "Left microphone, z: the height above the bell's mouth." },
    { kMicRX, nullptr,
      "Right microphone, x: to the player's right (+) or left (-) of the bell's mouth. Dragging a microphone in the "
      "drawing sets x, y and z." },
    { kMicRY, nullptr, "Right microphone, y: to the player's front (+) or back (-) of the bell's mouth." },
    { kMicRZ, nullptr, "Right microphone, z: the height above the bell's mouth." },
    { kMicLYaw, nullptr, "Direction in the floor plane: 0 the way the player faces, positive towards the player's right." },
    { kConcertPitch, nullptr,
      "The pitch of A4. The instrument is rebuilt in the background; the sounding notes take the new tuning." },
};

const Text* windsText(uint32_t internal)
{
    for (const Text& t : kWindsTexts)
        if (t.param == internal)
            return &t;
    return nullptr;
}

} // namespace

const char* nameOf(PluginKind k, uint32_t internal)
{
    if (k == PluginKind::Winds)
        if (const Text* t = windsText(internal); t != nullptr && t->name != nullptr)
            return t->name;
    return paramInfo(internal).name;
}

const char* descriptionOf(PluginKind k, uint32_t internal)
{
    if (k == PluginKind::Winds)
        if (const Text* t = windsText(internal); t != nullptr)
            return t->description;
    const char* d = paramInfo(internal).description;
    return d != nullptr ? d : "";
}

const char* labelOf(PluginKind k, uint32_t internal, uint32_t value)
{
    // the bell's positions (mic.rs, BELL_CLOSE): Close 1 on its axis, Close 2 well off it
    static const char* const winds[6] = { "Recording", "Player", "Audience", "Close 1: on the bell's axis",
        "Close 2: off the bell's axis", "Custom" };
    if (k == PluginKind::Winds && internal == kMicPosition && value < 6)
        return winds[value];
    return paramLabel(internal, value);
}

const std::vector<uint32_t>& portGroups(PluginKind k)
{
    static const auto groups = [](PluginKind kind) {
        std::vector<uint32_t> g;
        for (uint32_t i : ports(kind))
            if (std::find(g.begin(), g.end(), (uint32_t)paramInfo(i).group) == g.end())
                g.push_back(paramInfo(i).group);
        std::sort(g.begin(), g.end());
        return g;
    };
    static const std::vector<uint32_t> keys = groups(PluginKind::Keys), winds = groups(PluginKind::Winds);
    return k == PluginKind::Winds ? winds : keys;
}

uint32_t hostGroup(PluginKind k, uint32_t group)
{
    const std::vector<uint32_t>& g = portGroups(k);
    const auto it = std::find(g.begin(), g.end(), group);
    return it == g.end() ? kNoPort : (uint32_t)(it - g.begin());
}

bool hasPort(PluginKind k, uint32_t internal) { return internal < kParamCount && map(k).host[internal] != kNoPort; }

} // namespace pe
