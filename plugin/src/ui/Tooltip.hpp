// The tooltip's logic (pure; tested in ui_tests): when it shows, where it goes, what it says.
// Panel.hpp draws it, PianoUI feeds the timer.
#pragma once
#include "Layout.hpp"

#include <cstdint>
#include <string>

namespace pe {
constexpr uint64_t kTooltipDelayMs = 600;
constexpr float kTooltipW = 280;

/// The pointer rests on one row for kTooltipDelayMs: its tooltip shows. A press or a scroll
/// hides it until the pointer reaches another row.
class TooltipTimer {
public:
    /// the row under the pointer (-1: none) at `nowMs`; another row restarts the delay
    void hover(int row, uint64_t nowMs);
    /// a press or a scroll
    void dismiss();
    /// the row whose tooltip shows (-1: none); `busy`: a drag, a typed value or an open list
    int visible(uint64_t nowMs, bool busy) const;

private:
    int fRow = -1;
    uint64_t fSince = 0;
    bool fDismissed = false;
};

/// where a w × h tooltip goes for a row shown at `row` (window-logical) whose label ends at
/// `labelRight`: below the row from the label's end; above it where the view's bottom leaves no
/// room; moved left (and narrowed) to stay inside 0 … viewW, 0 … viewH
Rect tooltipRect(const Rect& row, float labelRight, float w, float h, float viewW, float viewH);
/// the parameter's full name (the instrument's own for the hammer's hardness: "Mallet hardness" …)
std::string tooltipTitle(uint32_t param, const InstrumentEntry& e);
/// the gestures a kind of control takes (`learnable`: MIDI learn's right-click too)
const char* tooltipGestures(Kind kind, bool learnable = false);
/// the Panic button's tooltip (it is no parameter)
extern const char* const kPanicTitle;
extern const char* const kPanicDescription;
/// the header menu's (its button right of Panic)
extern const char* const kMenuTitle;
extern const char* const kMenuDescription;
/// the Touch tab's velocity curve's
extern const char* const kCurveTitle;
/// the MIDI tab's program change switch and its Next / Previous instrument rows
extern const char* const kProgramChangeTitle;
extern const char* const kProgramChangeDescription;
extern const char* const kTakeoverTitle;
extern const char* const kTakeoverDescription;
extern const char* const kKnobDescription;
extern const char* const kStepDescription;
extern const char* const kCurveDescription;
extern const char* const kCurveGestures;
/// the Morph tab's plus button, its pad, its rows (the chosen instrument's, a partner's share) and a
/// partner's buttons
extern const char* const kMorphAddTitle;
extern const char* const kMorphAddDescription;
extern const char* const kMorphPadTitle;
extern const char* const kMorphPadDescription;
extern const char* const kMorphPadGestures;
extern const char* const kMorphMainDescription;
extern const char* const kMorphShareDescription;
extern const char* const kMorphUpDescription;
extern const char* const kMorphDownDescription;
extern const char* const kMorphRemoveDescription;
}
