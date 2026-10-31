#include "Morph.hpp"

#include <algorithm>
#include <cmath>

namespace pe {

namespace {

float cross(Point2 o, Point2 a, Point2 b)
{
    return (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
}

/// the nearest point to p on the segment a–b, and where (0 at a … 1 at b)
Point2 onSegment(Point2 p, Point2 a, Point2 b, float* at = nullptr)
{
    const float dx = b.x - a.x, dy = b.y - a.y, len2 = dx * dx + dy * dy;
    const float t = len2 > 0 ? std::clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / len2, 0.0f, 1.0f) : 0.0f;
    if (at != nullptr)
        *at = t;
    return { a.x + t * dx, a.y + t * dy };
}

float dist2(Point2 a, Point2 b)
{
    return (a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y);
}

std::string trim(const std::string& s)
{
    const size_t a = s.find_first_not_of(" \t"), b = s.find_last_not_of(" \t");
    return a == std::string::npos ? std::string() : s.substr(a, b - a + 1);
}

} // namespace

Point2 morphCorner(uint32_t n, uint32_t i)
{
    if (n == 2)
        return { 0.0f, i == 0 ? 1.0f : -1.0f };
    const double pi = 3.14159265358979323846; // M_PI is POSIX, not C++
    const double a = pi / 2 - 2 * pi * (double)i / (double)std::max(n, 1u);
    return { (float)std::cos(a), (float)std::sin(a) };
}

Point2 morphClamp(uint32_t n, float x, float y)
{
    if (!std::isfinite(x) || !std::isfinite(y)) // no number (a host's): the main instrument's corner
        return morphCorner(std::max(n, 1u), 0);
    const Point2 p { x, y };
    if (n <= 1)
        return p;
    if (n == 2)
        return onSegment(p, morphCorner(2, 0), morphCorner(2, 1));
    bool inside = true; // clockwise: the inside lies right of every edge
    for (uint32_t i = 0; i < n; ++i)
        inside = inside && cross(morphCorner(n, i), morphCorner(n, (i + 1) % n), p) <= 0;
    if (inside)
        return p;
    Point2 best = morphCorner(n, 0);
    for (uint32_t i = 0; i < n; ++i) {
        const Point2 q = onSegment(p, morphCorner(n, i), morphCorner(n, (i + 1) % n));
        if (dist2(p, q) < dist2(p, best))
            best = q;
    }
    return best;
}

std::vector<float> morphWeights(uint32_t n, float x, float y)
{
    if (n <= 1)
        return { 1.0f };
    const Point2 p = morphClamp(n, x, y);
    std::vector<double> w(n, 0.0);
    if (n == 2) {
        w[1] = std::clamp((1.0 - p.y) / 2.0, 0.0, 1.0);
        w[0] = 1.0 - w[1];
    } else {
        // on an edge: only its two corners, linearly (Wachspress divides by zero there)
        bool edge = false;
        for (uint32_t i = 0; i < n && !edge; ++i) {
            const Point2 a = morphCorner(n, i), b = morphCorner(n, (i + 1) % n);
            float t = 0;
            if (dist2(p, onSegment(p, a, b, &t)) < 1e-12f) {
                w[i] = 1.0 - t;
                w[(i + 1) % n] += t;
                edge = true;
            }
        }
        if (!edge) {
            double sum = 0;
            for (uint32_t i = 0; i < n; ++i) {
                const Point2 prev = morphCorner(n, (i + n - 1) % n), v = morphCorner(n, i), next = morphCorner(n, (i + 1) % n);
                const double c = std::fabs(cross(prev, v, next));
                const double a0 = std::fabs(cross(p, prev, v)), a1 = std::fabs(cross(p, v, next));
                w[i] = c / (a0 * a1);
                sum += w[i];
            }
            for (double& v : w)
                v /= sum;
        }
    }
    std::vector<float> out(n);
    for (uint32_t i = 0; i < n; ++i)
        out[i] = (float)(std::round(w[i] * 100.0) / 100.0);
    return out;
}

Point2 padToPlane(const Rect& pad, float x, float y)
{
    const float r = std::max(1.0f, pad.w / 2 - kPadMargin);
    return { (x - pad.x - pad.w / 2) / r, -(y - pad.y - pad.h / 2) / r };
}

Point2 planeToPad(const Rect& pad, Point2 p)
{
    const float r = std::max(1.0f, pad.w / 2 - kPadMargin);
    return { pad.x + pad.w / 2 + p.x * r, pad.y + pad.h / 2 - p.y * r };
}

std::vector<std::string> shownPartners(const std::string& stateText, const Catalog& catalog, const std::string& mainId)
{
    return validPartners(parsePartners(stateText), catalog, mainId);
}

Point2 morphPointAfter(size_t before, size_t after, Point2 current)
{
    if (after == 0 || (before == 0 && after > 0))
        return { 0.0f, 1.0f };
    if (after < before)
        return { 0.0f, 0.0f };
    return current;
}

std::string morphCornerLabel(const std::string& name)
{
    std::vector<size_t> starts; // where each character begins (UTF-8: not on a continuation byte)
    for (size_t i = 0; i < name.size(); ++i)
        if (((unsigned char)name[i] & 0xC0) != 0x80)
            starts.push_back(i);
    return starts.size() <= 14 ? name : name.substr(0, starts[13]) + "…";
}

std::string morphLabel(const std::string& name, uint32_t partnersPlaying)
{
    return partnersPlaying == 0 ? name : name + " + " + std::to_string(partnersPlaying) + " (morph)";
}

std::vector<std::string> parsePartners(const std::string& text)
{
    std::vector<std::string> out;
    size_t from = 0;
    while (from <= text.size()) {
        const size_t comma = std::min(text.find(',', from), text.size());
        const std::string id = trim(text.substr(from, comma - from));
        if (!id.empty() && std::find(out.begin(), out.end(), id) == out.end())
            out.push_back(id);
        from = comma + 1;
    }
    return out;
}

std::string formatPartners(const std::vector<std::string>& ids)
{
    std::string out;
    for (const std::string& id : ids)
        out += (out.empty() ? "" : ",") + id;
    return out;
}

std::vector<std::string> validPartners(const std::vector<std::string>& ids, const Catalog& catalog, const std::string& mainId)
{
    std::vector<std::string> out;
    const InstrumentEntry* main = catalog.find(mainId);
    if (main == nullptr || main->info.morph_family == 0)
        return out;
    for (const std::string& id : ids) {
        const InstrumentEntry* e = catalog.find(id);
        if (e == nullptr || id == mainId || e->info.morph_family != main->info.morph_family
            || std::find(out.begin(), out.end(), id) != out.end())
            continue;
        if (out.size() + 1 >= kMorphMax)
            break;
        out.push_back(id);
    }
    return out;
}

} // namespace pe
