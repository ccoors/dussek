// The morph's pure logic (tested in ui_tests): the pad's weights from its point (generalized
// barycentric coordinates over a regular polygon of the list's instruments), the partner list
// (the "morph" state) and which instruments may join it.
#pragma once
#include "Catalog.hpp"
#include "Layout.hpp"
#include "MicMap.hpp"

#include <cstdint>
#include <string>
#include <vector>

namespace pe {

/// the most instruments in a morph (the main one and four partners)
constexpr uint32_t kMorphMax = 5;

/// corner `i` of `n` (the list's order): on the unit circle, corner 0 (the main instrument) at the
/// top (0, 1), the others clockwise; n = 2: (0, 1) and (0, -1)
Point2 morphCorner(uint32_t n, uint32_t i);
/// (x, y) moved into the polygon of `n` corners (onto the segment for n = 2; n = 1: unchanged)
Point2 morphClamp(uint32_t n, float x, float y);
/// each corner's weight for the point (x, y) (clamped first), in list order, rounded to 0.01:
/// Wachspress coordinates (a corner is 1 for it, an edge mixes only its two corners linearly,
/// inside all positive); n = 1: {1}
std::vector<float> morphWeights(uint32_t n, float x, float y);

/// the "morph" state's partner ids (comma-separated): trimmed, empties and repeats dropped
std::vector<std::string> parsePartners(const std::string& text);
std::string formatPartners(const std::vector<std::string>& ids);
/// the partners that may join `mainId`: known to the catalog, of the main instrument's (nonzero)
/// morph family, not the main itself, at most kMorphMax - 1, in order
std::vector<std::string> validPartners(const std::vector<std::string>& ids, const Catalog& catalog, const std::string& mainId);

/// the pad's margin inside its square (the corners' labels)
constexpr float kPadMargin = 30;
/// a point in the pad's square ↔ the polygon's plane (the unit circle around its middle, y up)
Point2 padToPlane(const Rect& pad, float x, float y);
Point2 planeToPad(const Rect& pad, Point2 p);

/// a corner's label on the pad: the name, cut to 13 characters and "…" beyond 14 (characters, not bytes)
std::string morphCornerLabel(const std::string& name);

/// the Instrument row's text: the instrument's name, and "+ n (morph)" with n partners playing
std::string morphLabel(const std::string& name, uint32_t partnersPlaying);

/// the partners the "morph" state's text names that join `mainId` (what the DSP plays: the UI shows,
/// weighs and counts these, whatever order the host restored the states in)
std::vector<std::string> shownPartners(const std::string& stateText, const Catalog& catalog, const std::string& mainId);
/// where the pad's point goes when the list's partners go from `before` to `after`: the first one
/// joins on the main instrument's corner (silently), others where the point is; a removal puts it in
/// the middle, the last one's on the main instrument's corner; a reordering leaves it
Point2 morphPointAfter(size_t before, size_t after, Point2 current);

/// what a build was for, beyond its point and tuning: the main instrument, its partners, the rate
struct MorphKey {
    std::string main;
    std::vector<std::string> partners;
    double rate = 0;
    bool operator==(const MorphKey& o) const { return main == o.main && partners == o.partners && rate == o.rate; }
};

/// whether a build superseded while it ran is still worth playing: the same instruments at the same
/// rate (only the point or the tuning moved on), so a drag is heard as it goes, one engine per build
inline bool offerSuperseded(const MorphKey& built, const MorphKey& now) { return built == now; }

} // namespace pe
