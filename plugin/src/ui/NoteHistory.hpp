// The Touch tab's recent notes (pure; tested in ui_tests): the incoming velocities with the UI's
// time they arrived at, as dots on the curve that fade over kDotFadeS, and as bars of how many
// fell in each part of the velocity range over the last kBarWindowS (to find a cluster to spread).
#pragma once
#include <array>
#include <cstdint>
#include <deque>
#include <utility>
#include <vector>

namespace pe {

class NoteHistory {
public:
    static constexpr double kDotFadeS = 4.0;
    static constexpr double kBarWindowS = 30.0;
    static constexpr int kBins = 32;

    /// a note-on's incoming velocity (1–127) at time t (s, the UI's clock, not falling)
    void add(uint8_t velocity, double t);
    /// the notes younger than kDotFadeS at t, oldest first: (velocity, alpha 1 → 0)
    std::vector<std::pair<uint8_t, float>> dots(double t) const;
    /// kBins bins over velocity 1–127: the notes of the last kBarWindowS at t, the fullest bin 1
    std::array<float, kBins> bars(double t) const;
    /// forget every note (a double-click on the bars)
    void clear() { fNotes.clear(); }
    /// whether any dot is still fading at t (the view repaints while one is)
    bool fading(double t) const { return !fNotes.empty() && t - fNotes.back().second < kDotFadeS; }

private:
    std::deque<std::pair<uint8_t, double>> fNotes; // (velocity, time), oldest first
};

} // namespace pe
