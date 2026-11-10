#include "DiagPanel.hpp"
#include "Theme.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstring>

namespace pe {
using DGL_NAMESPACE::Color;
using DGL_NAMESPACE::NanoVG;

namespace {
constexpr float kRanges[5][2] = { { 27.5f, 16000.0f }, { 27.5f, 4000.0f }, { 55.0f, 2000.0f }, { 200.0f, 8000.0f }, { 1000.0f, 20000.0f } };
constexpr float kFloors[5] = { -60.0f, -70.0f, -80.0f, -90.0f, -100.0f };
constexpr int kNearestNone = -1;
constexpr float kHoverRadius = 8;
// NVG_IMAGE_NEAREST from nanovg.h; DGL's NanoVG::ImageFlags does not list it
constexpr int kImageNearest = 1 << 5;

uint64_t nowMs()
{
    return (uint64_t)std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now().time_since_epoch()).count();
}

Color voiceColor(int i)
{
    static const Color hues[6] = { Color(91, 154, 194), Color(222, 152, 84), Color(126, 186, 112),
                                   Color(196, 124, 186), Color(214, 194, 92), Color(104, 196, 184) };
    return hues[(size_t)(i % 6)];
}

// by T60: cool (blue) for short, warm (orange) for long-lived
Color decayColor(float decay)
{
    const float t60 = decay > 0 ? 6.91f / decay : 60.0f;
    const float t = std::min(1.0f, std::max(0.0f, std::log10(t60 / 0.05f) / std::log10(60.0f / 0.05f)));
    return Color(Color(90, 160, 235), Color(245, 150, 60), t);
}

void button(NanoVG& vg, const Rect& r, const char* label, bool on)
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

void DiagPanel::open()
{
    if (!fFrame) fFrame.reset(new PeTelemetryFrame());
    if (!fShown) fShown.reset(new PeTelemetryFrame());
    fOpen = true;
    fSpectro.clear();
    clearImage();
    fHaveFrame = false;
    fNewFrame = false;
    fFreeze = false;
    fHoverTarget = kNearestNone;
    std::memset(fShown.get(), 0, sizeof(PeTelemetryFrame));
    shownChanged();
    fAvailable = fClient.open(fToken);
    if (fAvailable) fClient.setEnabled(true);
    fDrops = DropWatch();
    fDrops.last = fClient.dropped();
    fLastFrameAt = nowMs();
}

void DiagPanel::close()
{
    fClient.close(); // clears the enabled flag: the engine stops collecting
    fAvailable = false;
    fOpen = false;
    fHaveFrame = false;
    fWave.clear(); // the retained columns (~4 MB) go with the panel
    fImageDirty = true;
    fStatusShown[0] = '\0';
}

void DiagPanel::setToken(uint32_t token)
{
    if (token == fToken) return;
    fToken = token;
    if (fOpen) { open(); fRepaint(); }
    // a crashed earlier UI may have left the flag on. Host edge case: a second UI attached to the same
    // instance can clear the first UI's flag this way (the same conflict exists through TelemetryClient::close)
    else if (token != 0) TelemetryClient::clearFlag(token);
}

void DiagPanel::setSettings(const PanelSettings& s)
{
    const bool remap = s.lo != fSettings.lo || s.hi != fSettings.hi || s.floorDb != fSettings.floorDb;
    fSettings = s;
    if (remap) { // the retained columns are drawn again for the new range/floor (also while frozen)
        fWave.remap(fSettings.lo, fSettings.hi, fSettings.floorDb);
        fImageDirty = true;
    }
}

void DiagPanel::clearImage() // an empty picture, the retained columns freed
{
    fWave.clear();
    fImageDirty = true;
}

void DiagPanel::addColumn()
{
    fWave.add(fSpectro.column(), fRate, fSettings.lo, fSettings.hi, fSettings.floorDb);
    fImageDirty = true;
}

void DiagPanel::setPartialsOffered(bool on)
{
    if (on == fPartials) return;
    fPartials = on;
    fHoverTarget = kNearestNone;
    layout(fArea, fStrip);
    fRepaint();
}

void DiagPanel::layout(const Rect& area, const StripGeometry& strip)
{
    fArea = area;
    fStrip = strip;
    // without the Partials (a bell's players are no strings' voices) the buttons close up
    float x = 8;
    for (int i = 0; i < 5; ++i) {
        Rect& r = i < 2 ? fViewTab[i] : i == 2 ? fRangeBtn : i == 3 ? fFloorBtn : fFreezeBtn;
        r = i == 1 && !fPartials ? Rect {} : Rect { x, area.y + 4, 90, 22 };
        x += r.w > 0 ? 96 : 0;
    }
    const float y = area.y + 30, h = area.h - 38;
    fSpecPlot = { 44, y, area.w - 52, h }; // a margin on the left for the frequency labels
    fPartPlot = { strip.keysArea.x, y, strip.keysArea.w, h }; // above the keys
}

void DiagPanel::poll()
{
    if (!fAvailable) return;
    fDrops = dropWatchUpdate(fDrops, fClient.dropped(), nowMs());
    if (fClient.pollFrame(*fFrame)) {
        fHaveFrame = true;
        fNewFrame = true;
        if (fFrame->rate > 0 && std::isfinite(fFrame->rate)) fRate = fFrame->rate;
        fLastFrameAt = nowMs();
        if (!fFreeze) { *fShown = *fFrame; shownChanged(); }
    }
    float audio[2 * 2048];
    for (;;) { // the audio since last time into the spectrogram, a hop at a time (each completed column drawn)
        const size_t n = fClient.pollAudio(audio, 2048);
        if (n == 0) break;
        for (size_t off = 0; off < n; off += 1024) {
            const size_t m = std::min<size_t>(1024, n - off);
            if (fSpectro.push(audio + 2 * off, m) > 0 && !fFreeze) addColumn(); // one column written into the ring
        }
    }
}

void DiagPanel::status(char* out, size_t n) const
{
    const uint32_t v = !fPartials ? kNoVoices : fHaveFrame ? std::min<uint32_t>(fShown->voices, PE_TELEMETRY_VOICES) : 0;
    formatStatus(out, n, fAvailable, dropsRecent(fDrops, nowMs()) ? fDrops.total : 0, nowMs() - fLastFrameAt > 1000, v, fFreeze);
}

void DiagPanel::idle()
{
    if (!fOpen) return; // closed: no work at all
    poll();
    bool repaint = (fImageDirty && view() == 0) || (fNewFrame && !fFreeze);
    fNewFrame = false;
    char st[sizeof fStatusShown];
    status(st, sizeof st);
    if (std::strcmp(st, fStatusShown) != 0) { std::memcpy(fStatusShown, st, sizeof st); repaint = true; }
    if (repaint) fRepaint();
}

void DiagPanel::draw(NanoVG& vg)
{
    if (!fOpen) return;
    vg.fontSize(theme::smallFont);
    button(vg, fViewTab[0], "Spectrogram", view() == 0);
    if (fPartials) button(vg, fViewTab[1], "Partials", view() == 1);
    char lo[24], hi[24], range[64];
    formatHz(fSettings.lo, lo, sizeof lo);
    formatHz(fSettings.hi, hi, sizeof hi);
    std::snprintf(range, sizeof range, "%s\xE2\x80\x93%s Hz", lo, hi);
    button(vg, fRangeBtn, range, false);
    char floor[32];
    std::snprintf(floor, sizeof floor, "floor %d dB", (int)std::lround(fSettings.floorDb));
    button(vg, fFloorBtn, floor, false);
    button(vg, fFreezeBtn, fFreeze ? "Frozen" : "Freeze", fFreeze);
    vg.fillColor(theme::dim);
    vg.textAlign(NanoVG::Align(NanoVG::ALIGN_RIGHT | NanoVG::ALIGN_MIDDLE));
    if (fStatusShown[0] != '\0') // DGL asserts on an empty string
        vg.text(fArea.x + fArea.w - 10, fArea.y + 15, fStatusShown, nullptr);

    const Rect& plot = view() == 0 ? fSpecPlot : fPartPlot;
    vg.beginPath(); vg.rect(plot.x, plot.y, plot.w, plot.h); vg.fillColor(theme::bg); vg.fill();
    if (!fAvailable) {
        vg.fillColor(theme::dim);
        vg.textAlign(NanoVG::Align(NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE));
        vg.text(plot.x + plot.w / 2, plot.y + plot.h / 2, "Analysis unavailable: no shared memory with the engine (sandboxed host?)", nullptr);
        return;
    }
    vg.save();
    vg.scissor(plot.x, plot.y, plot.w, plot.h);
    if (view() == 0) drawSpectrogram(vg, plot);
    else drawPartials(vg, plot);
    vg.restore();
    if (view() == 0) { // the frequency labels, left of the plot
        vg.fillColor(theme::faint);
        vg.textAlign(NanoVG::Align(NanoVG::ALIGN_RIGHT | NanoVG::ALIGN_MIDDLE));
        for (float f : { 55.0f, 110.0f, 220.0f, 440.0f, 880.0f, 1760.0f, 3520.0f, 7040.0f, 14080.0f }) {
            if (f < fSettings.lo || f > fSettings.hi) continue;
            const float y = plot.y + plot.h * freqToRow(f, fSettings.lo, fSettings.hi, kRows) / kRows;
            const char* l[] = { "55", "110", "220", "440", "880", "1.76k", "3.5k", "7k", "14k" };
            const int i = (int)std::lround(std::log2(f / 55.0f));
            vg.text(plot.x - 5, y, l[i], nullptr);
            vg.beginPath(); vg.rect(plot.x - 3, y, 3, 1); vg.fillColor(theme::faint); vg.fill();
        }
    }
    drawHover(vg);
}

void DiagPanel::drawSpectrogram(NanoVG& vg, const Rect& plot)
{
    if (!fTexture) {
        fTexture.reset(new DGL_NAMESPACE::NanoImage(vg.createImageFromRGBA(kColumns, kRows, fWave.rgba(), kImageNearest)));
        if (!fTexture->isValid()) fTexture.reset(); // no texture: nothing drawn, tried again next frame
    } else if (fImageDirty) {
        fTexture->update(fWave.rgba()); // the whole image: NanoImage has no partial update
    }
    fImageDirty = false;
    if (fTexture) {
        // the ring drawn as two rectangles, oldest part left: physical columns [head, kColumns) then [0, head). The
        // older rectangle reaches one pixel into the newer one. nanovg_gl.h clamps to the edge, so that sliver samples the
        // older part's LAST column (the seam's left neighbor), and the second rectangle's antialiased left edge (fractional x)
        // blends over it: the one pixel straddling the seam is a blend of its two neighbors where the single image would pick one.
        const ColumnRing::Split sp = fWave.ring().split();
        const float cw = plot.w / (float)kColumns;
        const float olderW = cw * (float)sp.firstCount;
        vg.beginPath();
        vg.rect(plot.x, plot.y, sp.secondCount > 0 ? std::min(plot.w, olderW + 1.0f) : plot.w, plot.h);
        vg.fillPaint(vg.imagePattern(plot.x - cw * (float)sp.first, plot.y, plot.w, plot.h, 0, *fTexture, 1.0f));
        vg.fill();
        if (sp.secondCount > 0) {
            vg.beginPath();
            vg.rect(plot.x + olderW, plot.y, plot.w - olderW, plot.h);
            vg.fillPaint(vg.imagePattern(plot.x + olderW, plot.y, plot.w, plot.h, 0, *fTexture, 1.0f));
            vg.fill();
        }
    }
    // the engine's partials on the newest column
    const uint32_t nv = std::min<uint32_t>(fShown->voices, PE_TELEMETRY_VOICES);
    for (uint32_t v = 0; v < nv; ++v)
        for (int p = 0; p < std::min<int>(fShown->voice[v].count, PE_TELEMETRY_PARTIALS); ++p) {
            float x, y;
            if (!partialPos((int)v, p, x, y)) continue;
            vg.beginPath();
            vg.circle(x, y, 2.2f);
            vg.fillColor(decayColor(fShown->voice[v].partials[p].decay));
            vg.fill();
            vg.strokeWidth(0.8f);
            vg.strokeColor(Color(0, 0, 0, 0.6f));
            vg.stroke();
        }
}

void DiagPanel::shownChanged()
{
    float ref = 0;
    const uint32_t nv = std::min<uint32_t>(fShown->voices, PE_TELEMETRY_VOICES);
    for (uint32_t v = 0; v < nv; ++v)
        if (fShown->voice[v].count > 0) ref = std::max(ref, fShown->voice[v].partials[0].amp);
    fRef = ref > 0 && std::isfinite(ref) ? ref : 1.0f;
}

bool DiagPanel::partialPos(int v, int p, float& x, float& y) const
{
    const PeTelemetryPartial& q = fShown->voice[v].partials[p];
    if (!(q.freq > 0) || !std::isfinite(q.freq)) return false;
    if (view() == 0) {
        if (q.freq < fSettings.lo || q.freq > fSettings.hi) return false;
        x = fSpecPlot.x + fSpecPlot.w - 2;
        y = fSpecPlot.y + fSpecPlot.h * freqToRow(q.freq, fSettings.lo, fSettings.hi, kRows) / kRows;
        return true;
    }
    // beyond the keyboard the strip's x is clamped: such partials are not shown (they would pile up at an end)
    if (q.freq < keyFrequency(0) * 0.97f || q.freq > keyFrequency(kKeys - 1) * 1.03f) return false;
    x = freqToStripX(fStrip, q.freq);
    y = fPartPlot.y + fPartPlot.h * (1.0f - partialHeight01(q.amp, fRef, fSettings.floorDb));
    return true;
}

void DiagPanel::drawPartials(NanoVG& vg, const Rect& plot)
{
    // faint guides at the A's (27.5 · 2^n)
    for (int n = 0; n < 8; ++n) {
        const float x = freqToStripX(fStrip, 27.5f * std::pow(2.0f, (float)n));
        vg.beginPath(); vg.rect(x - 0.5f, plot.y, 1, plot.h); vg.fillColor(theme::line); vg.fill();
        char l[16];
        std::snprintf(l, sizeof l, "A%d", n);
        vg.fillColor(theme::faint);
        vg.textAlign(NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_TOP));
        vg.text(x + 3, plot.y + 2, l, nullptr);
    }
    const float bottom = plot.y + plot.h;
    const uint32_t nv = std::min<uint32_t>(fShown->voices, PE_TELEMETRY_VOICES);
    for (uint32_t v = 0; v < nv; ++v) { // loudest first
        Color c = voiceColor((int)v);
        c.alpha = 0.85f;
        for (int p = 0; p < std::min<int>(fShown->voice[v].count, PE_TELEMETRY_PARTIALS); ++p) {
            float x, y;
            if (!partialPos((int)v, p, x, y) || y >= bottom) continue;
            const uint32_t kind = fShown->voice[v].partials[p].kind;
            const float w = (kind & 1u) ? 1.0f : 2.0f; // horizontal polarization: thinner
            vg.beginPath();
            vg.rect(x - w / 2, y, w, bottom - y);
            if (kind & 2u) { vg.strokeWidth(1.0f); vg.strokeColor(c); vg.stroke(); } // false-beat copy: hollow
            else { vg.fillColor(c); vg.fill(); }
        }
    }
}

