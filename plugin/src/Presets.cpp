#include "Presets.hpp"
#include "Numbers.hpp"
#include "Paths.hpp"

#include "Params.hpp"
#include "pianoengine.h"

#include <algorithm>
#include <cctype>
#include <cerrno>
#include <charconv>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dirent.h>
#include <fstream>
#include <sstream>
#include <sys/stat.h>

namespace pe {

namespace {

const char* const kStates[] = { "instrument", "velocity_curve", "morph", "felt", "scale_file" };

constexpr uint32_t kGrand = 0, kUpright = 1;

/// a Custom pair: position (m), pattern (%: 0 omni, 50 cardioid, 100 figure-8), aim (yaw: 0 to the
/// tail, 180 to the player; °), tilt (°), per microphone; the upright's wall (m; < 0: not set)
Preset custom(const char* name, uint32_t body, const float l[6], const float r[6], float wall = -1)
{
    Preset p;
    p.name = name;
    p.scope = PresetScope::Mics;
    p.body = body;
    p.shipped = true;
    p.values = { { "mic_position", 5.0f },
                 { "mic_l_x", l[0] }, { "mic_l_y", l[1] }, { "mic_l_z", l[2] },
                 { "mic_l_pattern", l[3] }, { "mic_l_yaw", l[4] }, { "mic_l_pitch", l[5] },
                 { "mic_r_x", r[0] }, { "mic_r_y", r[1] }, { "mic_r_z", r[2] },
                 { "mic_r_pattern", r[3] }, { "mic_r_yaw", r[4] }, { "mic_r_pitch", r[5] } };
    if (wall >= 0)
        p.values.push_back({ "mic_wall", wall });
    return p;
}

int symbolIndex(const std::string& symbol)
{
    for (uint32_t i = 0; i < kParamCount; ++i)
        if (symbol == paramInfo(i).symbol)
            return (int)i;
    return -1;
}

std::string trim(const std::string& s)
{
    const size_t a = s.find_first_not_of(" \t\r"), b = s.find_last_not_of(" \t\r");
    return a == std::string::npos ? std::string() : s.substr(a, b - a + 1);
}

std::string num(float v)
{
    char buf[32];
    const std::to_chars_result r = std::to_chars(buf, buf + sizeof buf, v);
    return std::string(buf, r.ptr);
}

/// a state's value on one line: backslash, newline and carriage return escaped
std::string escape(const std::string& s)
{
    std::string o;
    for (char c : s) {
        if (c == '\\') o += "\\\\";
        else if (c == '\n') o += "\\n";
        else if (c == '\r') o += "\\r";
        else o += c;
    }
    return o;
}

std::string unescape(const std::string& s)
{
    std::string o;
    for (size_t i = 0; i < s.size(); ++i) {
        if (s[i] == '\\' && i + 1 < s.size()) {
            const char c = s[++i];
            o += c == 'n' ? '\n' : c == 'r' ? '\r' : c;
        } else {
            o += s[i];
        }
    }
    return o;
}

bool lessByName(const Preset* a, const Preset* b) { return a->name < b->name; }

} // namespace

bool micScoped(uint32_t param)
{
    switch (param) {
    case kMicPosition: case kMicDistance: case kMicSpacing: case kDistanceLevel: case kMicWall:
    case kMicLX: case kMicLY: case kMicLZ: case kMicRX: case kMicRY: case kMicRZ:
    case kMicLPattern: case kMicLYaw: case kMicLPitch: case kMicRPattern: case kMicRYaw: case kMicRPitch:
        return true;
    default:
        return false;
    }
}

bool presetState(const std::string& key)
{
    return std::any_of(std::begin(kStates), std::end(kStates), [&](const char* s) { return key == s; });
}

const char* bodyName(uint32_t body)
{
    const char* n = pe_body_name(body);
    return n != nullptr ? n : pe_body_name(0);
}

bool bodyIndex(const std::string& name, uint32_t& body)
{
    for (uint32_t i = 0; const char* n = pe_body_name(i); ++i)
        if (name == n) {
            body = i;
            return true;
        }
    return false;
}

const std::vector<Preset>& shippedPresets()
{
    // the grand's coordinates: origin the soundboard's center, x to the treble, y to the tail, z up (the
    // player's ears at about (0, −1.25, 0.45)); the upright's: the board in the x–z plane, the player at −y
    static const std::vector<Preset> p = [] {
        std::vector<Preset> v;
        // two cardioids 17 cm apart, 110° between them, at the player's head, tilted to the board
        const float ortfL[6] = { -0.085f, -1.25f, 0.45f, 50, -55, -19 }, ortfR[6] = { 0.085f, -1.25f, 0.45f, 50, 55, -19 };
        v.push_back(custom("ORTF at the player's seat", kGrand, ortfL, ortfR));
        // one cardioid over the rear of the board, aimed down towards the hammers: both at one point (mono)
        const float jazz[6] = { -0.10f, 0.50f, 0.40f, 50, 180, -55 };
        v.push_back(custom("Jazz: one mic near the tail", kGrand, jazz, jazz));
        // two omnis 60 cm apart, 30 cm over the strings, under the open lid
        const float overL[6] = { -0.30f, -0.25f, 0.30f, 0, 0, -90 }, overR[6] = { 0.30f, -0.25f, 0.30f, 0, 0, -90 };
        v.push_back(custom("Spaced pair over the strings", kGrand, overL, overR));
        // two omnis 60 cm apart at 4 m on the curved side, at a standing listener's height
        const float hallL[6] = { 4.0f, -0.55f, 0.90f, 0, -90, -10 }, hallR[6] = { 4.0f, 0.05f, 0.90f, 0, -90, -10 };
        v.push_back(custom("Concert pair at 4 m", kGrand, hallL, hallR));
        // the upright: two cardioids 50 cm apart over the open top, aimed down
        const float topL[6] = { -0.25f, 0.05f, 1.00f, 50, 0, -85 }, topR[6] = { 0.25f, 0.05f, 1.00f, 50, 0, -85 };
        v.push_back(custom("Pair over the open top", kUpright, topL, topR));
        // two omnis 60 cm apart behind the board, the piano 1.2 m from the wall
        const float backL[6] = { -0.30f, 0.45f, 0.00f, 0, 180, 0 }, backR[6] = { 0.30f, 0.45f, 0.00f, 0, 180, 0 };
        v.push_back(custom("Pair behind the board", kUpright, backL, backR, 1.2f));
        const float uOrtfL[6] = { -0.085f, -1.00f, 0.45f, 50, -55, -24 }, uOrtfR[6] = { 0.085f, -1.00f, 0.45f, 50, 55, -24 };
        v.push_back(custom("ORTF at the player's seat", kUpright, uOrtfL, uOrtfR));
        // the full presets: Default (every parameter and state at its default, the instrument the menu's
        // first), then the repository's plugin/presets/*.pepreset, embedded by the build
        Preset d;
        d.name = "Default";
        d.scope = PresetScope::All;
        d.shipped = true;
        v.push_back(d);
        static const char* const texts[] = {
#include "ShippedPresets.inc"
            nullptr };
        for (const char* const* t = texts; *t != nullptr; ++t) {
            Preset f;
            if (parsePreset(*t, f)) {
                f.shipped = true;
                v.push_back(std::move(f));
            }
        }
        return v;
    }();
    return p;
}

std::string formatPreset(const Preset& p)
{
    std::string s = "# PianoEngine preset\nname = " + p.name + "\nscope = " + (p.scope == PresetScope::All ? "all" : "mics") + "\n";
    if (p.scope == PresetScope::Mics)
        s += std::string("body = ") + bodyName(p.body) + "\n";
    for (const auto& [k, v] : p.values)
        s += "param." + k + " = " + num(v) + "\n";
    for (const auto& [k, v] : p.states)
        s += "state." + k + " = " + escape(v) + "\n";
    return s;
}

bool parsePreset(const std::string& text, Preset& out)
{
    Preset p;
    bool scope = false, body = false;
    std::istringstream in(text);
    std::string line;
    while (std::getline(in, line)) {
        if (line.empty() || line[0] == '#')
            continue;
        const size_t eq = line.find('=');
        if (eq == std::string::npos)
            continue;
        const std::string k = trim(line.substr(0, eq)), v = trim(line.substr(eq + 1));
        if (k == "name") {
            p.name = v;
        } else if (k == "scope") {
            scope = v == "mics" || v == "all";
            p.scope = v == "all" ? PresetScope::All : PresetScope::Mics;
        } else if (k == "body") {
            body = bodyIndex(v, p.body);
        } else if (k.rfind("param.", 0) == 0) {
            float x = 0;
            if (pe::parseFloat(v.data(), v.data() + v.size(), x) != nullptr && std::isfinite(x))
                p.values.push_back({ k.substr(6), x });
        } else if (k.rfind("state.", 0) == 0 && presetState(k.substr(6))) {
            p.states.push_back({ k.substr(6), unescape(v) });
        }
    }
    if (p.name.empty() || !scope || (p.scope == PresetScope::Mics && !body))
        return false;
    out = std::move(p);
    return true;
}

std::string presetDir()
{
    const std::string data = pe::dataDirs().data;
    return data.empty() ? data : data + "/presets";
}

std::vector<Preset> loadUserPresets(const std::string& dir)
{
    std::vector<Preset> out;
    DIR* d = dir.empty() ? nullptr : opendir(dir.c_str());
    if (d == nullptr)
        return out;
    while (const dirent* e = readdir(d)) {
        const std::string n = e->d_name;
        if (n.size() < 9 || n.compare(n.size() - 9, 9, ".pepreset") != 0)
            continue;
        std::ifstream f(dir + "/" + n);
        std::stringstream ss;
        ss << f.rdbuf();
        Preset p;
        if (f && parsePreset(ss.str(), p)) {
            p.file = dir + "/" + n;
            out.push_back(std::move(p));
        }
    }
    closedir(d);
    std::sort(out.begin(), out.end(), [](const Preset& a, const Preset& b) { return a.name < b.name; });
    return out;
}

std::string presetFile(const std::string& dir, const std::string& name)
{
    std::string safe;
    for (unsigned char c : name)
        safe += (std::isalnum(c) || c == '-' || c == '_' || c == ' ') ? (char)c : '_';
    safe = trim(safe);
    if (safe.empty())
        safe = "preset";
    return dir + "/" + safe + ".pepreset";
}

bool saveUserPreset(const std::string& dir, const Preset& p, std::string& error)
{
    error.clear();
    if (dir.empty()) {
        error = "no folder for presets (neither XDG_DATA_HOME nor HOME set)";
        return false;
    }
    // the folder and its parents (…/pianoengine/presets)
    for (size_t i = 1; i <= dir.size(); ++i)
        if (i == dir.size() || dir[i] == '/') {
            const std::string part = dir.substr(0, i);
            if (mkdir(part.c_str(), 0755) != 0 && errno != EEXIST) {
                error = "cannot make " + part + ": " + std::strerror(errno);
                return false;
            }
        }
    const std::string path = presetFile(dir, p.name), tmp = path + ".tmp";
    {
        std::ofstream f(tmp, std::ios::trunc);
        f << formatPreset(p);
        if (!f) {
            error = "cannot write " + tmp;
            return false;
        }
    }
    if (std::rename(tmp.c_str(), path.c_str()) != 0) {
        error = "cannot write " + path + ": " + std::strerror(errno);
        std::remove(tmp.c_str());
        return false;
    }
    return true;
}

bool deleteUserPreset(const Preset& p) { return !p.shipped && !p.file.empty() && std::remove(p.file.c_str()) == 0; }

std::vector<const Preset*> presetsFor(PresetScope scope, uint32_t body, const std::vector<Preset>& user)
{
    std::vector<const Preset*> shipped, mine;
    auto fits = [&](const Preset& p) { return p.scope == scope && (scope == PresetScope::All || p.body == body); };
    for (const Preset& p : shippedPresets())
        if (fits(p))
            shipped.push_back(&p);
    for (const Preset& p : user)
        if (fits(p))
            mine.push_back(&p);
    std::stable_sort(mine.begin(), mine.end(), lessByName);
    shipped.insert(shipped.end(), mine.begin(), mine.end());
    return shipped;
}

std::vector<std::pair<uint32_t, float>> presetParams(const Preset& p)
{
    std::vector<std::pair<uint32_t, float>> out;
    // a full preset: every parameter at its default, then the file's values (a shipped one lists only
    // what differs)
    if (p.scope == PresetScope::All)
        for (uint32_t i = 0; i < kParamCount; ++i)
            out.push_back({ i, paramInfo(i).def });
    for (const auto& [k, v] : p.values) {
        const int i = symbolIndex(k);
        // a microphone preset sets microphones only, whatever its file says
        if (i < 0 || (p.scope == PresetScope::Mics && !micScoped((uint32_t)i)))
            continue;
        const ParamInfo& d = paramInfo((uint32_t)i);
        const float x = std::clamp(v, d.min, d.max);
        if (p.scope == PresetScope::All)
            out[(size_t)i].second = x;
        else
            out.push_back({ (uint32_t)i, x });
    }
    return out;
}

std::vector<std::pair<std::string, std::string>> presetStates(const Preset& p)
{
    std::vector<std::pair<std::string, std::string>> out;
    if (p.scope != PresetScope::All)
        return out;
    for (const char* k : kStates) {
        // a state the file leaves out: its default (empty: the menu's first, straight, none, shipped, none)
        auto it = std::find_if(p.states.begin(), p.states.end(), [&](const auto& s) { return s.first == k; });
        out.push_back({ k, it != p.states.end() ? it->second : std::string() });
    }
    return out;
}

std::vector<const Preset*> shippedFullPresets()
{
    std::vector<const Preset*> out;
    for (const Preset& p : shippedPresets())
        if (p.scope == PresetScope::All)
            out.push_back(&p);
    return out;
}

int matchingPreset(const std::vector<const Preset*>& offered, const float* values)
{
    for (size_t k = 0; k < offered.size(); ++k) {
        const std::vector<std::pair<uint32_t, float>> ps = presetParams(*offered[k]);
        if (ps.empty())
            continue;
        const bool same = std::all_of(ps.begin(), ps.end(), [&](const std::pair<uint32_t, float>& q) {
            const ParamInfo& d = paramInfo(q.first);
            // within half the smallest step a control shows (an integer's 0.5; a float's 1/1000 of its range)
            const float tol = d.integer || d.boolean ? 0.5f : 1e-3f * (d.max - d.min);
            return std::fabs(values[q.first] - q.second) <= tol;
        });
        if (same)
            return (int)k;
    }
    return -1;
}

PresetMenu presetMenu(const std::vector<const Preset*>& offered, int current, bool deleteArmed)
{
    PresetMenu m;
    for (size_t i = 0; i < offered.size(); ++i) {
        m.items.push_back(offered[i]->name);
        m.actions.push_back((int)i);
    }
    m.items.push_back("Save as…");
    m.actions.push_back(kPresetSaveAs);
    if (current >= 0 && (size_t)current < offered.size() && !offered[(size_t)current]->shipped) {
        const std::string& n = offered[(size_t)current]->name;
        m.items.push_back(deleteArmed ? "Delete “" + n + "”: click again to confirm" : "Delete “" + n + "”…");
        m.actions.push_back(kPresetDelete);
    }
    return m;
}

std::string presetLabel(const std::vector<const Preset*>& offered, int current)
{
    return current >= 0 && (size_t)current < offered.size() ? offered[(size_t)current]->name : std::string("None");
}

Preset presetFrom(const std::string& name, PresetScope scope, uint32_t body, const float* values,
                  const std::vector<std::pair<std::string, std::string>>& states)
{
    Preset p;
    p.name = name;
    p.scope = scope;
    p.body = body;
    for (uint32_t i = 0; i < kParamCount; ++i)
        if (scope == PresetScope::All || micScoped(i))
            p.values.push_back({ paramInfo(i).symbol, values[i] });
    if (scope == PresetScope::All)
        for (const auto& [k, v] : states)
            if (presetState(k))
                p.states.push_back({ k, v });
    return p;
}

} // namespace pe
