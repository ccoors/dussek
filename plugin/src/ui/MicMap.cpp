#include "MicMap.hpp"

#include <algorithm>
#include <cmath>

namespace pe {

Point2 ViewTransform::toScreen(double a, double b) const
{
    return { (float)(ox + a * scale), (float)(flip ? oy - b * scale : oy + b * scale) };
}

void ViewTransform::toWorld(Point2 s, double& a, double& b) const
{
    a = (s.x - ox) / scale;
    b = flip ? (oy - s.y) / scale : (s.y - oy) / scale;
}

namespace {

struct Box {
    double a0 = 1e9, a1 = -1e9, b0 = 1e9, b1 = -1e9;
    void add(double a, double b) { if (std::isfinite(a) && std::isfinite(b)) { a0 = std::min(a0, a); a1 = std::max(a1, a); b0 = std::min(b0, b); b1 = std::max(b1, b); } }
};

} // namespace

ViewTransform fitView(const PeMicGeometry& g, View v, const Rect& r)
{
    Box box;
    if (v == kTop) {
        for (uint32_t i = 0; i < g.case_top_count; ++i)
            box.add(g.case_top[i][0], g.case_top[i][1]);
        box.add(g.keys_top[0], g.keys_top[1]);
        for (int i = 1; i < 5; ++i)
            box.add(g.markers[i][0], g.markers[i][1]);
        if (g.has_pair)
            for (int i = 0; i < 2; ++i)
                box.add(g.mics[i][0], g.mics[i][1]);
        box.add(0, g.wall_y);
    } else {
        if (g.outline_front_count > 0) {
            for (uint32_t i = 0; i < g.outline_front_count && i < 32; ++i)
                box.add(g.outline_front[i][0], g.outline_front[i][1]);
        } else {
            box.add(g.case_front[0], g.case_front[1]);
            box.add(g.case_front[2], g.case_front[3]);
        }
        box.add(g.lid_front[1][0], g.lid_front[1][1]);
        box.add(0, g.floor_z);
        if (g.has_pair)
            for (int i = 0; i < 2; ++i)
                box.add(g.mics[i][frontAxis(g)], g.mics[i][2]);
    }
    const double wa = std::max(box.a1 - box.a0, 0.1), wb = std::max(box.b1 - box.b0, 0.1);
    ViewTransform t;
    // both views upwards on the screen: from the front z, from above y (the back; the
    // front faces the viewer at the bottom), so +x (treble) is right in both, unmirrored
    t.flip = true;
    // above the caption's band at the foot (the microphones and the floor off its text)
    const float h = std::max(r.h - kCaptionBand, 1.0f);
    t.scale = (float)std::max(1e-3, std::min(r.w / wa, h / wb) * 0.84);
    const double ca = (box.a0 + box.a1) / 2, cb = (box.b0 + box.b1) / 2;
    t.ox = (float)(r.x + r.w / 2 - ca * t.scale);
    t.oy = (float)(t.flip ? r.y + h / 2 + cb * t.scale : r.y + h / 2 - cb * t.scale);
    return t;
}

int hitMic(const PeMicGeometry& g, const ViewTransform& t, View v, Point2 p, float radius)
{
    if (!g.has_pair)
        return -1;
    int best = -1;
    float bestD = radius;
    for (int i = 0; i < 2; ++i) {
        double a, b;
        viewAB(g, v, g.mics[i], a, b);
        const Point2 s = t.toScreen(a, b);
        const float d = std::hypot(s.x - p.x, s.y - p.y);
        if (d <= bestD) { bestD = d; best = i; }
    }
    return best;
}

int hitMarker(const PeMicGeometry& g, const ViewTransform& t, Point2 p, float radius, uint32_t current)
{
    for (int i = 1; i < 5; ++i) {
        if ((uint32_t)i == current)
            continue;
        const Point2 s = t.toScreen(g.markers[i][0], g.markers[i][1]);
        if (std::hypot(s.x - p.x, s.y - p.y) <= radius)
            return i;
    }
    return 0;
}

bool MicDrag::begin(const PeMicGeometry& g, View v, int which, double a, double b)
{
    // from the front only a height moves; from the side (the harp) the pair moves in that plane
    fActive = g.has_pair && (which == 0 || which == 1) && (v == kTop || g.height_drag || frontAxis(g) == 1);
    if (fActive) { fG = g; fView = v; fWhich = which; fA0 = a; fB0 = b; }
    return fActive;
}

DragValues MicDrag::update(double a, double b) const
{
    const PeMicGeometry& g = fG;
    const double da = a - fA0, db = b - fB0;
    double distance = g.distance, spacing = g.spacing;
    // the dragged microphone moves outward along the spread: + for the right one
    const double side = fWhich == 1 ? 1.0 : -1.0;
    if (fView == kTop) {
        spacing += 2.0 * side * (da * g.spread[0] + db * g.spread[1]);
        if (!g.height_drag)
            distance += da * g.axis[0] + db * g.axis[1];
    } else {
        spacing += 2.0 * side * (da * g.spread[frontAxis(g)] + db * g.spread[2]);
        if (g.height_drag)
            distance += db; // a height: z
        else
            distance += da * g.axis[frontAxis(g)] + db * g.axis[2];
    }
    distance = std::min(g.distance_max, std::max(g.distance_min, distance));
    spacing = std::min(g.spacing_max, std::max(0.0, spacing));
    return { (float)distance, (float)(spacing * 100.0) };
}

bool FreeDrag::begin(const PeMicGeometry& g, View v, int which, double a, double b)
{
    fActive = g.has_pair && (which == 0 || which == 1);
    if (fActive) {
        for (int k = 0; k < 3; ++k) fPos[k] = g.mics[which][k];
        fView = v; fWhich = which; fA0 = a; fB0 = b; fFrontAxis = frontAxis(g);
    }
    return fActive;
}

void FreeDrag::update(double a, double b, double out[3]) const
{
    for (int k = 0; k < 3; ++k) out[k] = fPos[k];
    const int ia = fView == kTop ? 0 : fFrontAxis, ib = fView == kTop ? 1 : 2;
    out[ia] += a - fA0;
    out[ib] += b - fB0;
}

namespace {
constexpr double kDeg = 3.14159265358979323846 / 180.0;
}

Point2 aimTip(const PeMicGeometry& g, const ViewTransform& t, View v, int which, float len)
{
    const double yaw = g.yaw[which] * kDeg, pitch = g.pitch[which] * kDeg;
    double da, db; // the unit direction in the view's plane (world a, b)
    if (v == kTop) {
        da = std::sin(yaw); db = std::cos(yaw);
    } else if (frontAxis(g) == 1) { // from the side: the aim's y component decides the side
        da = (std::cos(yaw) < 0 ? -1.0 : 1.0) * std::cos(pitch); db = std::sin(pitch);
    } else {
        da = (g.yaw[which] < 0 ? -1.0 : 1.0) * std::cos(pitch); db = std::sin(pitch);
    }
    double ma, mb;
    viewAB(g, v, g.mics[which], ma, mb);
    const Point2 m = t.toScreen(ma, mb);
    return { (float)(m.x + da * len), (float)(t.flip ? m.y - db * len : m.y + db * len) };
}

int hitAim(const PeMicGeometry& g, const ViewTransform& t, View v, float len, Point2 p, float radius)
{
    if (!g.has_pair)
        return -1;
    int best = -1;
    float bestD = radius;
    for (int i = 0; i < 2; ++i) {
        if (!(g.pattern[i] > 0))
            continue; // omni: no arrow
        const Point2 s = aimTip(g, t, v, i, len);
        const float d = std::hypot(s.x - p.x, s.y - p.y);
        if (d <= bestD) { bestD = d; best = i; }
    }
    return best;
}

bool AimDrag::begin(const PeMicGeometry& g, View v, int which)
{
    fActive = g.has_pair && (which == 0 || which == 1);
    if (fActive) { fView = v; fWhich = which; }
    return fActive;
}

void AimDrag::update(double a, double b, double& yaw, double& pitch) const
{
    if (a == 0 && b == 0)
        return; // on the microphone: no direction
    if (fView == kTop)
        yaw = std::atan2(a, b) / kDeg;
    else
        pitch = std::min(90.0, std::max(-90.0, std::atan2(b, std::fabs(a)) / kDeg));
}

} // namespace pe