int DiagPanel::nearest(float hx, float hy, float& px, float& py) const
{
    if (!fAvailable || !fHaveFrame || hx < 0) return kNearestNone;
    const Rect& plot = view() == 0 ? fSpecPlot : fPartPlot;
    if (!plot.contains(hx, hy) && !(view() == 0 && hx >= plot.x && hx < plot.x + plot.w + kHoverRadius && hy >= plot.y && hy < plot.y + plot.h))
        return kNearestNone;
    int best = kNearestNone;
    float bestD = kHoverRadius * kHoverRadius;
    const uint32_t nv = std::min<uint32_t>(fShown->voices, PE_TELEMETRY_VOICES);
    for (uint32_t v = 0; v < nv; ++v)
        for (int p = 0; p < std::min<int>(fShown->voice[v].count, PE_TELEMETRY_PARTIALS); ++p) {
            float x, y;
            if (!partialPos((int)v, p, x, y)) continue;
            // spectrogram: the dot; partials: the whole bar (from its top down to the bottom), not just its tip
            const float dy = view() == 1 && hy > y ? 0.0f : y - hy;
            const float d = (x - hx) * (x - hx) + dy * dy;
            if (d <= bestD) { bestD = d; best = (int)v * 64 + p; px = x; py = y; }
        }
    return best;
}

