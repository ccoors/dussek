#include "Format.hpp"
#include "EngineValues.hpp"
#include "Params.hpp"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace pe {
namespace {

std::string lower(std::string s)
{
    for (char& c : s)
        c = (char)std::tolower((unsigned char)c);
    return s;
}

std::string trim(const std::string& s)
{
    const size_t a = s.find_first_not_of(" \t");
    if (a == std::string::npos)
        return std::string();
    return s.substr(a, s.find_last_not_of(" \t") - a + 1);
}

std::string printf1(const char* fmt, double v)
{
    char buf[64];
    std::snprintf(buf, sizeof buf, fmt, v);
    return buf;
}

bool isLevel(const ParamInfo& p) { return p.unit && std::strcmp(p.unit, "dB") == 0 && p.min == kOffDb; }

float clampRound(const ParamInfo& p, float v)
{
    v = std::min(p.max, std::max(p.min, v));
    return p.integer ? std::round(v) : v;
}

} // namespace

std::string formatValue(uint32_t param, float value)
{
    const ParamInfo& p = paramInfo(param);
    if (p.boolean)
        return value >= 0.5f ? "On" : "Off";
    if (paramLabelCount(param) > 0) {
        const char* l = paramLabel(param, (uint32_t)std::max(0L, std::lround(value - p.min)));
        return l ? l : "?";
    }
    if (param == kStretch)
        return value == 0.0f ? "As recorded" : printf1("%+.2f", value);
    if (isLevel(p) && value <= kOffDb)
        return "Off";
    if (param == kLid)
        return value <= 0.0f ? "Closed" : value >= 100.0f ? "Open" : printf1("%.0f %%", value);
    if (param == kPillow)
        return value <= 0.0f ? "Off" : printf1("%.0f %%", value);
    if (param == kCondition)
        return printf1("%.0f %%", value) + " · " + conditionZone(std::round(value));
    const char* unit = p.unit ? p.unit : "";
    if (std::strcmp(unit, "dB") == 0)
        return p.min >= 0 ? printf1("%.1f dB", value) // a range (the Dynamics), not a gain
            : value == 0.0f ? "0.0 dB" : printf1("%+.1f dB", value);
    if (std::strcmp(unit, "%") == 0)
        return p.min < 0 && value > 0 ? printf1("+%.0f %%", value) : printf1("%.0f %%", value);
    if (std::strcmp(unit, "m") == 0)
        return param == kMicDistance && value <= 0.0f ? "Default" : printf1("%.2f m", value); // only the distance has a default; coordinates may be 0 or negative
    if (std::strcmp(unit, "cm") == 0)
        return printf1("%.0f cm", value);
    if (std::strcmp(unit, "Hz") == 0)
        return printf1("%.1f Hz", value);
    if (std::strcmp(unit, "ms") == 0)
        return printf1("%.0f ms", value);
    if (param == kAir) // the MIDI scale it is played on (an expression pedal's, CC 11 by default)
        return printf1("%.0f", value);
    if (p.integer)
        return printf1("%.0f", value);
    return printf1("%.2f", value);
}

bool parseValue(uint32_t param, const std::string& text, float& out)
{
    const ParamInfo& p = paramInfo(param);
    const std::string t = lower(trim(text));
    if (t.empty())
        return false;
    if (p.boolean && (t == "on" || t == "off")) {
        out = t == "on" ? 1.0f : 0.0f;
        return true;
    }
    if (isLevel(p) && t == "off") {
        out = kOffDb;
        return true;
    }
    for (uint32_t i = 0, n = paramLabelCount(param); i < n; ++i)
        if (lower(paramLabel(param, i)) == t) {
            out = p.min + (float)i;
            return true;
        }
    if (param == kStretch && t == "as recorded") {
        out = 0.0f;
        return true;
    }
    if ((param == kLid && t == "closed") || (param == kPillow && t == "off")) {
        out = 0.0f;
        return true;
    }
    if (param == kLid && t == "open") {
        out = 100.0f;
        return true;
    }
    if (param == kMicDistance && t == "default") {
        out = 0.0f;
        return true;
    }
    if (param == kCondition && conditionZoneValue(t, out))
        return true;
    char* end = nullptr;
    const double v = std::strtod(t.c_str(), &end);
    if (end == t.c_str() || !std::isfinite(v) || t.compare(0, 3, "nan") == 0 || t.compare(0, 3, "inf") == 0)
        return false;
    // after the number only the parameter's own unit
    std::string rest = trim(end);
    if (param == kCondition) { // the shown text: "10 % · Normal"
        const size_t dot = rest.find("·");
        if (dot != std::string::npos)
            rest = trim(rest.substr(0, dot));
    }
    if (!rest.empty() && (p.unit == nullptr || rest != lower(p.unit)))
        return false;
    out = clampRound(p, (float)std::max(-1e30, std::min(1e30, v)));
    return true;
}

std::string formatMicDistance(float value, bool hasPair, double effective)
{
    if (!hasPair)
        return formatValue(kMicDistance, value);
    if (!std::isfinite(effective)) // the free pair has no single distance: its coordinates say where it is
        return "Custom";
    const std::string m = printf1("%.2f m", effective);
    if (value <= 0.0f)
        return m + " (default)";
    return std::fabs(effective - value) > 0.005 ? m + " (limited)" : m;
}

std::string formatMicSpacing(float value, bool hasPair, double effective)
{
    if (!hasPair || !std::isfinite(effective))
        return formatValue(kMicSpacing, value);
    const float cm = (float)(effective * 100.0);
    const std::string s = formatValue(kMicSpacing, cm);
    return std::fabs(cm - value) > 0.5f ? s + " (limited)" : s;
}

float toNormalized(uint32_t param, float value)
{
    const ParamInfo& p = paramInfo(param);
    return p.max > p.min ? std::min(1.0f, std::max(0.0f, (value - p.min) / (p.max - p.min))) : 0.0f;
}

float fromNormalized(uint32_t param, float norm)
{
    const ParamInfo& p = paramInfo(param);
    return clampRound(p, p.min + std::min(1.0f, std::max(0.0f, norm)) * (p.max - p.min));
}


} // namespace pe
