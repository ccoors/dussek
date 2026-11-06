// Drawing of the tabbed panel: the tab bar, group subheadings, compact rows (label · control ·
// value on one line, switches for toggles) and the tooltip (no state; PianoUI owns interaction).
#pragma once
#include "Layout.hpp"
#include "Widgets.hpp"

namespace pe {
/// `hot`: the tab under the pointer (-1: none)
void drawTabBar(DGL_NAMESPACE::NanoVG& vg, const Layout& l, Tab active, int hot);
void drawSubheading(DGL_NAMESPACE::NanoVG& vg, const Subheading& s);
void drawEmptyNote(DGL_NAMESPACE::NanoVG& vg, const Rect& r);
void drawCompactRow(DGL_NAMESPACE::NanoVG& vg, const Row& r, const ControlLook& c);
/// MIDI learn: a tied control's tag ("CC 21"): a compact row's at the right end of its label (the
/// layout leaves room, kTagW), a header slider's after its label, a header button's on its top
/// right corner; and the outline of a control waiting for a CC
void drawTieTag(DGL_NAMESPACE::NanoVG& vg, const Row& r, const char* text, const char* label);
void drawWaiting(DGL_NAMESPACE::NanoVG& vg, const Row& r);
/// a MIDI tab row: the control's name, what it is tied to (`detail`), its button (× or learn;
/// `accent`: outlined, as while it waits)
void drawMidiRow(DGL_NAMESPACE::NanoVG& vg, const MidiRow& r, const char* name, const std::string& detail, const char* button,
    bool hot, bool accent);
/// a Morph tab row: the instrument's name and its share of the morph, its move up, move down and
/// remove buttons (none on the main row)
void drawMorphRow(DGL_NAMESPACE::NanoVG& vg, const MorphRow& r, const std::string& name, float share, bool hot);
/// a plain button (the Morph tab's plus): dimmed when it does nothing
void drawPlainButton(DGL_NAMESPACE::NanoVG& vg, const Rect& b, const char* text, bool enabled, bool hot);
/// the pad: the polygon of the list's instruments (a line for two; dimmed with a hint for one), each
/// corner's label, the point; "building…" under it while the worker builds
void drawMorphPad(DGL_NAMESPACE::NanoVG& vg, const Rect& pad, const std::vector<std::string>& labels, float x, float y,
    bool building, bool hot);
/// where the instrument does not morph
/// the About tab: its lines, the project's link and its Copy link button (`hot`: under the pointer)
void drawAboutTab(DGL_NAMESPACE::NanoVG& vg, const Layout& l, bool hot);
/// the tooltip's height at `width`: the title, the wrapped description (may be empty) and the gestures
float tooltipHeight(DGL_NAMESPACE::NanoVG& vg, float width, const char* title, const char* body, const char* gestures);
void drawTooltip(DGL_NAMESPACE::NanoVG& vg, const Rect& r, const char* title, const char* body, const char* gestures);
}
