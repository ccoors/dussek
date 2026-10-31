#include "CurveEdit.hpp"

#include <algorithm>
#include <cmath>
#include <vector>

namespace pe {

namespace {
int clampInt(float v, int lo, int hi) { return std::max(lo, std::min(hi, (int)std::lround(v))); }
} // namespace

CurveEdit::CurveEdit() { pe_velocity_curve_parse("", &fCurve); }

void CurveEdit::set(const PeVelocityCurve& c) { fCurve = c; }

void CurveEdit::toGraph(const Rect& g, float vin, float vout, float& x, float& y)
{
    x = g.x + (vin - 1) / 126 * g.w;
    y = g.y + g.h - (vout - 1) / 126 * g.h;
}

void CurveEdit::fromGraph(const Rect& g, float x, float y, float& vin, float& vout)
{
    vin = 1 + (x - g.x) / std::max(g.w, 1.0f) * 126;
    vout = 1 + (g.y + g.h - y) / std::max(g.h, 1.0f) * 126;
}

int CurveEdit::pointAt(const Rect& graph, float x, float y) const
{
    int best = -1;
    float bestD = kHitRadius * kHitRadius;
    for (uint32_t i = 0; i < fCurve.n; ++i) {
        float px, py;
        toGraph(graph, fCurve.vin[i], fCurve.vout[i], px, py);
        const float d = (px - x) * (px - x) + (py - y) * (py - y);
        if (d <= bestD) {
            bestD = d;
            best = (int)i;
        }
    }
    return best;
}

int CurveEdit::add(const Rect& graph, float x, float y)
{
    const uint32_t n = fCurve.n;
    if (n >= (uint32_t)kMaxPoints)
        return -1;
    float fin, fout;
    fromGraph(graph, x, y, fin, fout);
    const int vin = clampInt(fin, 1, 127);
    uint32_t k = 1; // the new point goes before point k
    while (k < n && fCurve.vin[k] < vin)
        ++k;
    if (vin <= fCurve.vin[k - 1] || vin >= fCurve.vin[k]) // on a neighbor's in
        return -1;
    const int vout = clampInt(fout, fCurve.vout[k - 1], fCurve.vout[k]);
    for (uint32_t i = n; i > k; --i) {
        fCurve.vin[i] = fCurve.vin[i - 1];
        fCurve.vout[i] = fCurve.vout[i - 1];
    }
    fCurve.vin[k] = (uint8_t)vin;
    fCurve.vout[k] = (uint8_t)vout;
    fCurve.n = n + 1;
    return (int)k;
}

void CurveEdit::move(int index, const Rect& graph, float x, float y)
{
    if (index < 0 || (uint32_t)index >= fCurve.n)
        return;
    const uint32_t i = (uint32_t)index, last = fCurve.n - 1;
    float fin, fout;
    fromGraph(graph, x, y, fin, fout);
    if (i != 0 && i != last)
        fCurve.vin[i] = (uint8_t)clampInt(fin, fCurve.vin[i - 1] + 1, fCurve.vin[i + 1] - 1);
    const int lo = i == 0 ? 1 : fCurve.vout[i - 1];
    const int hi = i == last ? 127 : fCurve.vout[i + 1];
    fCurve.vout[i] = (uint8_t)clampInt(fout, lo, hi);
}

bool CurveEdit::remove(int index)
{
    if (index <= 0 || (uint32_t)index >= fCurve.n - 1)
        return false;
    for (uint32_t i = (uint32_t)index; i + 1 < fCurve.n; ++i) {
        fCurve.vin[i] = fCurve.vin[i + 1];
        fCurve.vout[i] = fCurve.vout[i + 1];
    }
    --fCurve.n;
    fCurve.vin[fCurve.n] = fCurve.vout[fCurve.n] = 0;
    return true;
}

std::string CurveEdit::text() const
{
    std::vector<char> buf(64);
    size_t n = pe_velocity_curve_format(&fCurve, buf.data(), buf.size());
    if (n >= buf.size()) {
        buf.resize(n + 1);
        n = pe_velocity_curve_format(&fCurve, buf.data(), buf.size());
    }
    return std::string(buf.data(), n);
}

} // namespace pe
