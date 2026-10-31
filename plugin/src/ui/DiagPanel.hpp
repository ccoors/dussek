// The analysis panel above the diagnostics' header: a spectrogram of the output with the
// engine's partials on its newest column, or the partials as bars above their keys (the
// strip's x axis), a hover tooltip and the view/range/floor/freeze controls. Owned by
// DiagView; works (polls telemetry, draws) only while open. Window-logical coordinates.
#pragma once
#include "Layout.hpp"
#include "NanoVG.hpp"
#include "Spectro.hpp"
#include "Strip.hpp"
#include "TelemetryClient.hpp"

#include <functional>
#include <memory>
#include <cstddef>

namespace pe {

struct PanelSettings { int view = 0; float lo = 27.5f, hi = 16000.0f, floorDb = -90.0f; }; // view: 0 spectrogram, 1 partials

class DiagPanel {
public:
    DiagPanel(std::function<void()> repaint, std::function<void()> changed) : fRepaint(std::move(repaint)), fChanged(std::move(changed)) {}
    ~DiagPanel() { close(); }
    void open();  // enable telemetry (if the region opens), start from an empty picture
    void close(); // disable telemetry (the engine stops writing), drop the data (the texture goes at the next draw)
    /// Frees the waterfall's texture. A NanoVG image must be deleted while the GL context is current, so this
    /// is called from the draw path (DiagView::draw) while the panel is closed, never from close() itself.
    void releaseTexture() { fTexture.reset(); }
    bool hasTexture() const { return fTexture != nullptr; }
    bool isOpen() const { return fOpen; }
    void setToken(uint32_t token); // reopens when open
    void setConcertPitch(float hz) { if (hz > 0 && hz < 2000) fConcertPitch = hz; } // the tooltip's harmonic reference
    PanelSettings settings() const { return fSettings; }
    void setSettings(const PanelSettings& s);
    /// the panel's area and the strip below it (the partials' x axis)
    void layout(const Rect& area, const StripGeometry& strip);
    void idle(); // polls telemetry, repaints at the frame rate (nothing when closed)
    void draw(DGL_NAMESPACE::NanoVG& vg);
    bool mouse(float x, float y); // a press
    void hover(float x, float y); // x < 0: away
private:
    void poll();
    void clearImage();
    void addColumn();
    void drawSpectrogram(DGL_NAMESPACE::NanoVG& vg, const Rect& plot);
    void drawPartials(DGL_NAMESPACE::NanoVG& vg, const Rect& plot);
    void drawHover(DGL_NAMESPACE::NanoVG& vg);
    /// the shown partial nearest to (x, y) within 8 px in the active view: voice·64 + partial, or -1
    int nearest(float x, float y, float& px, float& py) const;
    bool partialPos(int v, int p, float& x, float& y) const;
    void shownChanged(); // fRef from the shown frame
    void status(char* out, size_t n) const;
    void settingsChanged(bool clear);

    std::function<void()> fRepaint, fChanged;
    bool fOpen = false;
    uint32_t fToken = 0;
    TelemetryClient fClient;
    bool fAvailable = false; // the region opened
    DropWatch fDrops; // the engine's dropped counter, re-based at open(); the notice shows ~2 s after growth
    PanelSettings fSettings;
    Spectrogram fSpectro;
    // the waterfall: a ring of kColumns columns (+ the raw dB columns retained for a re-render when the range or
    // floor changes), drawn as two rectangles; the texture exists only while the panel is drawn open
    Waterfall fWave { kColumns, kRows };
    std::unique_ptr<DGL_NAMESPACE::NanoImage> fTexture;
    bool fImageDirty = false, fFreeze = false, fNewFrame = false;
    std::unique_ptr<PeTelemetryFrame> fFrame, fShown; // the latest; the one on screen (frozen or live)
    bool fHaveFrame = false;
    float fRate = 48000.0f;
    uint64_t fLastFrameAt = 0; // ms: for "stalled"
    float fRef = 1.0f;         // the loudest partial of the shown frame (0 dB), computed once per shown frame
    float fConcertPitch = 440.0f;
    char fStatusShown[48] = "";
    Rect fArea, fSpecPlot, fPartPlot, fViewTab[2], fRangeBtn, fFloorBtn, fFreezeBtn;
    StripGeometry fStrip;
    float fHoverX = -1, fHoverY = -1;
    int fHoverTarget = -1;
    static constexpr int kColumns = 480, kRows = 180;
};

} // namespace pe
