#include "MicView.hpp"
#include "Theme.hpp"

#include <cmath>

namespace pe {
using DGL_NAMESPACE::NanoVG;

namespace {
void rectWorld(NanoVG& vg, const ViewTransform& t, const double q[4])
{
    const Point2 a = t.toScreen(q[0], q[1]), b = t.toScreen(q[2], q[3]);
    vg.rect(std::fmin(a.x, b.x), std::fmin(a.y, b.y), std::fabs(b.x - a.x), std::fabs(b.y - a.y));
}

/// a microphone with a pattern: its aim arrow and its polar glyph r(θ) = |(1−p) + p·cos θ| (12 px), turned to the aim
void drawAim(NanoVG& vg, const PeMicGeometry& g, View v, const ViewTransform& t, int i, Point2 m)
{
    const double p = g.pattern[i];
    const Point2 tip = aimTip(g, t, v, i, kAimLen);
    const float ux = (tip.x - m.x) / kAimLen, uy = (tip.y - m.y) / kAimLen; // the aim on screen, unit
    vg.beginPath();
    for (int k = 0; k <= 64; ++k) {
        const double th = k * (2 * 3.14159265358979323846 / 64);
        const double rr = 12 * std::fabs((1 - p) + p * std::cos(th));
        const float c = (float)(rr * std::cos(th)), s = (float)(rr * std::sin(th));
        const float x = m.x + c * ux - s * uy, y = m.y + c * uy + s * ux;
        k == 0 ? vg.moveTo(x, y) : vg.lineTo(x, y);
    }
    vg.strokeColor(theme::accent); vg.strokeWidth(1); vg.stroke();
    vg.beginPath(); vg.moveTo(m.x, m.y); vg.lineTo(tip.x, tip.y);
    vg.strokeColor(theme::accent); vg.strokeWidth(1.5f); vg.stroke();
    vg.beginPath(); // the head
    vg.moveTo(tip.x + 3 * ux, tip.y + 3 * uy);
    vg.lineTo(tip.x - 4 * ux - 3.5f * uy, tip.y - 4 * uy + 3.5f * ux);
    vg.lineTo(tip.x - 4 * ux + 3.5f * uy, tip.y - 4 * uy - 3.5f * ux);
    vg.closePath();
    vg.fillColor(theme::accent); vg.fill();
}
}

void drawMicView(NanoVG& vg, const PeMicGeometry& g, View v, const Rect& r, const ViewTransform& t, int hotMic, int hotMarker,
    uint32_t position, const char* caption)
{
    vg.save();
    vg.beginPath();
    vg.roundedRect(r.x, r.y, r.w, r.h, theme::radius);
    vg.fillColor(theme::panel);
    vg.fill();
    vg.intersectScissor(r.x, r.y, r.w, r.h); // within the caller's clip (the scrolled content area)

    if (v == kTop) {
        if (std::isfinite(g.wall_y)) { // the upright's wall
            const Point2 a = t.toScreen(-10, g.wall_y);
            vg.beginPath(); vg.moveTo(r.x, a.y); vg.lineTo(r.x + r.w, a.y);
            vg.strokeColor(theme::woodEdge); vg.strokeWidth(3); vg.stroke();
        }
        vg.beginPath();
        for (uint32_t i = 0; i < g.case_top_count; ++i) {
            const Point2 p = t.toScreen(g.case_top[i][0], g.case_top[i][1]);
            i == 0 ? vg.moveTo(p.x, p.y) : vg.lineTo(p.x, p.y);
        }
        vg.closePath();
        vg.fillColor(theme::wood); vg.fill();
        vg.strokeColor(theme::woodEdge); vg.strokeWidth(1); vg.stroke();
        if (std::isfinite(g.keys_top[0])) {
            vg.beginPath(); rectWorld(vg, t, g.keys_top); vg.fillColor(theme::keys); vg.fill();
        }
        for (int i = 1; i < 5; ++i) { // the other positions
            if ((uint32_t)i == position)
                continue;
            const Point2 m = t.toScreen(g.markers[i][0], g.markers[i][1]);
            vg.beginPath(); vg.circle(m.x, m.y, i == hotMarker ? 6 : 4);
            vg.fillColor(i == hotMarker ? theme::dim : theme::marker); vg.fill();
        }
    } else {
        const Point2 f = t.toScreen(0, g.floor_z);
        vg.beginPath(); vg.moveTo(r.x, f.y); vg.lineTo(r.x + r.w, f.y);
        vg.strokeColor(theme::line); vg.strokeWidth(1); vg.stroke();
        if (g.outline_front_count > 0) { // the harp from the side: its polygons, split at NaN points
            bool open = false;
            for (uint32_t i = 0; i <= g.outline_front_count && i <= 32; ++i) {
                const bool end = i == g.outline_front_count || i == 32 || !std::isfinite(g.outline_front[i][0]);
                if (end) {
                    if (open) {
                        vg.closePath(); vg.fillColor(theme::wood); vg.fill();
                        vg.strokeColor(theme::woodEdge); vg.strokeWidth(1); vg.stroke();
                    }
                    open = false;
                    continue;
                }
                const Point2 q = t.toScreen(g.outline_front[i][0], g.outline_front[i][1]);
                if (!open) { vg.beginPath(); vg.moveTo(q.x, q.y); open = true; } else vg.lineTo(q.x, q.y);
            }
        } else {
            vg.beginPath(); rectWorld(vg, t, g.case_front); vg.fillColor(theme::wood); vg.fill();
            vg.strokeColor(theme::woodEdge); vg.stroke();
        }
        if (std::isfinite(g.keys_front[0])) {
            vg.beginPath(); rectWorld(vg, t, g.keys_front); vg.fillColor(theme::keys); vg.fill();
        }
        const Point2 a = t.toScreen(g.lid_front[0][0], g.lid_front[0][1]), b = t.toScreen(g.lid_front[1][0], g.lid_front[1][1]);
        vg.beginPath();
        if (!std::isfinite(g.lid_front[0][0])) {
            // no lid (the harp)
        } else if (g.body == 1) { // the upright's open top lid, seen face-on
            vg.rect(std::fmin(a.x, b.x), std::fmin(a.y, b.y), std::fabs(b.x - a.x), std::fabs(b.y - a.y));
            vg.strokeColor(theme::woodEdge); vg.strokeWidth(1); vg.stroke();
        } else {
            vg.moveTo(a.x, a.y); vg.lineTo(b.x, b.y);
            vg.strokeColor(theme::dim); vg.strokeWidth(2); vg.stroke();
        }
    }

    if (g.has_pair) {
        Point2 m[2];
        for (int i = 0; i < 2; ++i) {
            double a, b;
            viewAB(g, v, g.mics[i], a, b);
            m[i] = t.toScreen(a, b);
        }
        vg.beginPath(); vg.moveTo(m[0].x, m[0].y); vg.lineTo(m[1].x, m[1].y);
        vg.strokeColor(theme::accent); vg.strokeWidth(1); vg.stroke();
        if (v == kFront && g.height_drag) { // the height above the board / opening
            for (int i = 0; i < 2; ++i) {
                const Point2 base = t.toScreen(g.mics[i][frontAxis(g)], g.origin[2]);
                vg.beginPath(); vg.moveTo(m[i].x, m[i].y); vg.lineTo(base.x, base.y);
                vg.strokeColor(theme::accent); vg.stroke();
            }
        }
        for (int i = 0; i < 2; ++i)
            if (g.pattern[i] > 0) // omni (and every preset): no arrow, no glyph
                drawAim(vg, g, v, t, i, m[i]);
        for (int i = 0; i < 2; ++i) {
            vg.beginPath(); vg.circle(m[i].x, m[i].y, i == hotMic ? 7 : 5.5f);
            vg.fillColor(i == hotMic ? theme::accentText : theme::accent); vg.fill();
        }
        // which is which: L left of its dot, R right of it (also when the two coincide)
        vg.fontSize(theme::smallFont);
        for (int i = 0; i < 2; ++i) {
            vg.fillColor(i == hotMic ? theme::accentText : theme::dim);
            vg.textAlign(NanoVG::Align((i == 0 ? NanoVG::ALIGN_RIGHT : NanoVG::ALIGN_LEFT) | NanoVG::ALIGN_MIDDLE));
            vg.text(m[i].x + (i == 0 ? -10.0f : 10.0f), m[i].y, i == 0 ? "L" : "R", nullptr);
        }
    } else if (v == kTop) {
        vg.fontSize(theme::smallFont); vg.fillColor(theme::dim);
        vg.textAlign(NanoVG::Align(NanoVG::ALIGN_CENTER | NanoVG::ALIGN_TOP));
        vg.text(r.x + r.w / 2, r.y + 6, "Recording: the fitted sound as recorded", nullptr);
    }
    vg.fontSize(theme::smallFont); vg.fillColor(theme::faint);
    vg.textAlign(NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_BOTTOM));
    vg.text(r.x + 6, r.y + r.h - 4, caption, nullptr);
    vg.restore();
}
}
