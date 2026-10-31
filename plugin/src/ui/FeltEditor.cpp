#include "FeltEditor.hpp"

#include <algorithm>
#include <cmath>

namespace pe {

namespace {
constexpr int kLo = 21, kHi = 108;
/// per quantity: the range's default top (the stiffness's half span) and the margin over the highest value
constexpr float kDefaultTop[4] = { 12.0f, 4.0f, 12.0f, 3.0f };
constexpr float kMargin[4] = { 3.0f, 0.5f, 2.0f, 0.5f };
} // namespace

FeltEditor::FeltEditor() { refit(); }

void FeltEditor::set(const PeFelt& f)
{
    fFelt = f;
    play();
    refit();
}

std::string FeltEditor::text() const
{
    char buf[512];
    const size_t n = pe_felt_format(&fFelt, buf, sizeof(buf));
    if (n < sizeof(buf))
        return std::string(buf, n);
    std::string big(n + 1, '\0'); // 32 points with long bends pass 512 bytes
    pe_felt_format(&fFelt, big.data(), big.size());
    big.resize(n);
    return big;
}

void FeltEditor::setShipped(const PeFeltKeys& k)
{
    fShipped = k;
    play();
    refit();
}

bool FeltEditor::applies() const
{
    return std::any_of(std::begin(fShipped.hammer), std::end(fShipped.hammer), [](uint8_t h) { return h != 0; });
}

void FeltEditor::play() { pe_felt_apply(&fShipped, &fFelt, &fPlayed); }

void FeltEditor::settle(uint32_t index)
{
    PeFeltCurve& c = fFelt.curve[fSelected];
    const int i = c.key[index] - kLo;
    if (fShipped.hammer[i] == 0)
        return;
    // the bend that plays at the point's own key (pe_felt_apply clamps to the felt's limits)
    float bend = c.bend[index];
    switch (fSelected) {
    case 1: bend = fPlayed.p[i] - fShipped.p[i]; break;
    case 2: bend = fPlayed.core05[i] / std::pow(10.0f, fFelt.hardness_db / 20.0f) - fShipped.core05[i]; break;
    case 3: bend = fPlayed.core_depth_mm[i] - fShipped.core_depth_mm[i]; break;
    default: bend = shown(0, c.key[index], true) - fFelt.hardness_db;
    }
    if (bend != c.bend[index]) {
        c.bend[index] = bend;
        play();
    }
}

float FeltEditor::shippedValue(uint32_t q, int key) const
{
    const int i = std::max(kLo, std::min(kHi, key)) - kLo;
    switch (q) {
    case 1: return fShipped.p[i];
    case 2: return fShipped.core05[i];
    case 3: return fShipped.core_depth_mm[i];
    default: return 0.0f; // the stiffness: dB re shipped
    }
}

float FeltEditor::shown(uint32_t q, int key, bool user) const
{
    if (!user)
        return shippedValue(q, key);
    const int i = std::max(kLo, std::min(kHi, key)) - kLo;
    switch (q) {
    case 1: return fPlayed.p[i];
    case 2: return fPlayed.core05[i];
    case 3: return fPlayed.core_depth_mm[i];
    default:
        return fShipped.f05[i] > 0 && fPlayed.f05[i] > 0 ? 20.0f * std::log10(fPlayed.f05[i] / fShipped.f05[i]) : 0.0f;
    }
}

void FeltEditor::range(uint32_t q, float& lo, float& hi) const
{
    q = q < 4 ? q : 0;
    lo = fLo[q];
    hi = fHi[q];
}

bool FeltEditor::refit()
{
    bool changed = false;
    for (uint32_t q = 0; q < 4; ++q) {
        float top = 0;
        for (int key = kLo; key <= kHi; ++key) {
            if (fShipped.hammer[key - kLo] == 0)
                continue;
            const float a = shown(q, key, false), b = shown(q, key, true);
            top = q == 0 ? std::max({ top, std::fabs(a), std::fabs(b) }) : std::max({ top, a, b });
        }
        const float hi = std::max(kDefaultTop[q], top > 0 ? top + kMargin[q] : 0.0f);
        const float lo = q == 0 ? -hi : 0.0f;
        changed = changed || hi != fHi[q] || lo != fLo[q];
        fLo[q] = lo;
        fHi[q] = hi;
    }
    return changed;
}

bool FeltEditor::dropped() { return refit(); }

void FeltEditor::reset()
{
    fFelt = PeFelt {};
    play();
    refit();
}

void FeltEditor::toGraph(const Rect& g, uint32_t q, float key, float value, float& x, float& y) const
{
    float lo, hi;
    range(q, lo, hi);
    x = g.x + (key - kLo) / (kHi - kLo) * g.w;
    y = g.y + g.h - (value - lo) / std::max(hi - lo, 1e-6f) * g.h;
}

void FeltEditor::fromGraph(const Rect& g, uint32_t q, float x, float y, float& key, float& value) const
{
    float lo, hi;
    range(q, lo, hi);
    key = kLo + (x - g.x) / std::max(g.w, 1.0f) * (kHi - kLo);
    value = lo + (g.y + g.h - y) / std::max(g.h, 1.0f) * (hi - lo);
}

int FeltEditor::pointAt(const Rect& g, float x, float y) const
{
    const PeFeltCurve& c = fFelt.curve[fSelected];
    int best = -1;
    float bestD = kHitRadius * kHitRadius;
    for (uint32_t i = 0; i < c.n; ++i) {
        float px, py;
        toGraph(g, fSelected, c.key[i], shown(fSelected, c.key[i], true), px, py);
        const float d = (px - x) * (px - x) + (py - y) * (py - y);
        if (d <= bestD) {
            bestD = d;
            best = (int)i;
        }
    }
    return best;
}

int FeltEditor::add(const Rect& g, float x, float y)
{
    PeFeltCurve& c = fFelt.curve[fSelected];
    if (c.n >= (uint32_t)kMaxPoints)
        return -1;
    float fk, value;
    fromGraph(g, fSelected, x, y, fk, value);
    const int key = std::max(kLo, std::min(kHi, (int)std::lround(fk)));
    uint32_t k = 0; // the new point goes before point k
    while (k < c.n && c.key[k] < key)
        ++k;
    if (k < c.n && c.key[k] == key)
        return -1;
    for (uint32_t i = c.n; i > k; --i) {
        c.key[i] = c.key[i - 1];
        c.bend[i] = c.bend[i - 1];
    }
    c.key[k] = (uint8_t)key;
    c.bend[k] = value - shippedValue(fSelected, key);
    c.n += 1;
    play();
    settle(k);
    return (int)k;
}

void FeltEditor::move(int index, const Rect& g, float x, float y)
{
    PeFeltCurve& c = fFelt.curve[fSelected];
    if (index < 0 || (uint32_t)index >= c.n)
        return;
    const uint32_t i = (uint32_t)index;
    float fk, value;
    fromGraph(g, fSelected, x, y, fk, value);
    const int lo = i == 0 ? kLo : c.key[i - 1] + 1;
    const int hi = i + 1 == c.n ? kHi : c.key[i + 1] - 1;
    const int key = std::max(lo, std::min(hi, (int)std::lround(fk)));
    c.key[i] = (uint8_t)key;
    c.bend[i] = value - shippedValue(fSelected, key);
    play();
    settle(i);
}

bool FeltEditor::remove(int index)
{
    PeFeltCurve& c = fFelt.curve[fSelected];
    if (index < 0 || (uint32_t)index >= c.n)
        return false;
    for (uint32_t i = (uint32_t)index; i + 1 < c.n; ++i) {
        c.key[i] = c.key[i + 1];
        c.bend[i] = c.bend[i + 1];
    }
    c.n -= 1;
    play();
    return true;
}

} // namespace pe
