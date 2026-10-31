// The visualization panel above the diagnostics' header, the Analysis panel's alternative: the
// instrument as the engine plays it, each key's strings from above (a sounding string a glowing
// band as wide as its partials move it, exaggerated; the strings ringing along shown larger), the
// dampers on them, the hammers from the front rising to the strings; bars instead of strings on a
// bar instrument. Owned by DiagView; works (polls telemetry, draws) only while open.
// Window-logical coordinates.
#pragma once
#include "Layout.hpp"
#include "NanoVG.hpp"
#include "Strip.hpp"
#include "TelemetryClient.hpp"
#include "Viz.hpp"

#include <cstdint>
#include <functional>
#include <memory>

namespace pe {

class VizPanel {
public:
    explicit VizPanel(std::function<void()> repaint) : fRepaint(std::move(repaint)) {}
    ~VizPanel() { close(); }
    void open();  // enable telemetry (if the region opens)
    void close(); // disable telemetry (the engine stops writing)
    bool isOpen() const { return fOpen; }
    void setToken(uint32_t token); // reopens when open
    void setBars(bool bars) { fBars = bars; } // bars on a frame (the mallet instruments)
    void layout(const Rect& area, const StripGeometry& strip);
    void idle(); // polls telemetry, repaints while anything moves
    void draw(DGL_NAMESPACE::NanoVG& vg);

private:
    void poll();
    uint64_t engineNow() const; // the engine's position now, estimated from the newest frame
    void drawStrings(DGL_NAMESPACE::NanoVG& vg, uint64_t now);
    void drawBars(DGL_NAMESPACE::NanoVG& vg, uint64_t now);
    void drawHammers(DGL_NAMESPACE::NanoVG& vg, uint64_t now);

    std::function<void()> fRepaint;
    bool fOpen = false, fAvailable = false, fBars = false;
    uint32_t fToken = 0;
    TelemetryClient fClient;
    std::unique_ptr<PeTelemetryFrame> fFrame;
    bool fHaveFrame = false;
    uint64_t fFrameAtMs = 0; // when the newest frame arrived
    float fRate = 48000.0f;
    HammerTrack fHammers[kKeys];
    bool fAnyHammer = false; // some key is struck by a hammer: the hammers' row is drawn
    bool fSounding = false;  // some string sounds: its snapshot is drawn anew at every UI tick
    uint32_t fSeed = 1;      // the strings' snapshot phases, new each drawing
    Rect fArea, fPlot;
    StripGeometry fStrip;
    float fFront = 0;        // the strings' front end (where the hammers strike)
};

} // namespace pe
