#include "DiagView.hpp"
#include "Theme.hpp"

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
          [this] { setLevel(fLevel >= 1 ? 0 : 1); fHost.changed(); },
          [this] { setLevel(fLevel == 2 ? 1 : 2); fHost.changed(); },
          [this] { setLevel(fLevel == 3 ? 1 : 3); fHost.changed(); } }),
      fPanel([this] { fHost.repaint(); }, [this] { fHost.changed(); }),
      fViz([this] { fHost.repaint(); })
{
}

float DiagView::height() const
{
    return kHeaderH + (fLevel >= 1 ? kStripH : 0.0f) + (fLevel >= 2 ? kPanelH : 0.0f);
}

void DiagView::setLevel(int level)
{
    level = std::min(3, std::max(0, level));
    if (level == fLevel) return;
    if (level == 0) cancel();
    fLevel = level;
    fInput.setLevel(fLevel);
    // one reader of the frames at a time: the one closing first (it clears the engine's flag)
    if (fLevel != 2) fPanel.close(); // the engine stops collecting
    if (fLevel != 3) fViz.close();
    if (fLevel == 2) fPanel.open();
    if (fLevel == 3) fViz.open();
}

void DiagView::setPanelSettings(const PanelSettings& s) { fPanel.setSettings(s); }
void DiagView::setToken(uint32_t token) { fPanel.setToken(token); fViz.setToken(token); }

void DiagView::idle()
{
    if (fLevel == 2) fPanel.idle();
    if (fLevel == 3) fViz.idle(); // nothing to poll or draw otherwise
}

void DiagView::layout(float width, float windowHeight)
{
    fWidth = width;
    fTop = windowHeight - height();
    // the panel on top, then the header, then the strip
    const float header = fTop + (fLevel >= 2 ? kPanelH : 0.0f);
    fKeyboardTab = { 8, header + 3, 96, kHeaderH - 6 };
    fAnalysisTab = { 112, header + 3, 84, kHeaderH - 6 };
    fVizTab = { 204, header + 3, 108, kHeaderH - 6 };
    fStrip = computeStrip({ 0, header + kHeaderH, width, kStripH });
    fInput.setLayout({ fTop, kPanelH, fKeyboardTab, fAnalysisTab, fVizTab, fStrip });
    fPanel.layout({ 0, fTop, width, kPanelH }, fStrip);
    fViz.layout({ 0, fTop, width, kPanelH }, fStrip);
}

void DiagView::setKeyWords(const float words[8])
{
    bool same = true;
    for (int i = 0; i < 8; ++i) same = same && words[i] == fWords[i];
    if (same) return;
    std::copy(words, words + 8, fWords);
    if (fLevel >= 1) fHost.repaint();
}

void DiagView::setPedal(float depth)
{
    if (depth == fPedal) return;
    fPedal = depth;
    if (fLevel >= 1) fHost.repaint();
}

void DiagView::setSostenuto(bool down)
{
    if (down == fSostenuto) return;
    fSostenuto = down;
    if (fLevel >= 1) fHost.repaint();
}

void DiagView::setSoft(float depth)
{
    if (depth == fSoft) return;
    fSoft = depth;
    if (fLevel >= 1) fHost.repaint();
}

void DiagView::setSoftEnabled(bool on)
{
    if (on == fSoftEnabled) return;
    fSoftEnabled = on;
    if (!on) fInput.cancel();
    fInput.setSoftEnabled(on);
    if (fLevel >= 1) fHost.repaint();
}

void DiagView::setRecorded(int lo, int hi)
{
    if (lo == fRecordedLo && hi == fRecordedHi) return;
    fRecordedLo = lo;
    fRecordedHi = hi;
    if (fLevel >= 1) fHost.repaint();
}

void DiagView::setReversedKeys(bool on)
{
    if (on == fReversedKeys) return;
    fReversedKeys = on;
    if (fLevel >= 1) fHost.repaint();
}

