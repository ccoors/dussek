#include "NoteHistory.hpp"

#include <algorithm>

namespace pe {

void NoteHistory::add(uint8_t velocity, double t)
{
    if (velocity == 0)
        return;
    fNotes.push_back({ std::min<uint8_t>(velocity, 127), t });
    while (!fNotes.empty() && t - fNotes.front().second > kBarWindowS)
        fNotes.pop_front();
}

std::vector<std::pair<uint8_t, float>> NoteHistory::dots(double t) const
{
    std::vector<std::pair<uint8_t, float>> out;
    for (const auto& n : fNotes) {
        const double age = t - n.second;
        if (age >= 0 && age < kDotFadeS)
            out.push_back({ n.first, (float)(1.0 - age / kDotFadeS) });
    }
    return out;
}

std::array<float, NoteHistory::kBins> NoteHistory::bars(double t) const
{
    std::array<float, kBins> out {};
    for (const auto& n : fNotes)
        if (t - n.second <= kBarWindowS)
            out[std::min(kBins - 1, (n.first - 1) * kBins / 127)] += 1.0f;
    const float top = *std::max_element(out.begin(), out.end());
    if (top > 0)
        for (float& v : out)
            v /= top;
    return out;
}

} // namespace pe
