#include "DiagView.hpp"
#include "Theme.hpp"
#include "Working.hpp"

#include <algorithm>

namespace pe {
using DGL_NAMESPACE::Color;
using DGL_NAMESPACE::NanoVG;

namespace {
Color mix(const Color& a, const Color& b, float t)
{
    return Color(a.red + (b.red - a.red) * t, a.green + (b.green - a.green) * t, a.blue + (b.blue - a.blue) * t, 1.0f);
}

void drawTab(NanoVG& vg, const Rect& r, const char* label, bool on)
{
    vg.beginPath();
    vg.roundedRect(r.x, r.y, r.w, r.h, theme::radius);
    vg.fillColor(on ? theme::line : theme::bg);
    vg.fill();
    vg.fillColor(on ? theme::accentText : theme::dim);
    vg.textAlign(NanoVG::Align(NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE));
    vg.text(r.x + r.w / 2, r.y + r.h / 2, label, nullptr);
}
}

DiagView::DiagView(DiagHost host)
    : fHost(std::move(host)),
      fInput(StripHost {
          [this](int k, int v) { fHost.noteOn(k, v); fHost.repaint(); },
          [this](int k) { fHost.noteOff(k); fHost.repaint(); },
          [this](float depth, bool begin, bool end) { fHost.pedal(depth, begin, end); fHost.repaint(); },
          [this](bool down, bool begin, bool end) { fHost.sostenuto(down, begin, end); fHost.repaint(); },
          [this](float depth, bool begin, bool end) { fHost.soft(depth, begin, end); fHost.repaint(); },
          [this] { setLevel(toggleStrip(fLevel)); fHost.changed(); },
          [this] { setLevel(toggleAnalysis(fLevel)); fHost.changed(); },
          [this] { setLevel(toggleViz(fLevel)); fHost.changed(); } }),
      fPanel([this] { fHost.repaint(); }, [this] { fHost.changed(); }),
      fViz([this] { fHost.repaint(); })
{
}

float DiagView::height() const
{
    return kHeaderH + (diagStrip(fLevel) ? kStripH : 0.0f) + (diagPanel(fLevel) != 0 ? kPanelH : 0.0f);
}

void DiagView::setLevel(int level)
{
    level = std::min(5, std::max(0, level));
    if (diagPanel(level) == 2 && !fVizOffered) // no visualization here (a saved state from another instrument)
        level = diagLevel(diagStrip(level), 0);
    if (level == fLevel) return;
    if (!diagStrip(level)) cancel(); // the strip shut: no note or pedal left on
    fLevel = level;
    fInput.setLevel(fLevel);
    // one reader of the frames at a time: the one closing first (it clears the engine's flag)
    if (diagPanel(fLevel) != 1) fPanel.close(); // the engine stops collecting
    if (diagPanel(fLevel) != 2) fViz.close();
    if (diagPanel(fLevel) == 1) fPanel.open();
    if (diagPanel(fLevel) == 2) fViz.open();
}

void DiagView::setPanelSettings(const PanelSettings& s) { fPanel.setSettings(s); }
void DiagView::setToken(uint32_t token) { fPanel.setToken(token); fViz.setToken(token); }

void DiagView::idle()
{
    if (diagPanel(fLevel) == 1) fPanel.idle();
    if (diagPanel(fLevel) == 2) fViz.idle(); // nothing to poll or draw otherwise
}

void DiagView::layout(float width, float windowHeight)
{
    fWidth = width;
    fTop = windowHeight - height();
    // the panel on top, then the header, then the strip (its geometry also when shut: the panels' key columns)
    const float header = fTop + (diagPanel(fLevel) != 0 ? kPanelH : 0.0f);
    fStrip = computeStrip({ 0, header + kHeaderH, width, kStripH }, fPedals);
    const DiagBar bar = diagBar(width, header, kHeaderH, diagStrip(fLevel), fStrip);
    fKeyboardTab = bar.keyboard;
    fAnalysisTab = bar.analysis;
    fVizTab = bar.viz;
    fWorkingRect = bar.working;
    fInput.setLayout({ fTop, kPanelH, fKeyboardTab, fAnalysisTab, fVizOffered ? fVizTab : Rect {}, fStrip });
    fPanel.layout({ 0, fTop, width, kPanelH }, fStrip);
    fViz.layout({ 0, fTop, width, kPanelH }, fStrip);
}

void DiagView::setKeyWords(const float words[8])
{
    bool same = true;
    for (int i = 0; i < 8; ++i) same = same && words[i] == fWords[i];
    if (same) return;
    std::copy(words, words + 8, fWords);
    if (diagStrip(fLevel)) fHost.repaint();
}

void DiagView::setPedal(float depth)
{
    if (depth == fPedal) return;
    fPedal = depth;
    if (diagStrip(fLevel)) fHost.repaint();
}

void DiagView::setSostenuto(bool down)
{
    if (down == fSostenuto) return;
    fSostenuto = down;
    if (diagStrip(fLevel)) fHost.repaint();
}

void DiagView::setSoft(float depth)
{
    if (depth == fSoft) return;
    fSoft = depth;
    if (diagStrip(fLevel)) fHost.repaint();
}

void DiagView::setPedals(Pedals p)
{
    if (p.soft == fPedals.soft && p.sostenuto == fPedals.sostenuto && p.sustain == fPedals.sustain) return;
    fPedals = p;
    if (diagStrip(fLevel)) fHost.repaint();
}

void DiagView::setRecorded(int lo, int hi)
{
    if (lo == fRecordedLo && hi == fRecordedHi) return;
    fRecordedLo = lo;
    fRecordedHi = hi;
    if (diagStrip(fLevel)) fHost.repaint();
}

void DiagView::setPlays(int lo, int hi)
{
    if (lo == fPlaysLo && hi == fPlaysHi) return;
    fPlaysLo = lo;
    fPlaysHi = hi;
    if (diagStrip(fLevel)) fHost.repaint();
}

void DiagView::setVizOffered(bool on)
{
    if (on == fVizOffered) return;
    fVizOffered = on;
    if (!on && diagPanel(fLevel) == 2) setLevel(diagLevel(diagStrip(fLevel), 0)); // through setLevel: the input and the panel follow
    fInput.setLayout({ fTop, kPanelH, fKeyboardTab, fAnalysisTab, fVizOffered ? fVizTab : Rect {}, fStrip });
    fHost.repaint();
}

void DiagView::setReversedKeys(bool on)
{
    if (on == fReversedKeys) return;
    fReversedKeys = on;
    if (diagStrip(fLevel)) fHost.repaint();
}

void DiagView::draw(NanoVG& vg)
{
    vg.beginPath(); vg.rect(0, fTop, fWidth, height()); vg.fillColor(theme::panel); vg.fill();
    vg.beginPath(); vg.rect(0, fTop, fWidth, 1); vg.fillColor(theme::line); vg.fill();
    const int panel = diagPanel(fLevel);
    if (panel != 0) {
        if (panel == 1) fPanel.draw(vg); else fViz.draw(vg);
        vg.beginPath(); vg.rect(0, fTop + kPanelH, fWidth, 1); vg.fillColor(theme::line); vg.fill();
    }
    if (panel != 1) fPanel.releaseTexture(); // closed: free the GL image while the context is current (recreated on reopen)
    vg.fontSize(theme::smallFont);
    // the three toggles, each its own part: the strip, the analysis, the visualization
    const bool strip = diagStrip(fLevel);
    drawTab(vg, fKeyboardTab, strip ? "Keyboard  \xE2\x96\xBE" : "Keyboard  \xE2\x96\xB8", strip);
    if (fWorking && fWorkingRect.w > 0) { // a change on its way to the sound (Working.hpp)
        vg.fillColor(theme::accentText);
        vg.textAlign(NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE));
        vg.text(fWorkingRect.x + 4, fWorkingRect.y + fWorkingRect.h / 2, kWorkingLabel, nullptr);
    }
    drawTab(vg, fAnalysisTab, panel == 1 ? "Analysis  \xE2\x96\xBE" : "Analysis  \xE2\x96\xB8", panel == 1);
    if (fVizOffered) drawTab(vg, fVizTab, panel == 2 ? "Visualization  \xE2\x96\xBE" : "Visualization  \xE2\x96\xB8", panel == 2);
    if (!strip) return;

