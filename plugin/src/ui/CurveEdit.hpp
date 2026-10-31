// The Touch tab's velocity curve, as the user edits it (pure; tested in ui_tests): its points,
// which one is under the pointer, adding, moving and removing them within the curve's rules
// (engine/src/velocity.rs: 2–8 points at whole velocities 1–127, the ends at in 1 and 127 and
// moving up and down only, in strictly rising, out not falling), and the graph's mapping between
// velocities and logical coordinates.
#pragma once
#include "Layout.hpp"
#include "pianoengine.h"

#include <string>

namespace pe {

class CurveEdit {
public:
    CurveEdit();
    void set(const PeVelocityCurve& c);
    const PeVelocityCurve& curve() const { return fCurve; }
    /// the point within kHitRadius of the logical (x, y) in `graph`, the nearest (-1: none)
    int pointAt(const Rect& graph, float x, float y) const;
    /// a point at the logical (x, y), its out held within the neighbors' (the curve keeps rising):
    /// its index, or -1 (none added: at kMaxPoints, on an end's in or another point's)
    int add(const Rect& graph, float x, float y);
    /// point `index` towards the logical (x, y): the ends up and down only; between the
    /// neighbors' ins (1 apart) and outs (the curve keeps rising); 1–127
    void move(int index, const Rect& graph, float x, float y);
    /// false for the ends
    bool remove(int index);
    /// "1:1,64:80,127:127" (the "velocity_curve" state)
    std::string text() const;

    /// velocity (in, out) → the logical point in `g` (in 1 at its left, out 127 at its top)
    static void toGraph(const Rect& g, float vin, float vout, float& x, float& y);
    static void fromGraph(const Rect& g, float x, float y, float& vin, float& vout);

    static constexpr int kMaxPoints = 8;
    static constexpr float kHitRadius = 8;

private:
    PeVelocityCurve fCurve {};
};

} // namespace pe
