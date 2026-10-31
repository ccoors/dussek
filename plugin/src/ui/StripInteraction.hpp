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

struct StripHost {
    std::function<void(int key, int velocity)> noteOn; // key 0 = A0
    std::function<void(int key)> noteOff;
    std::function<void(float depth, bool begin, bool end)> pedal; // `begin` opens, `end` closes the edit gesture (depth 0)
    std::function<void(bool down, bool begin, bool end)> sostenuto; // the same for the sostenuto lever (on/off)
    std::function<void(float depth, bool begin, bool end)> soft;    // and the soft pedal's lever (depth)
    std::function<void()> levelToggle;                            // the "Keyboard" tab
    std::function<void()> analysisToggle;                         // the "Analysis" tab (strip open only)
    std::function<void()> vizToggle;                              // the "Visualization" tab (the same)
};

/// where the parts are: the view starts at `top`, at level 2 or 3 a panel fills `panelH` below it,
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
        Panel,   ///< in the open panel (level 2 or 3): the caller routes it there
        Ours,    ///< handled here (a tab, the pedal, a key, or the rest of the header and strip)
    };
    explicit StripInteraction(StripHost host);
    StripInteraction(const StripInteraction&) = delete; // fPlayer's callbacks capture this
    StripInteraction& operator=(const StripInteraction&) = delete;
    void setLayout(const StripLayout& layout) { fLayout = layout; }
    void setLevel(int level) { fLevel = level; } // 0 collapsed, 1 strip, 2 strip + analysis, 3 strip + visualization
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
    /// the soft pedal does nothing on some instruments (the harpsichords): its lever is dead
    void setSoftEnabled(bool on) { fSoftEnabled = on; }

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
    bool fSoftDrag = false, fSoftEnabled = true;
    float fSoftSent = 0;
    bool fKeyDrag = false; // pressed on the keys: moving plays (also back onto the keys after leaving them)
    float fPedalSent = 0;
};

} // namespace pe