    // reversed (many harpsichords): ebony naturals, bone sharps
    const Color white = fReversedKeys ? Color(76, 68, 60) : Color(220, 222, 225);
    const Color black = fReversedKeys ? Color(226, 218, 200) : Color(34, 36, 40);
    // extrapolated keys (beyond the recorded ones): an amber tint on either color
    const Color amber(204, 150, 84);
    for (int pass = 0; pass < 2; ++pass) // white keys, then black on top
        for (int k = 0; k < kKeys; ++k) {
            const KeyShape& s = fStrip.keys[(size_t)k];
            if (s.black != (pass == 1)) continue;
            const KeyLight l = keyLight(fWords, k);
            // held: the accent; ringing after release: a dim tint; otherwise the key's own color
            Color c = s.black ? black : white;
            if (k + 21 < fRecordedLo || k + 21 > fRecordedHi) c = mix(c, amber, s.black ? 0.4f : 0.45f);
            // keys that do not play: gray, apart from the amber, on either color
            if (k + 21 < fPlaysLo || k + 21 > fPlaysHi) c = s.black ? Color(84, 86, 90) : Color(128, 130, 134);
            if (l.held) c = mix(c, theme::accent, 0.85f);
            else if (l.ringing) c = mix(c, theme::accent, 0.35f);
            vg.beginPath(); vg.rect(s.r.x + 0.5f, s.r.y, std::max(0.5f, s.r.w - 1.0f), s.r.h - 1.0f);
            vg.fillColor(c); vg.fill();
        }
    // a pedal that acts: a slot with a lever at its depth, its name in the header row above
    const auto lever = [&](const Rect& r, float depth, const char* name) {
        if (r.w <= 0) return;
        vg.beginPath(); vg.roundedRect(r.x + r.w / 2 - 3, r.y, 6, r.h, 3); vg.fillColor(theme::line); vg.fill();
        const float y = r.y + 6 + (r.h - 12) * depth;
        vg.beginPath(); vg.roundedRect(r.x, y - 6, r.w, 12, 3);
        vg.fillColor(depth > 0.02f ? theme::accent : theme::marker); vg.fill();
        vg.fillColor(theme::dim);
        vg.textAlign(NanoVG::Align(NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE));
        vg.text(r.x + r.w / 2, fKeyboardTab.y + fKeyboardTab.h / 2, name, nullptr);
    };
    // while dragged: the lever's own depth (the engine's output lags behind); otherwise the engine's
    lever(fStrip.pedal, fInput.pedalDragging() ? fInput.pedalSent() : fPedal, "sust.");
    // the sostenuto: two positions (up / down), as the engine has it
    lever(fStrip.sostenuto, fInput.sostenutoDragging() || fSostenuto ? 1.0f : 0.0f, "sost.");
    lever(fStrip.soft, fInput.softDragging() ? fInput.softSent() : fSoft, "soft");
}

bool DiagView::mouse(bool press, float x, float y)
{
    if (!press) return fInput.release(); // ours only if a strip drag was really active
    switch (fInput.press(x, y)) {
    case StripInteraction::Press::Outside: return false;
    case StripInteraction::Press::Panel: if (diagPanel(fLevel) == 1) fPanel.mouse(x, y); return true;
    case StripInteraction::Press::Ours: break;
    }
    return true;
}

bool DiagView::motion(float x, float y)
{
    if (fInput.motion(x, y)) return true;
    const bool inPanel = diagPanel(fLevel) == 1 && y >= fTop && y < fTop + kPanelH;
    fPanel.hover(inPanel ? x : -1.0f, y);
    return y >= fTop;
}

void DiagView::cancel() { fInput.cancel(); }

} // namespace pe