void DiagPanel::drawHover(NanoVG& vg)
{
    float px = 0, py = 0;
    const int t = nearest(fHoverX, fHoverY, px, py);
    if (t < 0) return;
    const PeTelemetryVoice& v = fShown->voice[t / 64];
    const PeTelemetryPartial& q = v.partials[t % 64];
    // the nominal fundamental of the voice's key at the concert pitch (the listed partials are only the
    // strongest ones, the fundamental may not be among them); a 4' voice (reg > 0) lands on n = 2
    int n = 1;
    const float cents = hoverCents(q.freq, v.key, fConcertPitch, n);
    const float t60 = q.decay > 0 ? 6.91f / q.decay : INFINITY;
    char key[8], reg[24] = "";
    keyNameTo(v.key, key, sizeof key);
    if (v.reg > 0) std::snprintf(reg, sizeof reg, " reg %d", (int)v.reg);
    char text[256];
    std::snprintf(text, sizeof text, "%s (key)%s  f = %.1f Hz  %+.1f \xC2\xA2 from harmonic %d  T60 = %.1f s  %s  %.1f dB%s",
                  key, reg, (double)q.freq, (double)cents, n, (double)t60,
                  (q.kind & 1u) ? "horizontal" : "vertical", (double)amplitudeDb(q.amp, fRef), (q.kind & 2u) ? "  (false-beat copy)" : "");
    // the marker on the partial, the tooltip near the pointer (inside the panel)
    vg.beginPath(); vg.circle(px, py, 4.5f); vg.strokeWidth(1.5f); vg.strokeColor(theme::text); vg.stroke();
    vg.fontSize(theme::smallFont);
    DGL_NAMESPACE::Rectangle<float> b;
    const float w = vg.textBounds(0, 0, text, nullptr, b) + 12, h = 20;
    float x = std::min(fHoverX + 12, fArea.x + fArea.w - w - 4), y = fHoverY + 14;
    x = std::max(fArea.x + 4, x);
    if (y + h > fArea.y + fArea.h) y = fHoverY - h - 8;
    vg.beginPath(); vg.roundedRect(x, y, w, h, theme::radius); vg.fillColor(Color(20, 22, 25, 1.0f)); vg.fill();
    vg.strokeWidth(1); vg.strokeColor(theme::line); vg.stroke();
    vg.fillColor(theme::text);
    vg.textAlign(NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE));
    vg.text(x + 6, y + h / 2, text, nullptr);
}

