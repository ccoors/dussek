#include "Widgets.hpp"
#include "Theme.hpp"

#include <algorithm>

namespace pe {
using DGL_NAMESPACE::NanoVG;

namespace {
void label(NanoVG& vg, float x, float y, const char* s, const DGL_NAMESPACE::Color& c, NanoVG::Align a)
{
    if (s == nullptr || s[0] == '\0')
        return; // nothing to draw (NanoVG asserts on an empty string): the About tab's blank lines
    vg.fillColor(c);
    vg.textAlign(a);
    vg.text(x, y, s, nullptr);
}
}

void drawSlider(NanoVG& vg, const Rect& r, const ControlLook& c)
{
    const auto& tc = c.active ? theme::text : theme::faint;
    vg.fontSize(theme::fontSize);
    label(vg, r.x, r.y + r.h * 0.32f, c.label, tc, NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE));

    // The value is also an editable field. Reveal its boundary on hover so that
    // the click-to-type behavior is visible without turning every row into a box.
    const float valueW = std::min(r.w * 0.34f, 132.0f);
    if (c.hot || c.editing) {
        vg.beginPath();
        vg.roundedRect(r.x + r.w - valueW, r.y + 1, valueW, r.h * 0.56f, theme::radius);
        vg.fillColor(theme::panel);
        vg.fill();
        vg.strokeColor(c.editing ? theme::accent : theme::line);
        vg.strokeWidth(1);
        vg.stroke();
    }
    const std::string& v = c.editing && c.editText ? *c.editText : c.value;
    const auto& valueColor = c.editing ? theme::accentText : c.active ? (c.hot ? theme::accentText : theme::text) : theme::faint;
    label(vg, r.x + r.w - 8, r.y + r.h * 0.32f, (c.editing ? v + "|" : v).c_str(), valueColor,
        NanoVG::Align(NanoVG::ALIGN_RIGHT | NanoVG::ALIGN_MIDDLE));

    const float ty = r.y + r.h * 0.78f;
    const float trackH = c.hot ? theme::track + 1.5f : theme::track;
    vg.beginPath();
    vg.roundedRect(r.x, ty - trackH / 2, r.w, trackH, trackH / 2);
    vg.fillColor(theme::line);
    vg.fill();
    vg.beginPath();
    vg.roundedRect(r.x, ty - trackH / 2, r.w * c.norm, trackH, trackH / 2);
    vg.fillColor(c.active ? theme::accent : theme::marker);
    vg.fill();
    const float knobX = r.x + r.w * c.norm;
    const float knobR = c.hot ? theme::knob + 1.5f : theme::knob;
    if (c.hot) {
        vg.beginPath();
        vg.circle(knobX, ty, knobR + 2);
        vg.strokeColor(theme::accentText);
        vg.strokeWidth(1);
        vg.stroke();
    }
    vg.beginPath();
    vg.circle(knobX, ty, knobR);
    vg.fillColor(c.active ? (c.hot ? theme::accentText : theme::accent) : theme::marker);
    vg.fill();
}

void drawToggle(NanoVG& vg, const Rect& r, const ControlLook& c)
{
    const bool on = c.norm >= 0.5f;
    vg.beginPath();
    vg.roundedRect(r.x, r.y, r.w, r.h, theme::radius);
    vg.fillColor(on ? theme::accent : theme::panel);
    vg.fill();
    vg.fontSize(theme::fontSize);
    label(vg, r.x + r.w / 2, r.y + r.h / 2, c.label, on ? theme::text : theme::dim, NanoVG::Align(NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE));
}

void drawDropdown(NanoVG& vg, const Rect& r, const ControlLook& c)
{
    vg.beginPath();
    vg.roundedRect(r.x, r.y, r.w, r.h, theme::radius);
    vg.fillColor(c.hot ? theme::line : theme::panel);
    vg.fill();
    vg.fontSize(theme::fontSize);
    const std::string s = c.value + "  ▾";
    label(vg, r.x + 10, r.y + r.h / 2, s.c_str(), c.active ? theme::text : theme::faint, NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE));
}

void drawDropdownList(NanoVG& vg, const Rect& l, const std::vector<std::string>& items, int selected, int hot, float scroll)
{
    vg.beginPath();
    vg.roundedRect(l.x, l.y, l.w, l.h, theme::radius);
    vg.fillColor(theme::panel);
    vg.fill();
    vg.strokeColor(theme::line);
    vg.stroke();
    vg.save();
    vg.intersectScissor(l.x, l.y, l.w, l.h);
    vg.fontSize(theme::fontSize);
    const size_t first = (size_t)std::max(0.0f, scroll / kRowH);
    for (size_t i = first; i < items.size(); ++i) {
        const float y = l.y + i * kRowH - scroll;
        if (y >= l.y + l.h)
            break;
        if ((int)i == hot) {
            vg.beginPath();
            vg.rect(l.x, y, l.w, kRowH);
            vg.fillColor(theme::line);
            vg.fill();
        }
        label(vg, l.x + 10, y + kRowH / 2, items[i].c_str(), (int)i == selected ? theme::accentText : theme::text,
            NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_MIDDLE));
    }
    const ListBar b = listBar(l, (uint32_t)items.size(), scroll);
    if (b.track.w > 0) {
        vg.beginPath();
        vg.rect(b.track.x, b.track.y, b.track.w, b.track.h);
        vg.fillColor(theme::panel);
        vg.fill();
        vg.beginPath();
        vg.roundedRect(b.thumb.x + 2, b.thumb.y + 2, b.thumb.w - 4, b.thumb.h - 4, (b.thumb.w - 4) / 2);
        vg.fillColor(theme::marker);
        vg.fill();
    }
    vg.restore();
}
}
