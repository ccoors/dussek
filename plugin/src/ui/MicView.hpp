// Draws the two views of the microphone drawing from a PeMicGeometry.
#pragma once
#include "MicMap.hpp"
#include "NanoVG.hpp"

namespace pe {
void drawMicView(DGL_NAMESPACE::NanoVG& vg, const PeMicGeometry& g, View v, const Rect& r, const ViewTransform& t, int hotMic,
    int hotMarker, uint32_t position, const char* caption);
}