void DiagPanel::settingsChanged(bool clear)
{
    if (clear) { // the new range/floor: the retained columns are drawn again, the transform keeps running
        fWave.remap(fSettings.lo, fSettings.hi, fSettings.floorDb);
        fImageDirty = true;
    }
    fChanged();
    fRepaint();
}

bool DiagPanel::mouse(float x, float y)
{
    if (!fOpen || !fArea.contains(x, y)) return false;
    for (int i = 0; i < 2; ++i)
        if (fViewTab[i].contains(x, y) && fSettings.view != i) { fSettings.view = i; fHoverTarget = kNearestNone; settingsChanged(false); }
    if (fRangeBtn.contains(x, y)) {
        int i = 0;
        while (i < 5 && !(kRanges[i][0] == fSettings.lo && kRanges[i][1] == fSettings.hi)) ++i;
        i = i >= 5 ? 0 : (i + 1) % 5; // a range not in the list (a saved one): back to the first
        fSettings.lo = kRanges[i][0];
        fSettings.hi = kRanges[i][1];
        settingsChanged(true);
    }
    if (fFloorBtn.contains(x, y)) {
        int i = 0;
        while (i < 5 && kFloors[i] != fSettings.floorDb) ++i;
        fSettings.floorDb = kFloors[i >= 5 ? 0 : (i + 1) % 5];
        settingsChanged(true);
    }
    if (fFreezeBtn.contains(x, y)) { fFreeze = !fFreeze; fRepaint(); } // not persisted: the next open is live
    return true;
}

void DiagPanel::hover(float x, float y)
{
    const bool was = fHoverX >= 0;
    fHoverX = x;
    fHoverY = y;
    float px, py;
    const int t = nearest(x, y, px, py);
    // a tooltip follows the pointer; repaint when it appears, moves or goes
    if (t != fHoverTarget || t >= 0 || (was && x < 0)) { fHoverTarget = t; fRepaint(); }
}

} // namespace pe
