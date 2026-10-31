#include "Viz.hpp"

#include <algorithm>
#include <cmath>

namespace pe {

namespace {
constexpr float kPi = 3.14159265358979f;
}

float hammerHeight(const HammerTrack& h, uint64_t now, float rate)
{
    if (h.struckAt == 0 || !(rate > 0)) return 0.0f;
    if (now <= h.struckAt) return 1.0f;
    const float t = (float)(now - h.struckAt) / rate;
    // the rebound, in heights per second (a slower hammer falls back slower; the floor keeps a
    // ppp strike from hanging at the string)
    const float back = 0.5f * std::max(h.speed, 0.3f) / kVizBlowM;
    const auto rebound = [&](float s) { return std::min(1.0f, 1.0f - back * (s - kVizContactS)); };
    const float tu = h.upAt > h.struckAt ? (float)(h.upAt - h.struckAt) / rate : 1e30f;
    if (t <= tu) return std::max(rebound(t), kVizCheck); // held: the backcheck catches it
    // released on its way back: it goes on at its rebound speed; caught: it falls from the check
    const float y = rebound(tu) > kVizCheck ? rebound(t) : kVizCheck - kVizDrop * (t - tu);
    return std::clamp(y, 0.0f, 1.0f);
}

float stringPx(float length, float longest, float px)
{
    if (!(length > 0) || !(longest > 0)) return 0.0f;
    return px * std::pow(std::min(length / longest, 1.0f), 0.6f);
}

float shapeEnvelope(const float* amp, size_t n, float u)
{
    float sum = 0;
    for (size_t k = 0; k < n; ++k) {
        const float s = std::sin(kPi * (float)(k + 1) * u);
        sum += amp[k] * amp[k] * s * s;
    }
    return std::sqrt(sum);
}

float shapeSnapshot(const float* amp, const float* phase, size_t n, float u)
{
    float sum = 0;
    for (size_t k = 0; k < n; ++k) sum += amp[k] * std::cos(phase[k]) * std::sin(kPi * (float)(k + 1) * u);
    return sum;
}

float displacementPitches(float metres)
{
    return std::min(0.45f * std::sqrt(std::fabs(metres) / 1.2e-3f), kVizWidest);
}

void keyHue(int key, float rgb[3])
{
    static const float stops[5][3] = { { 1.0f, 0.35f, 0.16f }, { 1.0f, 0.75f, 0.24f }, { 0.47f, 0.9f, 0.55f },
                                       { 0.27f, 0.75f, 1.0f }, { 0.67f, 0.43f, 1.0f } };
    const float x = std::clamp((float)key / 87.0f, 0.0f, 1.0f) * 4.0f;
    const int i = std::min((int)x, 3);
    const float f = x - (float)i;
    for (int c = 0; c < 3; ++c) rgb[c] = stops[i][c] * (1 - f) + stops[i + 1][c] * f;
}

} // namespace pe
