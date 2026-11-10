#include "UiState.hpp"
#include "Layout.hpp"
#include "Params.hpp"
#include <algorithm>
#include "Numbers.hpp"
// std::to_chars for floating point: libstdc++ 11+, libc++ on macOS 13.3+
#include <charconv>
#include <sstream>
#include <fstream>
#include <filesystem>
#include <cmath>
#include <cstdio>
#include <cstring>

namespace pe {
const int kZoomSteps[8] = { 100, 125, 150, 175, 200, 250, 300, 400 };

namespace {
/// the number after `key` ("lo=" …) in `text`; locale independent (pe::parseFloat reads a point), the
/// old text format stays readable: an optional leading '+', exponent notation. False if there is no number.
bool readFloat(const char* text, const char* key, float& out)
{
    const char* p = std::strstr(text, key);
    if (p == nullptr) return false;
    p += std::strlen(key);
    if (*p == '+') ++p; // sscanf's %f took it, parseFloat does not
    float v = 0;
    if (parseFloat(p, p + std::strlen(p), v) == nullptr) return false;
    out = v;
    return true;
}
}

UiState parseUiState(const char* s)
{
    UiState st;
    if (s == nullptr)
        return st;
    // a state saved before the tabs has "open=…" (the sections) instead: its tab is Sound
    unsigned tab = 0;
    if (const char* t = std::strstr(s, "tab="); t != nullptr && std::sscanf(t, "tab=%u", &tab) == 1 && tab < kTabCount)
        st.tab = tab;
    int zoom = 0;
    if (const char* z = std::strstr(s, "zoom="); z != nullptr && std::sscanf(z, "zoom=%d", &zoom) == 1)
        for (int k : kZoomSteps)
            if (k == zoom)
                st.zoom = zoom;
    int diag = 0;
    if (const char* d = std::strstr(s, ";diag=")) {
        std::sscanf(d, ";diag=%d", &diag);
        st.diag = diag >= 0 && diag <= 5 ? diag : 0; // StripInteraction.hpp: 4 and 5 a panel without the strip
    }
    int view = 0;
    if (const char* v = std::strstr(s, ";view=")) {
        std::sscanf(v, ";view=%d", &view);
        st.view = view == 1 ? 1 : 0;
    }
    float lo = 0, hi = 0, fl = 0;
    if (readFloat(s, ";lo=", lo) && readFloat(s, ";hi=", hi) && std::isfinite(lo) && std::isfinite(hi) && lo >= 20.0f &&
        hi <= 24000.0f && hi / lo >= 4.0f) {
        st.lo = lo;
        st.hi = hi;
    }
    if (readFloat(s, ";floor=", fl) && std::isfinite(fl) && fl >= -120.0f && fl <= -20.0f)
        st.floorDb = fl;
    return st;
}

std::string formatUiState(const UiState& s)
{
    // the floats through std::to_chars: shortest text that reads back exactly, always with a point whatever the
    // numeric locale (a printf would write "27,5" under a German host). Same text as the old %g for these values.
    auto num = [](float v) {
        char buf[32];
        const std::to_chars_result r = std::to_chars(buf, buf + sizeof buf, v);
        return std::string(buf, r.ptr);
    };
    char head[96];
    std::snprintf(head, sizeof head, "tab=%u;zoom=%d;diag=%d;view=%d;", s.tab, s.zoom, s.diag, s.view); // integers only
    return std::string(head) + "lo=" + num(s.lo) + ";hi=" + num(s.hi) + ";floor=" + num(s.floorDb);
}

void minimumWindowSize(double hostScale, uint32_t& width, uint32_t& height)
{
    width = (uint32_t)(kMinWidth * hostScale);
    height = (uint32_t)(kMinHeight * hostScale);
}

void minimumWindowSize(double hostScale, double scale, float diagLogical, uint32_t& width, uint32_t& height)
{
    minimumWindowSize(hostScale, width, height);
    constexpr float kControlsViewport = 240.0f; // the drawing's rows and a few of the tab's (ui_tests)
    const uint32_t diagHeight = (uint32_t)std::lround(diagLogical * scale);
    const uint32_t controlsHeight = (uint32_t)std::lround(kControlsViewport * hostScale);
    height = std::max(height, diagHeight + controlsHeight);
}

bool WindowResizeTracker::pending(uint64_t nowMs) const
{
    return fCount > 0 && nowMs - fAt < kExpiryMs;
}

void WindowResizeTracker::requested(uint32_t height, uint64_t nowMs)
{
    if (!pending(nowMs)) fCount = 0; // an expired request is forgotten
    if (fCount == kMax) {
        for (int i = 1; i < kMax; ++i) fReq[i - 1] = fReq[i];
        --fCount;
    }
    fReq[fCount++] = height;
    fAt = nowMs;
}

void WindowResizeTracker::hostResized(uint32_t reportedHeight, uint64_t nowMs)
{
    if (!pending(nowMs)) { fCount = 0; return; }
    for (int i = 0; i < fCount; ++i)
        if (fReq[i] == reportedHeight) {
            for (int j = i + 1; j < fCount; ++j) fReq[j - i - 1] = fReq[j];
            fCount -= i + 1;
            return;
        }
    fCount = 0; // the host clamped a zoom request or the user resized the window
}

uint32_t WindowResizeTracker::zoomed(uint32_t actualHeight, double oldScale, double newScale, uint32_t minH, uint64_t nowMs)
{
    const uint32_t base = pending(nowMs) ? fReq[fCount - 1] : actualHeight;
    const uint32_t h = std::max<uint32_t>(minH, (uint32_t)std::lround(base / oldScale * newScale));
    requested(h, nowMs);
    return h;
}

std::vector<uint32_t> resetParams()
{
    std::vector<uint32_t> r;
    for (uint32_t i = 0; i < kParamCount; ++i)
        r.push_back(i);
    return r;
}

int parseSavedZoom(const std::string& text)
{
    int zoom = 0;
    if (const char* z = std::strstr(text.c_str(), "zoom="); z != nullptr && std::sscanf(z, "zoom=%d", &zoom) == 1)
        for (int k : kZoomSteps)
            if (k == zoom)
                return zoom;
    return 100;
}

int loadSavedZoom(const std::string& config)
{
    if (config.empty())
        return 100;
    std::ifstream f(config + "/window.txt");
    std::stringstream ss;
    ss << f.rdbuf();
    return parseSavedZoom(ss.str());
}

bool saveZoom(const std::string& config, int zoom)
{
    if (config.empty())
        return false;
    std::error_code ec;
    std::filesystem::create_directories(config, ec);
    std::ofstream f(config + "/window.txt");
    f << "zoom=" << zoom << "\n";
    return (bool)f;
}

UiState initialUiState(const std::string& config)
{
    UiState s;
    s.zoom = loadSavedZoom(config);
    return s;
}
}
