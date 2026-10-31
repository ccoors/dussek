#include "MidiMap.hpp"
#include "Paths.hpp"
#include "Format.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <sstream>

namespace pe {

bool learnableCc(uint32_t cc)
{
    return cc <= 127 && cc != 64 && cc != 66 && cc != 67 && cc != 120 && cc != 123;
}

const char* targetSymbol(uint32_t target)
{
    if (target < kParamCount)
        return paramInfo(target).symbol;
    return target == kTargetNext ? "next_instrument" : target == kTargetPrevious ? "previous_instrument"
        : target == kTargetKnob                                                     ? "instrument_knob"
                                                                                    : nullptr;
}

const char* targetName(uint32_t target)
{
    if (target < kParamCount)
        return paramInfo(target).name;
    return target == kTargetNext ? "Next instrument" : target == kTargetPrevious ? "Previous instrument"
        : target == kTargetKnob                                                     ? "Instrument (relative knob)"
                                                                                    : nullptr;
}

bool stepTarget(uint32_t target) { return target == kTargetNext || target == kTargetPrevious || target == kTargetKnob; }

int relativeSteps(uint8_t value)
{
    value &= 127;
    return value == 0 ? 1 : value < 64 ? (int)value : (int)value - 128;
}

void MidiMap::tie(uint32_t target, uint8_t channel, uint8_t cc, bool zero, bool relative, bool flip)
{
    if (target >= kTargetCount || channel > 15 || !learnableCc(cc))
        return;
    relative = relative || target == kTargetKnob;
    flip = flip && !relative && target < kParamCount && paramInfo(target).boolean;
    zero = zero && stepTarget(target) && !relative;
    // the CC's other side stays with the other step target
    const auto shares = [&](const Tie& t) {
        return stepTarget(target) && stepTarget(t.target) && !relative && !t.relative && t.zero != zero;
    };
    fTies.erase(std::remove_if(fTies.begin(), fTies.end(),
                    [&](const Tie& t) { return t.target == target || (t.channel == channel && t.cc == cc && !shares(t)); }),
        fTies.end());
    fTies.push_back({ channel, cc, target, zero, relative, flip });
}

bool MidiMap::forget(uint32_t target)
{
    const size_t n = fTies.size();
    fTies.erase(std::remove_if(fTies.begin(), fTies.end(), [&](const Tie& t) { return t.target == target; }), fTies.end());
    return fTies.size() != n;
}

const Tie* MidiMap::find(uint32_t target) const
{
    for (const Tie& t : fTies)
        if (t.target == target)
            return &t;
    return nullptr;
}

std::string MidiMap::text() const
{
    std::string s = "# PianoEngine MIDI map: program_change on|off, one \"cc <channel 1-16> <cc 0-127> <control>\" per tie\n";
    s += programChange ? "program_change on\n" : "program_change off\n";
    s += softTakeover ? "soft_takeover on\n" : "soft_takeover off\n";
    for (const Tie& t : fTies)
        s += "cc " + std::to_string(t.channel + 1) + " " + std::to_string(t.cc) + " " + targetSymbol(t.target)
            + (t.zero ? " zero\n" : t.relative ? " relative\n" : t.flip ? " flip\n" : "\n");
    return s;
}

MidiMap MidiMap::parse(const std::string& text)
{
    MidiMap m;
    std::istringstream in(text);
    std::string line;
    while (std::getline(in, line)) {
        std::istringstream l(line);
        std::string word;
        if (!(l >> word) || word[0] == '#')
            continue;
        if (word == "program_change" || word == "soft_takeover") {
            std::string v;
            if (l >> v && (v == "on" || v == "off"))
                (word == "program_change" ? m.programChange : m.softTakeover) = v == "on";
            continue;
        }
        if (word != "cc")
            continue;
        long channel = 0, cc = 0;
        std::string symbol, side, extra;
        if (!(l >> channel >> cc >> symbol) || channel < 1 || channel > 16 || cc < 0 || cc > 127)
            continue;
        l >> side;
        const bool zero = side == "zero", relative = side == "relative", flip = side == "flip";
        if ((!side.empty() && !zero && !relative && !flip) || (l >> extra))
            continue;
        for (uint32_t t = 0; t < kTargetCount; ++t)
            if (symbol == targetSymbol(t)) {
                if ((!zero || stepTarget(t)) && (!flip || (t < kParamCount && paramInfo(t).boolean)))
                    m.tie(t, (uint8_t)(channel - 1), (uint8_t)cc, zero, relative, flip);
                break;
            }
    }
    return m;
}

bool MidiMap::save(const std::string& path) const
{
    std::error_code ec;
    if (path.empty())
        return false;
    std::filesystem::create_directories(std::filesystem::path(path).parent_path(), ec);
    if (ec)
        return false;
    const std::string tmp = path + ".tmp";
    {
        std::ofstream out(tmp, std::ios::trunc);
        if (!out)
            return false;
        out << text();
        out.close();
        if (!out) {
            std::remove(tmp.c_str());
            return false;
        }
    }
    std::filesystem::rename(tmp, path, ec); // replaces the old file, also on Windows (std::rename does not)
    if (ec) {
        std::remove(tmp.c_str());
        return false;
    }
    return true;
}

MidiMap MidiMap::load(const std::string& path)
{
    std::ifstream in(path);
    if (!in)
        return MidiMap();
    std::ostringstream s;
    s << in.rdbuf();
    return parse(s.str());
}

bool MidiMap::update(const std::string& path, const std::function<void(MidiMap&)>& edit)
{
    MidiMap next = load(path);
    edit(next);
    if (!next.save(path))
        return false;
    *this = next;
    return true;
}

std::string midiMapPath()
{
    const std::string config = pe::dataDirs().config;
    return config.empty() ? config : config + "/midi-map.txt";
}

std::string tieLabel(const Tie& t)
{
    return "CC " + std::to_string(t.cc) + (t.zero ? " at 0" : t.relative && t.target != kTargetKnob ? " ±" : t.flip ? " flip" : "");
}
std::string tieChannel(const Tie& t) { return std::to_string(t.channel + 1); }

std::vector<std::string> learnMenuItems(const MidiMap& m, uint32_t target)
{
    const char* const how = target < kParamCount && paramInfo(target).boolean ? " (pad flips it)" : " (relative knob)";
    if (const Tie* t = m.find(target))
        return { "Learn again", std::string("Learn again") + how, "Forget " + tieLabel(*t) };
    return { "Learn MIDI", std::string("Learn MIDI") + how };
}

bool mirrorApplies(bool firstRead, uint32_t seq, uint32_t seenSeq, float mirror, float current)
{
    return seq != 0 && (firstRead || seq != seenSeq) && std::isfinite(mirror) && mirror != current;
}

uint32_t packLearned(uint8_t channel, uint8_t cc, bool zero)
{
    return 1u + (zero ? 2048u : 0u) + (uint32_t)(channel & 15) * 128u + (uint32_t)(cc & 127);
}

bool unpackLearned(float value, uint8_t& channel, uint8_t& cc, bool& zero)
{
    if (!(value >= 1.0f && value <= 4096.0f))
        return false;
    const uint32_t v = (uint32_t)value - 1;
    zero = v >= 2048;
    channel = (uint8_t)(v % 2048 / 128);
    cc = (uint8_t)(v % 128);
    return true;
}

ListChoices listChoices(uint32_t param, const InstrumentEntry& e)
{
    ListChoices c;
    if (param == kMallet) {
        c.count = std::max(1u, std::min(kMalletCount, entryMallets(e)));
        for (uint32_t i = 0; i < c.count; ++i)
            c.values[i] = (uint8_t)i;
    } else if (param == kPrepMode) {
        uint32_t v[5];
        const uint32_t n = entryPreparations(e, v);
        c.values[c.count++] = 0; // None
        for (uint32_t i = 0; i < n && c.count < ListChoices::kMax; ++i)
            c.values[c.count++] = (uint8_t)v[i];
    } else if (param < kParamCount) {
        c.count = std::min(ListChoices::kMax, paramLabelCount(param));
        for (uint32_t i = 0; i < c.count; ++i)
            c.values[i] = (uint8_t)i;
    }
    return c;
}

float ccToValue(uint32_t param, uint8_t value, const ListChoices& lists)
{
    value = std::min<uint8_t>(value, 127);
    if (lists.count > 0)
        return (float)lists.values[std::min(lists.count - 1, (uint32_t)value * lists.count / 128)];
    if (paramInfo(param).boolean)
        return value > 0 ? 1.0f : 0.0f;
    return fromNormalized(param, value / 127.0f);
}

float nudgeValue(uint32_t param, float current, int steps, const ListChoices& lists)
{
    if (steps == 0)
        return current;
    const ParamInfo& info = paramInfo(param);
    if (info.boolean)
        return steps > 0 ? 1.0f : 0.0f;
    if (lists.count > 0) {
        uint32_t at = 0;
        for (uint32_t i = 1; i < lists.count; ++i)
            if (std::fabs(lists.values[i] - current) < std::fabs(lists.values[at] - current))
                at = i;
        const int j = std::max(0, std::min((int)lists.count - 1, (int)at + steps));
        return (float)lists.values[j];
    }
    if (info.integer)
        return std::max(info.min, std::min(info.max, std::round(current) + (float)steps));
    return fromNormalized(param, std::max(0.0f, std::min(1.0f, toNormalized(param, current) + (float)steps / 127.0f)));
}

} // namespace pe
