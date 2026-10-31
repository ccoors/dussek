#include "Panel.hpp"
#include "Morph.hpp"
#include "Theme.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cctype>
#include <string>

namespace pe {
using DGL_NAMESPACE::NanoVG;

namespace {
void label(NanoVG& vg, float x, float y, const char* s, const DGL_NAMESPACE::Color& c, int align)
{
    if (s == nullptr || s[0] == '\0')
        return; // nothing to draw (NanoVG asserts on an empty string): the About tab's blank lines
    vg.fillColor(c);
    vg.textAlign(NanoVG::Align(align));
    vg.text(x, y, s, nullptr);
}
constexpr float kTipPad = 8, kTipGap = 4;
}

void drawTabBar(NanoVG& vg, const Layout& l, Tab active, int hot)
{
    if (l.tabs.empty())
        return;
    const Rect& last = l.tabs.back().rect;
    vg.beginPath();
    vg.moveTo(l.content.x, last.y + last.h);
    vg.lineTo(l.content.x + l.content.w, last.y + last.h);
    vg.strokeColor(theme::line);
    vg.strokeWidth(1);
    vg.stroke();
    vg.fontSize(theme::fontSize);
    for (const TabButton& t : l.tabs) {
        const bool on = t.tab == active;
        if (on || (int)t.tab == hot) {
            vg.beginPath();
            vg.roundedRect(t.rect.x, t.rect.y, t.rect.w, t.rect.h, theme::radius);
            vg.fillColor(on ? theme::panel : theme::line);
            vg.fill();
        }
        if (on) { // the selected tab's mark
            vg.beginPath();
            vg.rect(t.rect.x + 4, t.rect.y + t.rect.h - 2, t.rect.w - 8, 2);
            vg.fillColor(theme::accent);
            vg.fill();
        }
        label(vg, t.rect.x + t.rect.w / 2, t.rect.y + t.rect.h / 2, tabName(t.tab), on ? theme::accentText : theme::dim,
            NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE);
    }
}

void drawSubheading(NanoVG& vg, const Subheading& s)
{
    std::string up(s.title);
    for (char& ch : up)
        ch = (char)std::toupper((unsigned char)ch);
    vg.fontSize(theme::smallFont);
    label(vg, s.rect.x + 4, s.rect.y + s.rect.h * 0.6f, up.c_str(), theme::faint, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
}

void drawEmptyNote(NanoVG& vg, const Rect& r)
{
    vg.fontSize(theme::fontSize);
    label(vg, r.x + 4, r.y + r.h / 2, "Nothing here applies to this instrument.", theme::dim, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
}

void drawPlainButton(NanoVG& vg, const Rect& b, const char* text, bool enabled, bool hot)
{
    vg.beginPath();
    vg.roundedRect(b.x + 1, b.y + 2, b.w - 2, b.h - 4, theme::radius);
    vg.fillColor(hot && enabled ? theme::line : theme::panel);
    vg.fill();
    vg.fontSize(theme::fontSize);
    label(vg, b.x + b.w / 2, b.y + b.h / 2, text, enabled ? theme::text : theme::faint, NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE);
}

void drawMorphRow(NanoVG& vg, const MorphRow& r, const std::string& name, float share, bool hot)
{
    if (hot) {
        vg.beginPath();
        vg.roundedRect(r.rect.x, r.rect.y, r.rect.w, r.rect.h, theme::radius);
        vg.fillColor(theme::panel);
        vg.fill();
    }
    const float my = r.rect.y + r.rect.h / 2;
    const float buttons = r.up.w > 0 ? r.up.x : r.rect.x + r.rect.w;
    const float shareX = buttons - 8;
    char pct[16];
    std::snprintf(pct, sizeof(pct), "%d %%", (int)std::lround(share * 100));
    vg.fontSize(theme::fontSize);
    label(vg, shareX, my, pct, share > 0 ? theme::text : theme::faint, NanoVG::ALIGN_RIGHT | NanoVG::ALIGN_MIDDLE);
    vg.save();
    vg.intersectScissor(r.rect.x, r.rect.y, std::max(1.0f, shareX - 48 - r.rect.x), r.rect.h);
    label(vg, r.rect.x + 4, my, name.c_str(), r.index == 0 ? theme::text : theme::dim, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
    vg.restore();
    if (r.up.w > 0) {
        drawPlainButton(vg, r.up, "↑", true, false);
        drawPlainButton(vg, r.down, "↓", true, false);
        drawPlainButton(vg, r.remove, "×", true, false);
    }
}

void drawMorphPad(NanoVG& vg, const Rect& pad, const std::vector<std::string>& labels, float x, float y, bool building, bool hot)
{
    const uint32_t n = (uint32_t)labels.size();
    vg.beginPath();
    vg.roundedRect(pad.x, pad.y, pad.w, pad.h, theme::radius);
    vg.fillColor(theme::panel);
    vg.fill();
    if (hot) {
        vg.strokeColor(theme::line);
        vg.strokeWidth(1);
        vg.stroke();
    }
    vg.fontSize(theme::smallFont);
    if (n <= 1) {
        label(vg, pad.x + pad.w / 2, pad.y + pad.h / 2, "Add an instrument to morph", theme::dim,
            NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE);
    } else {
        vg.beginPath();
        for (uint32_t i = 0; i < n; ++i) {
            const Point2 c = planeToPad(pad, morphCorner(n, i));
            if (i == 0)
                vg.moveTo(c.x, c.y);
            else
                vg.lineTo(c.x, c.y);
        }
        if (n > 2)
            vg.closePath();
        vg.strokeColor(theme::marker);
        vg.strokeWidth(1.5f);
        vg.stroke();
        for (uint32_t i = 0; i < n; ++i) {
            const Point2 u = morphCorner(n, i);
            const Point2 c = planeToPad(pad, u);
            vg.beginPath();
            vg.circle(c.x, c.y, 3);
            vg.fillColor(theme::marker);
            vg.fill();
            // the label centered over its corner (under it for the lower ones), its box inside the pad
            const std::string text = morphCornerLabel(labels[i]);
            DGL_NAMESPACE::Rectangle<float> tb;
            const float tw = vg.textBounds(0, 0, text.c_str(), nullptr, tb);
            const float lx = std::clamp(c.x - tw / 2, pad.x + 4, std::max(pad.x + 4, pad.x + pad.w - 4 - tw));
            const float ly = std::clamp(c.y + (u.y >= -0.3f ? -12.0f : 12.0f), pad.y + 8, pad.y + pad.h - 8);
            label(vg, lx, ly, text.c_str(), i == 0 ? theme::text : theme::dim, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
        }
        const Point2 p = planeToPad(pad, morphClamp(n, x, y));
        vg.beginPath();
        vg.circle(p.x, p.y, 6);
        vg.fillColor(theme::accent);
        vg.fill();
    }
    if (building)
        label(vg, pad.x + pad.w / 2, pad.y + pad.h + 11, "building…", theme::dim, NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE);
}

void drawMorphNote(NanoVG& vg, const Rect& r)
{
    vg.fontSize(theme::fontSize);
    label(vg, r.x + 4, r.y + r.h / 2, "This instrument does not morph.", theme::dim,
        NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
}

void drawAboutTab(NanoVG& vg, const Layout& l, bool hot)
{
    vg.fontSize(theme::fontSize);
    for (const AboutLine& a : l.aboutLines) {
        const DGL_NAMESPACE::Color& c = a.style == LineStyle::Dim ? theme::dim : a.style == LineStyle::Heading ? theme::accentText : theme::text;
        label(vg, a.rect.x, a.rect.y + a.rect.h / 2, a.text.c_str(), c, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
    }
    const Rect& k = l.aboutLink;
    vg.save();
    vg.intersectScissor(k.x, k.y, k.w, k.h);
    label(vg, k.x + 4, k.y + k.h / 2, kProjectUrl, theme::accentText, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
    vg.restore();
    drawPlainButton(vg, l.aboutCopy, "Copy link", true, hot);
}
void drawTieTag(NanoVG& vg, const Row& r, const char* text, const char* labelText)
{
    DGL_NAMESPACE::Rectangle<float> b;
    vg.fontSize(theme::fontSize);
    const float labelW = vg.textBounds(0, 0, labelText, nullptr, b);
    vg.fontSize(theme::fontSize - 3);
    const float w = vg.textBounds(0, 0, text, nullptr, b) + 8, h = 14;
    float x, cy;
    if (r.compact) {
        x = r.label.x + r.label.w - w - 2;
        cy = r.label.y + r.label.h / 2;
    } else if (r.kind == Kind::Slider) {
        x = r.rect.x + labelW + 8;
        cy = r.rect.y + r.rect.h * 0.32f;
    } else {
        x = r.rect.x + r.rect.w - w + 4;
        cy = r.rect.y + 2;
    }
    const float y = cy - h / 2;
    vg.beginPath();
    vg.roundedRect(x, y, w, h, 3);
    vg.fillColor(theme::bg);
    vg.fill();
    vg.strokeColor(theme::accent);
    vg.strokeWidth(1);
    vg.stroke();
    label(vg, x + w / 2, y + h / 2, text, theme::accentText, NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE);
}

void drawWaiting(NanoVG& vg, const Row& r)
{
    vg.beginPath();
    vg.roundedRect(r.rect.x + 0.5f, r.rect.y + 0.5f, r.rect.w - 1, r.rect.h - 1, theme::radius);
    vg.strokeColor(theme::accent);
    vg.strokeWidth(1.5f);
    vg.stroke();
}

void drawMidiRow(NanoVG& vg, const MidiRow& r, const char* name, const std::string& detail, const char* button, bool hot,
    bool accent)
{
    if (hot) {
        vg.beginPath();
        vg.roundedRect(r.rect.x, r.rect.y, r.rect.w, r.rect.h, theme::radius);
        vg.fillColor(theme::panel);
        vg.fill();
    }
    const float my = r.rect.y + r.rect.h / 2;
    const float detailX = r.rect.x + std::max(r.rect.w * 0.45f, std::min(180.0f, r.rect.w - r.button.w - 90));
    vg.fontSize(theme::fontSize);
    vg.save();
    vg.intersectScissor(r.rect.x, r.rect.y, detailX - 8 - r.rect.x, r.rect.h);
    label(vg, r.rect.x + 4, my, name, hot ? theme::text : theme::dim, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
    vg.restore();
    vg.save();
    vg.intersectScissor(detailX, r.rect.y, std::max(1.0f, r.button.x - 8 - detailX), r.rect.h);
    label(vg, detailX, my, detail.c_str(), theme::text, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
    vg.restore();
    const Rect& b = r.button;
    vg.beginPath();
    vg.roundedRect(b.x + 1, b.y + 2, b.w - 2, b.h - 4, theme::radius);
    vg.fillColor(theme::panel);
    vg.fill();
    if (accent) {
        vg.strokeColor(theme::accent);
        vg.strokeWidth(1);
        vg.stroke();
    }
    label(vg, b.x + b.w / 2, my, button, accent ? theme::accentText : theme::text, NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE);
}

void drawCompactRow(NanoVG& vg, const Row& r, const ControlLook& c)
{
    if (c.hot) { // the row under the pointer lights up
        vg.beginPath();
        vg.roundedRect(r.rect.x, r.rect.y, r.rect.w, r.rect.h, theme::radius);
        vg.fillColor(theme::panel);
        vg.fill();
    }
    const float my = r.rect.y + r.rect.h / 2;
    vg.fontSize(theme::fontSize);
    vg.save();
    vg.intersectScissor(r.label.x, r.label.y, r.label.w, r.label.h); // a long label is cut, not drawn over the control
    label(vg, r.label.x + 4, my, c.label, c.hot ? theme::text : theme::dim, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
    vg.restore();
    switch (r.kind) {
    case Kind::Slider: {
        const Rect& t = r.control;
        const float th = theme::track;
        vg.beginPath();
        vg.roundedRect(t.x, my - th / 2, t.w, th, th / 2);
        vg.fillColor(theme::line);
        vg.fill();
        vg.beginPath();
        vg.roundedRect(t.x, my - th / 2, t.w * c.norm, th, th / 2);
        vg.fillColor(theme::accent);
        vg.fill();
        vg.beginPath();
        vg.roundedRect(t.x + t.w * c.norm - 2, my - 7, 4, 14, 2);
        vg.fillColor(c.hot ? theme::accentText : theme::text);
        vg.fill();
        const Rect& v = r.value; // click to type: a field while typing
        if (c.editing) {
            vg.beginPath();
            vg.roundedRect(v.x, v.y + 2, v.w, v.h - 4, theme::radius);
            vg.fillColor(theme::bg);
            vg.fill();
            vg.strokeColor(theme::accent);
            vg.strokeWidth(1);
            vg.stroke();
        }
        const std::string s = c.editing && c.editText ? *c.editText + "|" : c.value;
        vg.save();
        vg.intersectScissor(v.x, v.y, v.w, v.h);
        label(vg, v.x + v.w - 4, my, s.c_str(), c.editing || c.hot ? theme::accentText : theme::text,
            NanoVG::ALIGN_RIGHT | NanoVG::ALIGN_MIDDLE);
        vg.restore();
        break;
    }
    case Kind::Toggle: {
        const bool on = c.norm >= 0.5f;
        const Rect& s = r.control;
        vg.beginPath();
        vg.roundedRect(s.x, s.y, s.w, s.h, s.h / 2);
        vg.fillColor(on ? theme::accent : theme::line);
        vg.fill();
        const float d = std::max(1.0f, s.h - 4);
        vg.beginPath();
        vg.circle(on ? s.x + s.w - 2 - d / 2 : s.x + 2 + d / 2, s.y + s.h / 2, d / 2);
        vg.fillColor(on ? theme::accentText : theme::dim);
        vg.fill();
        break;
    }
    case Kind::Dropdown: {
        const Rect& b = r.control;
        vg.beginPath();
        vg.roundedRect(b.x, b.y + 1, b.w, b.h - 2, theme::radius);
        vg.fillColor(c.editing ? theme::bg : c.hot ? theme::line : theme::panel);
        vg.fill();
        if (c.editing) { // a name typed in place (a preset's Save as…)
            vg.strokeColor(theme::accent);
            vg.strokeWidth(1);
            vg.stroke();
        }
        vg.save();
        vg.intersectScissor(b.x, b.y, b.w, b.h);
        const std::string s = c.editing && c.editText ? *c.editText + "|" : c.value + "  ▾";
        label(vg, b.x + 8, my, s.c_str(), c.editing ? theme::accentText : theme::text, NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE);
        vg.restore();
        break;
    }
    }
}

float tooltipHeight(NanoVG& vg, float width, const char* title, const char* body, const char* gestures)
{
    const float w = std::max(1.0f, width - 2 * kTipPad);
    float b[4];
    float h = 2 * kTipPad;
    vg.textAlign(NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_TOP));
    vg.fontSize(theme::fontSize);
    vg.textBoxBounds(0, 0, w, title, nullptr, b);
    h += b[3] - b[1];
    vg.fontSize(theme::smallFont);
    for (const char* part : { body, gestures }) // each below a gap; an empty one takes no room
        if (*part) {
            vg.textBoxBounds(0, 0, w, part, nullptr, b);
            h += kTipGap + b[3] - b[1];
        }
    return h;
}

void drawTooltip(NanoVG& vg, const Rect& r, const char* title, const char* body, const char* gestures)
{
    vg.beginPath();
    vg.roundedRect(r.x, r.y, r.w, r.h, theme::radius);
    vg.fillColor(theme::panel);
    vg.fill();
    vg.strokeColor(theme::accent);
    vg.strokeWidth(1);
    vg.stroke();
    const float x = r.x + kTipPad, w = std::max(1.0f, r.w - 2 * kTipPad);
    float y = r.y + kTipPad, b[4];
    vg.textAlign(NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_TOP));
    vg.fontSize(theme::fontSize);
    vg.fillColor(theme::accentText);
    vg.textBox(x, y, w, title);
    vg.textBoxBounds(x, y, w, title, nullptr, b);
    y = b[3] + kTipGap;
    vg.fontSize(theme::smallFont);
    if (*body) {
        vg.fillColor(theme::text);
        vg.textBox(x, y, w, body);
        vg.textBoxBounds(x, y, w, body, nullptr, b);
        y = b[3] + kTipGap;
    }
    vg.fillColor(theme::faint);
    if (*gestures)
        vg.textBox(x, y, w, gestures);
}

} // namespace pe
