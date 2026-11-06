// The diagnostics view at the window's bottom: at level 2 the analysis panel (DiagPanel), at
// level 3 the visualization (VizPanel) in its place, a header row ("Keyboard" tab, and with the
// strip open the "Analysis" and "Visualization" toggles) and the player strip (keys and pedal).
// Coordinates are window-logical (not scrolled).
#pragma once
#include "DiagPanel.hpp"
#include "Layout.hpp"
#include "NanoVG.hpp"
#include "Strip.hpp"
#include "StripInteraction.hpp"
#include "VizPanel.hpp"

#include <functional>

namespace pe {

struct DiagHost {
    std::function<void(int key, int velocity)> noteOn; // key 0 = A0
    std::function<void(int key)> noteOff;
    std::function<void(float depth, bool begin, bool end)> pedal; // edit gesture brackets
    std::function<void(bool down, bool begin, bool end)> sostenuto; // the same, on/off
    std::function<void(float depth, bool begin, bool end)> soft;    // and the soft pedal
    std::function<void()> repaint;
    std::function<void()> changed; // level or panel settings changed: persist and relayout
};

class DiagView {
public:
    static constexpr float kHeaderH = 24, kStripH = 46, kPanelH = 300;
    using PanelSettings = pe::PanelSettings;
    explicit DiagView(DiagHost host);
    /// logical height of the diagnostics area shown; it occupies the bottom and reduces the controls viewport
    float height() const;
    void setLevel(int level); // 0 collapsed, 1 strip, 2 strip + analysis, 3 strip + visualization
    int level() const { return fLevel; }
    void layout(float width, float windowHeight);
    void setKeyWords(const float words[8]);
    void setPedal(float depth); // the engine's pedal depth (shown when the lever is not dragged)
    void setSostenuto(bool down); // the engine's sostenuto state (CC66 or the lever)
    void setSoft(float depth);    // the engine's soft pedal depth (CC67 or the lever)
    void setPedals(Pedals p);     // the pedals on the strip; `layout` places them
    void setBars(bool bars) { fViz.setBars(bars); } // the visualization draws bars, not strings
    void setRecorded(int lo, int hi); // the instrument's recorded keys (MIDI): the others are tinted as extrapolated
    void setReversedKeys(bool on);    // dark naturals and light sharps (many harpsichords)
    void setWorking(bool on) { fWorking = on; } // "updating…" in the bar (the caller repaints)
    void draw(DGL_NAMESPACE::NanoVG& vg);
    bool mouse(bool press, float x, float y); // true: the event is ours
    bool motion(float x, float y);
    void cancel(); // focus lost, window closing: no note or pedal left on (pedal sent back to 0)
    PanelSettings panelSettings() const { return fPanel.settings(); }
    void setPanelSettings(const PanelSettings& s);
    void setConcertPitch(float hz) { fPanel.setConcertPitch(hz); }
    void setToken(uint32_t token); // the DSP's region token (0: none): opens it, or marks the panel unavailable
    void idle();                   // from uiIdle: polls telemetry, repaints while a panel is open

private:
    DiagHost fHost;
    int fLevel = 0;
    float fWidth = 0, fTop = 0;
    Rect fKeyboardTab, fAnalysisTab, fVizTab, fWorkingRect;
    bool fWorking = false;
    StripGeometry fStrip;
    float fWords[8] = {};
    float fPedal = 0;     // the engine's pedal depth (kDiagPedalOut)
    float fSoft = 0;      // the engine's soft pedal depth (kDiagSoftOut)
    Pedals fPedals;
    int fRecordedLo = 21, fRecordedHi = 108; // MIDI
    bool fReversedKeys = false;
    bool fSostenuto = false; // the engine's sostenuto state (kDiagSostenutoOut)
    StripInteraction fInput; // the pointer state machine (pure); this class draws and delegates
    DiagPanel fPanel; // open (telemetry on) exactly at level 2
    VizPanel fViz;    // and this one at level 3
};

} // namespace pe
