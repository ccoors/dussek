#include "Layout.hpp"
#include "DistrhoPluginInfo.h"
#include "MidiMap.hpp"
#include "Morph.hpp"
#include "Format.hpp"
#include "Params.hpp"

#include <algorithm>
#include <cmath>
#include <string>
#include <utility>

namespace pe {

std::vector<std::string> menuItems(int zoom, bool resetArmed)
{
    std::vector<std::string> items(kMenuCount);
    items[kMenuZoom] = "Zoom: " + std::to_string(zoom) + " %  ▸";
    items[kMenuCopy] = "Copy CLI";
    items[kMenuReset] = resetArmed ? "Reset: click again to confirm" : "Reset…";
    return items;
}

Rect submenuRect(const Rect& list, int row, uint32_t count, float viewTop, float viewBottom, float viewRight)
{
    const float w = std::max(list.w, 200.0f), h = std::min(count * kRowH, std::max(viewBottom - viewTop, kRowH));
    const float x = list.x + list.w + w <= viewRight ? list.x + list.w : std::max(0.0f, list.x - w);
    const float y = std::max(viewTop, std::min(list.y + row * kRowH, viewBottom - h));
    return { x, y, w, h };
}

Rect dropdownListRect(const Rect& a, uint32_t count, float viewTop, float viewBottom, float itemsW, float viewRight)
{
    const float full = count * kRowH;
    const float below = a.y + a.h, roomBelow = viewBottom - below, roomAbove = a.y - viewTop;
    const bool down = full <= roomBelow || (full > roomAbove && roomBelow >= roomAbove);
    const float h = std::min(full, std::max(down ? roomBelow : roomAbove, kRowH));
    const float y = down ? below : a.y - h;
    const float bar = h < full ? kListBarW : 0.0f;
    const float w = std::min(std::max({ a.w, 200.0f, itemsW + 20 + bar }), std::max(viewRight, 1.0f));
    return { std::max(0.0f, std::min(a.x, viewRight - w)), y, w, h };
}

float listMaxScroll(const Rect& list, uint32_t count)
{
    return std::max(0.0f, count * kRowH - list.h);
}

int listRowAt(const Rect& list, uint32_t count, float scroll, float y)
{
    if (y < list.y || y >= list.y + list.h)
        return -1;
    const int i = (int)std::floor((y - list.y + scroll) / kRowH);
    return i >= 0 && i < (int)count ? i : -1;
}

float listScrollTo(const Rect& list, uint32_t count, int row)
{
    if (row < 0)
        return 0;
    return std::clamp((row + 0.5f) * kRowH - list.h / 2, 0.0f, listMaxScroll(list, count));
}

float listWheel(const Rect& list, uint32_t count, float scroll, float pixels)
{
    return std::clamp(scroll - pixels, 0.0f, listMaxScroll(list, count));
}

ListBar listBar(const Rect& list, uint32_t count, float scroll)
{
    const float max = listMaxScroll(list, count);
    if (max <= 0)
        return {};
    const Rect track { list.x + list.w - kListBarW, list.y, kListBarW, list.h };
    const float th = std::max(kListThumbMin, list.h * list.h / (count * kRowH));
    const float ty = list.y + (list.h - th) * std::clamp(scroll / max, 0.0f, 1.0f);
    return { track, { track.x, ty, track.w, th } };
}

float listScrollAtThumb(const Rect& list, uint32_t count, float thumbTop)
{
    const ListBar b = listBar(list, count, 0);
    const float travel = list.h - b.thumb.h;
    if (b.track.w <= 0 || travel <= 0)
        return 0;
    return listMaxScroll(list, count) * std::clamp((thumbTop - list.y) / travel, 0.0f, 1.0f);
}

// the sustain latch's toggle: this much wider than a stop's (its label)
static constexpr float kLatchExtra = 48;

Kind paramKind(uint32_t param)
{
    if (paramInfo(param).boolean)
        return Kind::Toggle;
    return paramLabelCount(param) > 0 ? Kind::Dropdown : Kind::Slider;
}

const char* tabName(Tab t)
{
    static const char* const names[kTabCount] = { "Sound", "Morph", "Touch", "Felt", "Mics", "Tuning", "Condition", "Extras", "MIDI", "About" };
    return names[t < kTabCount ? t : 0];
}

const std::vector<TabGroup>& tabGroups(Tab t)
{
    static const std::vector<TabGroup> g[kTabCount] = {
        { { nullptr, { kPresetRow, kMallet, kHammerHardness, kSustain, kBoardBass, kMotorSpeed, kMotorDepth, kBebung, kBendRange, kRoom, kCondition } } },
        {}, // Morph: the list and the pad (Morph X and Y), no rows
        { { nullptr, { kDynamics, kVariation, kRestrike } } }, // Touch: the Dynamics, the variation, the re-strike, then the velocity curve
        {}, // Felt: the felt's curves (FeltEditor), no rows
        { { nullptr, { kDistanceLevel, kLid, kLidOff, kPillow, kSpreadKeys } },
          { "Custom pair", { kMicLX, kMicLY, kMicLZ, kMicRX, kMicRY, kMicRZ, kMicLPattern, kMicLYaw, kMicLPitch,
                               kMicRPattern, kMicRYaw, kMicRPitch, kMicWall } } },
        { { nullptr, { kConcertPitch, kTemperament, kTemperamentKey, kStretch, kUnisonWidth } } },
        { { nullptr, { kWearVariant, kWearHammers, kWearVoicing, kWearAction, kWearDampers, kWearUnisons, kWearTuning,
                       kWearFalseBeats, kWearStrings, kWearSoundboard } } },
        { { "Noises", { kKnock, kRelease, kPedalNoise, kPedalRing, kSympathetic, kAudioInput, kDamperFall } },
          { "Defects", { kHammerGrooves, kDamperBuzz, kRattle, kPhantom } },
          { "Preparation", { kPrepMode, kPluckPosition, kPluckBrightness, kMuteStrength, kHarmonicNumber, kBoltMass,
                               kBoltPosition, kPaperBuzz } } },
        {}, // MIDI: MIDI learn's ties, no parameters
        {}, // About: the plugin's and the instrument's text, no parameters
    };
    return g[t < kTabCount ? t : 0];
}

const std::vector<uint32_t>& headerParams()
{
    static const std::vector<uint32_t> p { kInstrumentRow, kStop8, kStop4, kStop8b, kLute, kBuff, kMotor, kSustainLatch, kVolume };
    return p;
}

const std::vector<uint32_t>& morphParams()
{
    static const std::vector<uint32_t> p { kMorphX, kMorphY };
    return p;
}

const std::vector<uint32_t>& drawingParams()
{
    static const std::vector<uint32_t> p { kMicPosition, kMicDistance, kMicSpacing };
    return p;
}

float textWidth(const char* s)
{
    // 13 px sans: about 7 px per character (UTF-8: count the characters, not the bytes)
    int n = 0;
    for (; *s; ++s)
        if ((*s & 0xC0) != 0x80)
            ++n;
    return 7.0f * n;
}

namespace {
std::vector<uint32_t> visibleOf(const std::vector<uint32_t>& params, const UiContext& c)
{
    std::vector<uint32_t> v;
    for (uint32_t p : params)
        if (paramVisible(p, c))
            v.push_back(p);
    return v;
}

/// the label column for these rows: their longest label, within limits
float labelColumn(const std::vector<uint32_t>& params, const UiContext& ctx)
{
    const InstrumentEntry& e = ctx.inst != nullptr ? *ctx.inst : noInstrument();
    float w = 0;
    for (uint32_t p : params)
        w = std::max(w, textWidth(displayName(p, e)) + (p < ctx.tied.size() && ctx.tied.test(p) ? kTagW : 0.0f));
    return std::clamp(w + 12, 80.0f, 180.0f + kTagW);
}

/// a header row: the old widgets (a slider's label and value above its track)
Row headerRow(uint32_t p, float x, float y, float w)
{
    Row r { p, paramKind(p), { x, y, std::max(w, 1.0f), kRowH } };
    r.label = r.control = r.value = r.rect;
    if (r.kind == Kind::Slider) {
        r.label = { x, y, r.rect.w * 0.55f, kRowH * 0.55f };
        r.value = { x + r.rect.w * 0.55f, y, r.rect.w * 0.45f, kRowH * 0.55f };
        r.control = { x, y + kRowH * 0.55f, r.rect.w, kRowH * 0.45f };
    }
    return r;
}

/// a slider's value column: its longest text (the range's ends, the default; the mic distance as the
/// engine reports it, "20.00 m (limited)"), at least kValueW
float valueWidth(uint32_t p)
{
    const ParamInfo& i = paramInfo(p);
    float w = 0;
    for (float v : { i.min, i.max, i.def })
        w = std::max(w, textWidth(formatValue(p, v).c_str()));
    if (p == kMicDistance)
        for (float v : { 0.0f, 5.0f })
            w = std::max(w, textWidth(formatMicDistance(v, true, 20.0).c_str()));
    return std::max(kValueW, w + 8);
}

/// the value column for these rows: their sliders' widest (the tracks end alike)
float valueColumn(const std::vector<uint32_t>& params)
{
    float w = kValueW;
    for (uint32_t p : params)
        if (paramKind(p) == Kind::Slider)
            w = std::max(w, valueWidth(p));
    return w;
}

/// a compact row: label · control · value on one line
Row compactRow(uint32_t p, float x, float y, float w, float labelW, float valueW, bool scrolls)
{
    Row r { p, paramKind(p), { x, y, std::max(w, 1.0f), kLineH } };
    r.compact = true;
    r.scrolls = scrolls;
    labelW = std::min(labelW, r.rect.w * 0.5f);
    r.label = { x, y, labelW, kLineH };
    const float end = x + r.rect.w, cx = std::min(x + labelW + 8, end - 1);
    if (r.kind == Kind::Slider) {
        const float vw = std::min(valueW, (end - cx) * 0.5f);
        r.value = { end - vw, y, vw, kLineH };
        r.control = { cx, y, std::max(1.0f, end - vw - 8 - cx), kLineH };
    } else if (r.kind == Kind::Toggle) {
        r.control = { cx, y + 3, std::max(1.0f, std::min(34.0f, end - cx)), 16 };
        r.value = r.control;
    } else {
        r.control = { cx, y, std::max(1.0f, end - cx), kLineH };
        r.value = r.control;
    }
    return r;
}
} // namespace

Layout computeLayout(float width, float height, Tab tab, const UiContext& ctx)
{
    Layout l;
    if (tab >= kTabCount)
        tab = kTabSound;
    const float w = std::max(width, 1.0f), h = std::max(height, 1.0f);
    l.wide = width >= kBreakpoint;
    const float pad = std::min(kPad, w / 20), inner = w - 2 * pad;
    auto header = [&](uint32_t p, float x, float y, float rw) { l.rows.push_back(headerRow(p, x, y, rw)); };
    const bool buff = paramVisible(kBuff, ctx), motor = paramVisible(kMotor, ctx), latch = paramVisible(kSustainLatch, ctx);
    // the stops this instrument has (Harpsichord B all four; a measured second 8′: two)
    std::vector<uint32_t> stops;
    for (uint32_t s : { kStop8, kStop4, kStop8b, kLute })
        if (paramVisible(s, ctx))
            stops.push_back(s);
    float y = pad;

    // header: instrument, stops, latch, volume, panic, the menu
    const float panicW = std::min(64.0f, inner / 6), menuW = std::min(40.0f, inner / 8);
    l.menu = { pad + inner - menuW, y, menuW, kRowH };
    l.panic = { l.menu.x - kGap - panicW, y, panicW, kRowH };
    const float right = l.panic.x; // the header's controls end before it
    // wide: one row, Volume keeping at least kVolMin, the toggles their labels' width (only the
    // instrument menu narrows, by up to 60 px); where that does not fit (Harpsichord B's four
    // stops, Buff and the latch just above the breakpoint), two rows as in the narrow layout
    const size_t first = l.rows.size();
    bool twoRows = !l.wide;
    const float kVolMin = 140, kProfMin = 120;
    if (l.wide) {
        const float n = (float)stops.size() + (buff ? 1 : 0) + (motor ? 1 : 0) + (latch ? 1 : 0);
        const float avail = right - kGap - pad;
        const float sw = 56, extra = latch ? kLatchExtra : 0;
        float profW = 240;
        profW -= std::max(0.0f, std::min(60.0f, profW + kGap + n * (sw + kGap) + extra + kVolMin - avail));
        float x = pad;
        header(kInstrumentRow, x, y, profW); x += profW + kGap;
        for (uint32_t s : stops) { header(s, x, y, sw); x += sw + kGap; }
        if (buff) { header(kBuff, x, y, sw); x += sw + kGap; }
        if (motor) { header(kMotor, x, y, sw); x += sw + kGap; }
        if (latch) { header(kSustainLatch, x, y, sw + extra); x += sw + extra + kGap; }
        const float volW = std::min(240.0f, right - kGap - x);
        if (volW >= kVolMin) {
            header(kVolume, right - kGap - volW, y, volW);
            y += kRowH + pad;
        } else {
            l.rows.resize(first);
            twoRows = true;
        }
    }
    if (twoRows) {
        // the toggles and Volume below the instrument; where Volume would get under kVolMin, the
        // last toggles go up beside the instrument (as long as it keeps kProfMin), and where even
        // that does not do, Volume takes a row of its own
        const float sw = std::min(56.0f, inner / 8);
        std::vector<std::pair<uint32_t, float>> toggles;
        for (uint32_t s : stops) toggles.push_back({ s, sw });
        if (buff) toggles.push_back({ kBuff, sw });
        if (motor) toggles.push_back({ kMotor, sw });
        if (latch) toggles.push_back({ kSustainLatch, sw + kLatchExtra });
        float below = 0; // the toggles' width on the second row
        for (const auto& t : toggles) below += t.second + kGap;
        float profW = right - kGap - pad;
        size_t up = toggles.size(); // toggles[up …] sit beside the instrument
        while (up > 0 && inner - below < kVolMin && profW - toggles[up - 1].second - kGap >= kProfMin) {
            --up;
            profW -= toggles[up].second + kGap;
            below -= toggles[up].second + kGap;
        }
        header(kInstrumentRow, pad, y, profW);
        float x = pad + profW + kGap;
        for (size_t i = up; i < toggles.size(); ++i) { header(toggles[i].first, x, y, toggles[i].second); x += toggles[i].second + kGap; }
        y += kRowH + kGap;
        x = pad;
        for (size_t i = 0; i < up; ++i) { header(toggles[i].first, x, y, toggles[i].second); x += toggles[i].second + kGap; }
        if (x > pad && pad + inner - x < kVolMin) {
            y += kRowH + kGap;
            x = pad;
        }
        header(kVolume, x, y, pad + inner - x);
        y += kRowH + pad;
    }

    const std::vector<uint32_t> mic = visibleOf(drawingParams(), ctx);
    const float line = kLineH + kLineGap;
    float panelX = pad, panelW = inner;
    // the tab bar's width on one row
    float tabsW = 0;
    for (uint32_t t = 0; t < kTabCount; ++t)
        tabsW += textWidth(tabName((Tab)t)) + 4 + (t > 0 ? 2 : 0);
    if (l.wide) {
        // the drawing and its rows stay put on the left; short windows shrink the views so the rows
        // stay in view. The drawing takes 55 % of the width, down to half where that puts the tabs
        // on one row (the default window)
        float share = 0.55f;
        if (const float one = 1 - (tabsW + 0.5f) / (inner - pad); one < share && one >= 0.5f)
            share = one;
        const float micW = (inner - pad) * share;
        const float full = std::min(micW * 0.45f, 260.0f), room = h - pad - y - kGap - mic.size() * line + kLineGap;
        const float viewH = std::max(std::min(full, room), 1.0f);
        const float topW = (micW - kGap) * 0.58f;
        l.topView = { pad, y, topW, viewH };
        l.frontView = { pad + topW + kGap, y, micW - topW - kGap, viewH };
        float my = y + viewH + kGap;
        const float lw = labelColumn(mic, ctx), vw = valueColumn(mic);
        for (uint32_t p : mic) { l.rows.push_back(compactRow(p, pad, my, micW, lw, vw, false)); my += line; }
        panelX = pad + micW + pad;
        panelW = inner - micW - pad;
    }

    // the tab bar (wraps where the five do not fit)
    float tx = panelX, ty = y;
    for (uint32_t t = 0; t < kTabCount; ++t) {
        const float bw = std::max(1.0f, std::min(textWidth(tabName((Tab)t)) + 4, panelW)); // ten fit one row at the default width
        if (tx > panelX && tx + bw > panelX + panelW) {
            tx = panelX;
            ty += kTabH + 2;
        }
        l.tabs.push_back({ (Tab)t, { tx, ty, bw, kTabH } });
        tx += bw + 2;
    }
    const float contentY = ty + kTabH + kGap;
    l.content = { panelX, contentY, std::max(panelW, 1.0f), std::max(1.0f, h - pad - contentY) };

    // the tab's content, in layout coordinates
    float cy = contentY;
    std::vector<TabGroup> blocks;
    std::vector<uint32_t> all;
    for (const TabGroup& g : tabGroups(tab)) {
        std::vector<uint32_t> v = visibleOf(g.params, ctx);
        if (v.empty())
            continue;
        all.insert(all.end(), v.begin(), v.end());
        blocks.push_back({ g.title, std::move(v) });
    }
    if (!l.wide && tab == kTabMics) { // narrow: the drawing and its rows head the Mics tab
        const float topH = std::min(inner * 0.6f, 280.0f), frontH = std::min(inner * 0.35f, 160.0f);
        l.topView = { pad, cy, inner, std::max(topH, 1.0f) };
        cy += topH + kGap;
        l.frontView = { pad, cy, inner, std::max(frontH, 1.0f) };
        cy += frontH + kGap;
        l.drawingScrolls = true;
        all.insert(all.end(), mic.begin(), mic.end()); // their labels line up with the tab's
        const float lw = labelColumn(all, ctx), vw = valueColumn(all);
        for (uint32_t p : mic) { l.rows.push_back(compactRow(p, pad, cy, inner, lw, vw, true)); cy += line; }
        cy += kGap;
    }
    const int byWidth = l.wide ? std::clamp((int)(panelW / kColumnWidth), 1, kMaxColumns) : 1;
    l.columns = std::max(1, std::min(byWidth, (int)blocks.size()));
    const int cols = l.columns;
    const float colW = std::max((panelW - (cols - 1) * pad) / cols, 1.0f);
    const float lw = labelColumn(all, ctx), vw = valueColumn(all);
    std::vector<float> colY(cols, cy);
    for (const TabGroup& b : blocks) { // groups stay whole; each goes into the shortest column
        const int c = (int)(std::min_element(colY.begin(), colY.end()) - colY.begin());
        const float x = panelX + c * (colW + pad);
        float& by = colY[c];
        if (by > cy)
            by += kGap;
        if (b.title != nullptr) {
            l.subheadings.push_back({ b.title, { x, by, colW, kSubheadH } });
            by += kSubheadH;
        }
        for (uint32_t p : b.params) { l.rows.push_back(compactRow(p, x, by, colW, lw, vw, true)); by += line; }
    }
    float bottom = *std::max_element(colY.begin(), colY.end());
    if (tab == kTabTouch) {
        const float ty = blocks.empty() ? cy : bottom + kGap; // under the Dynamics row, when it shows
        const float gx = panelX + kPad + kCurveLabelW, gw = std::max(panelW - 2 * kPad - kCurveLabelW, 1.0f);
        const float bw = std::min(textWidth("Reset curve") + 20, gw);
        l.resetCurve = { gx + gw - bw, ty, bw, kLineH };
        const float histH = 2 * kLineH;
        const float gh = std::max(160.0f, std::min(0.6f * gw, l.content.h - (ty - cy) - histH - 3 * kPad - kLineH));
        l.curve = { gx, ty + kLineH + kGap, gw, gh };
        l.history = { gx, l.curve.y + gh + kLineH + kGap, gw, histH };
        bottom = l.history.y + histH + kLineH + kLineGap; // and its caption under it
    } else if (tab == kTabMidi) {
        const float x = panelX, w = std::max(panelW, 1.0f);
        const float removeW = kLineH, learnW = std::min(textWidth("learning…") + 16, w); // its text while it waits
        float y = cy;
        auto row = [&](int tie, uint32_t target, float bw) {
            l.midiRows.push_back({ tie, target, { x, y, w, kLineH }, { x + w - bw, y, bw, kLineH } });
            y += line;
        };
        for (uint32_t i = 0; i < ctx.midiTies; ++i)
            row((int)i, kParamCount, removeW); // the target: the UI's, from the map
        if (!ctx.nextTied) row(-1, kTargetNext, learnW);
        if (!ctx.previousTied) row(-1, kTargetPrevious, learnW);
        if (!ctx.knobTied) row(-1, kTargetKnob, learnW);
        y += kGap;
        l.programSwitch = { x, y, w, kLineH };
        y += line;
        l.takeoverSwitch = { x, y, w, kLineH };
        y += line;
        if (ctx.nativeAudio) {
            y += kGap;
            l.subheadings.push_back({ "Audio and MIDI", { x, y, w, kSubheadH } });
            y += kSubheadH;
            float cx = x + std::min(textWidth("Buffer") + 16, w * 0.3f);
            for (int i = 0; i < kBufferSizeCount; ++i) {
                const float cw = std::min(textWidth("1024") + 16, std::max(1.0f, (x + w - cx) / (kBufferSizeCount - i) - kGap));
                l.bufferChips[i] = { cx, y, cw, kLineH };
                cx += cw + kGap;
            }
            y += line;
            const float bw = std::min(textWidth("Rescan") + 20, w);
            l.rescanMidi = { x, y, w, kLineH };
            l.rescanButton = { x + w - bw, y, bw, kLineH };
            y += line;
        }
        bottom = y;
    } else if (tab == kTabMorph) {
        const float x = panelX, w = std::max(panelW, 1.0f);
        if (ctx.inst == nullptr || ctx.inst->info.morph_family == 0) {
            l.morphNote = { x, cy, w, kLineH };
            bottom = cy + line;
        } else {
            const bool beside = w >= 520; // the list left of the pad, else above it
            const float listW = beside ? std::min(280.0f, w * 0.45f) : w;
            const uint32_t n = 1 + std::min(ctx.partners, kMorphMax - 1);
            float y = cy;
            for (uint32_t i = 0; i < n; ++i) {
                MorphRow r { (int)i, { x, y, listW, kLineH }, {}, {}, {} };
                if (i > 0) {
                    r.remove = { x + listW - kLineH, y, kLineH, kLineH };
                    r.down = { r.remove.x - kLineH, y, kLineH, kLineH };
                    r.up = { r.down.x - kLineH, y, kLineH, kLineH };
                }
                l.morphRows.push_back(r);
                y += line;
            }
            l.morphFull = n >= kMorphMax;
            l.morphPlus = { x, y + kGap, std::min(listW, 140.0f), kLineH };
            const float listBottom = l.morphPlus.y + kLineH + kLineGap;
            const float side = beside ? std::max(160.0f, std::min({ 320.0f, w - listW - 2 * kPad, l.content.h - 2 * line }))
                                      : std::min(w, 300.0f);
            l.morphPad = beside ? Rect { x + w - side, cy, side, side } : Rect { x, listBottom + kGap, side, side };
            bottom = std::max(listBottom, l.morphPad.y + side + line); // and a line under the pad
        }
    } else if (tab == kTabFelt) {
        const float x = panelX, w = std::max(panelW, 1.0f);
        if (ctx.inst == nullptr || !ctx.inst->info.hammer_piano) {
            l.feltNote = { x, cy, w, kLineH };
            bottom = cy + line;
        } else {
            static const char* const chips[4] = { "Stiffness", "Exponent", "Core force", "Core depth" };
            float cx = x;
            for (int i = 0; i < 4; ++i) {
                const float cw = std::min(textWidth(chips[i]) + 16, w);
                l.feltChips[i] = { cx, cy, cw, kLineH };
                cx += cw + kGap;
            }
            const float ow = textWidth("Original") + 20, rw = textWidth("Reset") + 20;
            float by = cy; // Original and Reset right of the chips, or on the next row where they do not fit
            if (cx + ow + kGap + rw > x + w)
                by = cy + line;
            l.feltReset = { x + w - rw, by, rw, kLineH };
            l.feltOriginal = { l.feltReset.x - kGap - ow, by, ow, kLineH };
            const float gy = by + kLineH + kGap, gx = x + kFeltLabelW, gw = std::max(w - kFeltLabelW, 1.0f);
            const float smallH = 70;
            const bool across = gw >= 3 * 160 + 2 * kGap; // the small graphs side by side, else stacked
            const float smallsH = across ? smallH : 3 * smallH + 2 * kGap;
            const float gh = std::max(160.0f, std::min(0.5f * gw, l.content.h - (gy - cy) - 2 * kLineH - smallsH - 2 * kGap));
            l.feltGraph = { gx, gy, gw, gh };
            l.feltNote = { gx, gy + gh + kLineH, gw, kLineH }; // under the graph's key labels
            const float sy = l.feltNote.y + kLineH + kGap;
            const float sw = across ? (gw - 2 * kGap) / 3 : gw;
            for (int i = 0; i < 3; ++i)
                l.feltSmall[i] = across ? Rect { gx + i * (sw + kGap), sy, sw, smallH } : Rect { gx, sy + i * (smallH + kGap), sw, smallH };
            bottom = l.feltSmall[2].y + smallH + kLineGap;
        }
    } else if (tab == kTabAbout) {
        const float x = panelX, w = std::max(panelW, 1.0f), tw = std::max(w - 8, 1.0f); // the text 4 px in from each side
        float y = cy;
        auto lines = [&](const std::vector<TextLine>& text, LineStyle style, LineStyle heading) {
            for (const TextLine& t : wrapLines(text, tw)) {
                l.aboutLines.push_back({ t.text, { x + 4, y, tw, kTextLineH }, t.heading ? heading : style });
                y += kTextLineH;
            }
        };
        auto line = [&](const std::string& text, LineStyle style) { lines({ { text, false } }, style, style); };
        auto subheading = [&](const char* title) {
            y += kGap;
            l.subheadings.push_back({ title, { x, y, w, kSubheadH } });
            y += kSubheadH + kLineGap;
        };
        line(std::string(DISTRHO_PLUGIN_NAME) + " " + versionText(), LineStyle::Title);
        line(std::string("Built from commit ") + buildCommit(), LineStyle::Dim);
        line(kPluginLicenseLine, LineStyle::Dim);
        // the link, its button at the right end (under it where they do not fit)
        y += kLineGap;
        const float bw = std::min(textWidth("Copy link") + 20, w), urlW = textWidth(kProjectUrl) + 8;
        l.aboutLink = { x, y, std::max(1.0f, std::min(urlW, w)), kLineH };
        if (urlW + kGap + bw <= w) {
            l.aboutCopy = { x + w - bw, y, bw, kLineH };
        } else {
            y += kLineH + kLineGap;
            l.aboutCopy = { x, y, bw, kLineH };
        }
        y += kLineH;
        subheading("Instrument");
        if (ctx.inst == nullptr) {
            line("The chosen instrument is missing.", LineStyle::Text);
        } else {
            line(ctx.inst->name, LineStyle::Title);
            line(ctx.inst->dir, LineStyle::Dim);
            y += kTextLineH / 2;
            if (ctx.license != nullptr && !ctx.license->empty())
                lines(*ctx.license, LineStyle::Text, LineStyle::Heading);
            else
                line("No LICENSE.md in its folder: its terms are unknown.", LineStyle::Text);
        }
        bottom = y + kLineGap; // the last line's gap, taken off below
    } else if (blocks.empty() && !l.drawingScrolls) {
        l.emptyNote = { panelX, cy, std::max(panelW, 1.0f), kLineH };
        bottom = cy + line;
    }
    l.contentHeight = std::max(1.0f, bottom - kLineGap - contentY);
    return l;
}

bool inContent(const Layout& l, float x, float y) { return l.content.contains(x, y); }

void toLayout(const Layout& l, float scroll, bool in, float& x, float& y)
{
    (void)l;
    (void)x;
    if (in)
        y += scroll;
}

int rowAt(const Layout& l, float x, float y, float scroll)
{
    const bool in = inContent(l, x, y);
    const float ly = in ? y + scroll : y;
    for (size_t i = 0; i < l.rows.size(); ++i)
        if (l.rows[i].scrolls == in && l.rows[i].rect.contains(x, ly))
            return (int)i;
    return -1;
}

int viewAt(const Layout& l, float x, float y, float scroll)
{
    if (l.drawingScrolls) {
        if (!inContent(l, x, y))
            return -1;
        y += scroll;
    }
    return l.topView.contains(x, y) ? 0 : l.frontView.contains(x, y) ? 1 : -1;
}

Tab tabAt(const Layout& l, float x, float y)
{
    for (const TabButton& t : l.tabs)
        if (t.rect.contains(x, y))
            return t.tab;
    return kTabCount;
}

Rect shown(const Rect& r, bool scrolls, float scroll) { return scrolls ? Rect { r.x, r.y - scroll, r.w, r.h } : r; }

float maxScroll(const Layout& l) { return std::max(0.0f, l.contentHeight - l.content.h); }

float wheelScroll(const Layout& l, float x, float y, float scroll, float pixels)
{
    if (!inContent(l, x, y))
        return scroll;
    return std::clamp(scroll - pixels, 0.0f, maxScroll(l));
}

} // namespace pe
