// The pointer state machine of the diagnostics view (pure; tested in ui_tests with a fake host):
// where a press goes (panel, tabs, pedal lever, keys, the rest of the header and strip), the key
// drag (glissando via StripPlayer), the pedal drag with its edit-gesture bracket, and the rule
// that makes both safe against a lost release (X11 never reports a focus loss to a window that
// keeps the pointer grab elsewhere): ANY press first cancels a stale strip drag, closing its
// bracket exactly once; a release is ours only if a strip drag was really active.
// Coordinates are window-logical (not scrolled).
#pragma once
#include "Layout.hpp"
#include "Strip.hpp"

#include <functional>

namespace pe {

/// The diagnostics view's level, as saved: 0 collapsed, 1 the strip, 2 the strip and the analysis, 3 the strip and the
/// visualization, 4 the analysis alone, 5 the visualization alone. Each tab toggles its part and leaves the other.
constexpr bool diagStrip(int level) { return level >= 1 && level <= 3; }
/// the panel open: 0 none, 1 the analysis, 2 the visualization
constexpr int diagPanel(int level) { return level == 2 || level == 4 ? 1 : level == 3 || level == 5 ? 2 : 0; }
constexpr int diagLevel(bool strip, int panel) { return panel == 0 ? (strip ? 1 : 0) : (strip ? 1 : 3) + panel; }
constexpr int toggleStrip(int level) { return diagLevel(!diagStrip(level), diagPanel(level)); }
constexpr int toggleAnalysis(int level) { return diagLevel(diagStrip(level), diagPanel(level) == 1 ? 0 : 1); }
constexpr int toggleViz(int level) { return diagLevel(diagStrip(level), diagPanel(level) == 2 ? 0 : 2); }

struct StripHost {
    std::function<void(int key, int velocity)> noteOn; // key 0 = A0
    std::function<void(int key)> noteOff;
    std::function<void(float depth, bool begin, bool end)> pedal; // `begin` opens, `end` closes the edit gesture (depth 0)
    std::function<void(bool down, bool begin, bool end)> sostenuto; // the same for the sostenuto lever (on/off)
    std::function<void(float depth, bool begin, bool end)> soft;    // and the soft pedal's lever (depth)
    std::function<void()> levelToggle;                            // the "Keyboard" tab
    std::function<void()> analysisToggle;                         // the "Analysis" tab
    std::function<void()> vizToggle;                              // the "Visualization" tab (none where the layout has no tab)
};

/// where the parts are: the view starts at `top`, with a panel open it fills `panelH` below it,
/// then the header row (the tabs) and the strip
struct StripLayout {
    float top = 0, panelH = 0;
    Rect keyboardTab, analysisTab, vizTab;
    StripGeometry strip;
};

class StripInteraction {
public:
    enum class Press {
        Outside, ///< above the view: not ours (any stale strip drag has been cancelled)
        Panel,   ///< in the open panel: the caller routes it there
        Ours,    ///< handled here (a tab, the pedal, a key, or the rest of the header and strip)
    };
    explicit StripInteraction(StripHost host);
    StripInteraction(const StripInteraction&) = delete; // fPlayer's callbacks capture this
    StripInteraction& operator=(const StripInteraction&) = delete;
    /// a pedal being dragged that the new layout no longer has is released
    void setLayout(const StripLayout& layout);
    void setLevel(int level) { fLevel = level; } // diagStrip, diagPanel
    Press press(float x, float y);
    /// true only if a strip drag was really active (a release that is not ours goes on to the controls)
    bool release();
    /// true: a strip drag took the motion
    bool motion(float x, float y);
    /// focus lost, window closing, a stale drag: no note left on, the pedal back at 0 (bracket closed)
    void cancel();
    bool dragging() const { return fKeyDrag || fPedalDrag || fSostenutoDrag || fSoftDrag; }
    bool pedalDragging() const { return fPedalDrag; }
    float pedalSent() const { return fPedalSent; } // the lever's depth while dragged (0 otherwise)
    bool sostenutoDragging() const { return fSostenutoDrag; }
    bool softDragging() const { return fSoftDrag; }
    float softSent() const { return fSoftSent; }

private:
    void endPedal();
    void endSostenuto();
    void endSoft();
    StripHost fHost;
    StripLayout fLayout;
    int fLevel = 0;
    StripPlayer fPlayer;
    bool fPedalDrag = false;
    bool fSostenutoDrag = false;
    bool fSoftDrag = false;
    float fSoftSent = 0;
    bool fKeyDrag = false; // pressed on the keys: moving plays (also back onto the keys after leaving them)
    float fPedalSent = 0;
};

} // namespace pe