void DiagView::draw(NanoVG& vg)
{
    vg.beginPath(); vg.rect(0, fTop, fWidth, height()); vg.fillColor(theme::panel); vg.fill();
    vg.beginPath(); vg.rect(0, fTop, fWidth, 1); vg.fillColor(theme::line); vg.fill();
    if (fLevel >= 2) {
        if (fLevel == 2) fPanel.draw(vg); else fViz.draw(vg);
        vg.beginPath(); vg.rect(0, fTop + kPanelH, fWidth, 1); vg.fillColor(theme::line); vg.fill();
    }
    if (fLevel != 2) fPanel.releaseTexture(); // closed: free the GL image while the context is current (recreated on reopen)
    vg.fontSize(theme::smallFont);
    // the Analysis and Visualization toggles exist only on the open strip
    drawTab(vg, fKeyboardTab, fLevel >= 1 ? "Keyboard  \xE2\x96\xBE" : "Keyboard  \xE2\x96\xB8", fLevel >= 1);
    if (fLevel < 1) return;
    drawTab(vg, fAnalysisTab, fLevel == 2 ? "Analysis  \xE2\x96\xBE" : "Analysis  \xE2\x96\xB8", fLevel == 2);
    drawTab(vg, fVizTab, fLevel == 3 ? "Visualization  \xE2\x96\xBE" : "Visualization  \xE2\x96\xB8", fLevel == 3);

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
            if (l.held) c = mix(c, theme::accent, 0.85f);
            else if (l.ringing) c = mix(c, theme::accent, 0.35f);
            vg.beginPath(); vg.rect(s.r.x + 0.5f, s.r.y, std::max(0.5f, s.r.w - 1.0f), s.r.h - 1.0f);
            vg.fillColor(c); vg.fill();
        }
    // the pedal: a slot with a lever at the current depth
    const Rect& p = fStrip.pedal;
    vg.beginPath(); vg.roundedRect(p.x + p.w / 2 - 3, p.y, 6, p.h, 3); vg.fillColor(theme::line); vg.fill();
    // while dragged: the lever's own depth (the engine's output lags behind); otherwise the engine's
    const float depth = fInput.pedalDragging() ? fInput.pedalSent() : fPedal;
    const float y = p.y + 6 + (p.h - 12) * depth;
    vg.beginPath(); vg.roundedRect(p.x, y - 6, p.w, 12, 3);
    vg.fillColor(depth > 0.02f ? theme::accent : theme::marker); vg.fill();
    // the sostenuto: the same slot, two positions (up / down), as the engine has it
    const Rect& s = fStrip.sostenuto;
    vg.beginPath(); vg.roundedRect(s.x + s.w / 2 - 3, s.y, 6, s.h, 3); vg.fillColor(theme::line); vg.fill();
    const bool sdown = fInput.sostenutoDragging() || fSostenuto;
    const float sy = s.y + 6 + (s.h - 12) * (sdown ? 1.0f : 0.0f);
    vg.beginPath(); vg.roundedRect(s.x, sy - 6, s.w, 12, 3);
    vg.fillColor(sdown ? theme::accent : theme::marker); vg.fill();
    // the soft pedal: depth like the sustain; dim and without accent where the instrument has none
    const Rect& u = fStrip.soft;
    vg.beginPath(); vg.roundedRect(u.x + u.w / 2 - 3, u.y, 6, u.h, 3); vg.fillColor(theme::line); vg.fill();
    const float udepth = fInput.softDragging() ? fInput.softSent() : fSoft;
    const float uy = u.y + 6 + (u.h - 12) * udepth;
    vg.beginPath(); vg.roundedRect(u.x, uy - 6, u.w, 12, 3);
    vg.fillColor(!fSoftEnabled ? theme::line : udepth > 0.02f ? theme::accent : theme::marker); vg.fill();
    // their names in the header row above
    vg.fillColor(theme::dim);
    vg.textAlign(NanoVG::Align(NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE));
    vg.text(s.x + s.w / 2, fKeyboardTab.y + fKeyboardTab.h / 2, "sost.", nullptr);
    vg.text(u.x + u.w / 2, fKeyboardTab.y + fKeyboardTab.h / 2, "soft", nullptr);
    vg.text(p.x + p.w / 2, fKeyboardTab.y + fKeyboardTab.h / 2, "sust.", nullptr);
}

bool DiagView::mouse(bool press, float x, float y)
{
    if (!press) return fInput.release(); // ours only if a strip drag was really active
    switch (fInput.press(x, y)) {
    case StripInteraction::Press::Outside: return false;
    case StripInteraction::Press::Panel: if (fLevel == 2) fPanel.mouse(x, y); return true;
    case StripInteraction::Press::Ours: break;
    }
    return true;
}

bool DiagView::motion(float x, float y)
{
    if (fInput.motion(x, y)) return true;
    const bool inPanel = fLevel == 2 && y >= fTop && y < fTop + kPanelH;
    fPanel.hover(inPanel ? x : -1.0f, y);
    return y >= fTop;
}

void DiagView::cancel() { fInput.cancel(); }

} // namespace pe
