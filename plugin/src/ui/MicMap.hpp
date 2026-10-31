// The microphone drawing's coordinates and dragging (pure; tested in ui_tests).
#pragma once
#include "Layout.hpp"
#include "pianoengine.h"

namespace pe {

enum View : int { kTop = 0, kFront = 1 };
struct Point2 { float x, y; };
/// the second view's horizontal world axis: 0 x (from the front), 1 y (from the side: the harp)
inline int frontAxis(const PeMicGeometry& g) { return g.front_axis == 1 ? 1 : 0; }
/// a point's (a, b) in view v: from above (x, y), from the front (x, z) or the side (y, z)
inline void viewAB(const PeMicGeometry& g, View v, const double p[3], double& a, double& b)
{
    a = v == kTop ? p[0] : p[frontAxis(g)];
    b = v == kTop ? p[1] : p[2];
}
/// world (a, b) ↔ screen: from above a = x, b = y (the front, keys, at the bottom); from the front a = x, b = z (up)
/// (from the side, the harp: a = y)
struct ViewTransform { float scale = 1, ox = 0, oy = 0; bool flip = false;
    Point2 toScreen(double a, double b) const; void toWorld(Point2 s, double& a, double& b) const; };
ViewTransform fitView(const PeMicGeometry& g, View v, const Rect& r);  // whole drawing, aspect kept, centered, 8 % margin
int hitMic(const PeMicGeometry& g, const ViewTransform& t, View v, Point2 p, float radius);    // 0/1, −1 none (and at Recording)
int hitMarker(const PeMicGeometry& g, const ViewTransform& t, Point2 p, float radius, uint32_t current); // 1…4, 0 none (top view); never the current position (not drawn)
struct DragValues { float distance; float spacingCm; };
class MicDrag {
public:
    /// starts a drag of microphone `which` in view `v` at world (a, b); false: not draggable there
    bool begin(const PeMicGeometry& g, View v, int which, double a, double b);
    DragValues update(double a, double b) const;   // clamped to the engine's limits
    bool active() const { return fActive; }
    void end() { fActive = false; }
private:
    PeMicGeometry fG {}; View fView = kTop; int fWhich = 0; double fA0 = 0, fB0 = 0; bool fActive = false;
};

/// Free drag of one microphone (Custom): it follows the pointer in the view's plane (from above x, y; from the front x, z;
/// from the side y, z);
/// the third coordinate stays. The caller clamps the result with pe_mic_clamp.
class FreeDrag {
public:
    bool begin(const PeMicGeometry& g, View v, int which, double a, double b);   // false at Recording
    void update(double a, double b, double out[3]) const;                       // the microphone's new position
    int which() const { return fWhich; }
    bool active() const { return fActive; }
    void end() { fActive = false; }
private:
    double fPos[3] = {}; double fA0 = 0, fB0 = 0; View fView = kTop; int fWhich = 0, fFrontAxis = 0; bool fActive = false;
};

/// the aim arrow's length on screen (px), drawn and hit-tested
constexpr float kAimLen = 18;
/// where microphone `which`'s aim arrow ends on screen (`len` px long). From above the arrow points along
/// (sin yaw, cos yaw) in (x, y); from the front it shows the tilt, (±cos pitch, sin pitch) in (x, z), on the
/// side the microphone faces (+x unless the yaw is negative): always full length, and its tip follows
/// the pointer while AimDrag sets the pitch.
Point2 aimTip(const PeMicGeometry& g, const ViewTransform& t, View v, int which, float len);
/// which arrow tip (0 left, 1 right, −1 none) is within `radius` px of p; only microphones with a pattern above 0 have an arrow
int hitAim(const PeMicGeometry& g, const ViewTransform& t, View v, float len, Point2 p, float radius);
/// Dragging an arrow's tip sets the aim: from above the yaw (0 along +y, towards +x positive), from the front the pitch.
class AimDrag {
public:
    bool begin(const PeMicGeometry& g, View v, int which);
    void update(double a, double b, double& yaw, double& pitch) const;   // a, b: the pointer relative to the microphone, world coordinates
    int which() const { return fWhich; }
    bool active() const { return fActive; }
    void end() { fActive = false; }
private:
    View fView = kTop; int fWhich = 0; bool fActive = false;
};

} // namespace pe
