// Where everything goes, in logical units (pure; tested in ui_tests). The header across the top;
// wide: the microphone drawing on the left and a tabbed panel on the right, narrow: one column
// with the drawing in the Mics tab. Only the tab's content scrolls.
#pragma once
#include "Instruments.hpp"

#include <cstdint>
#include <string>
#include <vector>

namespace pe {

struct Rect { float x = 0, y = 0, w = 0, h = 0;
    bool contains(float px, float py) const { return px >= x && px < x + w && py >= y && py < y + h; }
    bool overlaps(const Rect& o) const { return x < o.x + o.w && o.x < x + w && y < o.y + o.h && o.y < y + h; } };
enum Tab : uint32_t { kTabSound, kTabMorph, kTabTouch, kTabFelt, kTabMics, kTabTuning, kTabCondition, kTabExtras, kTabMidi, kTabCount };
enum class Kind { Slider, Toggle, Dropdown };
/// a control: its row and the row's parts (the label; the control: a slider's track, a switch, a
/// menu box; the value text, click to type). Compact rows put them on one line; the header's keep
/// the old widgets. `scrolls`: in the content area (layout coordinates, shown shifted by the scroll)
struct Row { uint32_t param; Kind kind; Rect rect, label, control, value; bool compact = false, scrolls = false; };
/// a tab's group: an optional subheading (nullptr: none) and its parameters
struct TabGroup { const char* title; std::vector<uint32_t> params; };
struct Subheading { const char* title; Rect rect; }; // in the content area
struct TabButton { Tab tab; Rect rect; };
/// a MIDI tab row: a tie (`tie`: its index in MidiMap::ties()) or an untied Next / Previous
/// instrument (`tie` -1); `button` its remove (×) or learn button
struct MidiRow { int tie; uint32_t target; Rect rect, button; };
/// a Morph tab row: the list's `index`-th instrument (0: the main one, without buttons), its move up,
/// move down and remove buttons
struct MorphRow { int index; Rect rect, up, down, remove; };
struct Layout {
    bool wide = false;
    int columns = 1;                // the content's columns
    Rect panic, menu;               // header, right: Panic, the menu (zoom, Copy CLI, Reset)
    Rect topView, frontView;        // the microphone drawing (w == 0: not shown)
    bool drawingScrolls = false;    // narrow: the drawing is in the Mics tab's content
    std::vector<TabButton> tabs;
    std::vector<Subheading> subheadings;
    std::vector<Row> rows;          // every visible control
    Rect content;                   // where the tab's content scrolls (window-logical)
    float contentHeight = 0;        // the content's height from content.y (> content.h: it scrolls)
    Rect emptyNote;                 // w > 0: nothing in this tab applies to the instrument
    // the Touch tab (w == 0 on the others), in layout coordinates: the velocity curve's plotting
    // area (its axis labels in the margins: kCurveLabelW to the left, kLineH below), its Reset
    // button above it, the recent notes' bars under it (their caption below them)
    Rect curve, resetCurve, history;
    // the MIDI tab (empty on the others), in layout coordinates: a row per tie, then Next and
    // Previous instrument where untied, then the program change switch's row
    std::vector<MidiRow> midiRows;
    Rect programSwitch, takeoverSwitch;
    // the Morph tab (empty on the others), in layout coordinates: a row per instrument, the plus
    // button under them (`morphFull`: the list holds kMorphMax, it adds none), the pad (a line under
    // it for "building…"); `morphNote` instead where the instrument does not morph
    std::vector<MorphRow> morphRows;
    Rect morphPlus, morphPad, morphNote;
    bool morphFull = false;
    // the Felt tab (w == 0 on the others), in layout coordinates: the quantities' chips, Original and
    // Reset in their row (or the next), the selected quantity's graph (its labels in the margins:
    // kFeltLabelW to the left, kLineH below), the other three small under it; `feltNote` a line
    // under the graph ("measuring…"), or alone where the felt does not apply
    Rect feltChips[4], feltOriginal, feltReset, feltGraph, feltSmall[3], feltNote;
};
constexpr float kDefaultWidth = 900, kDefaultHeight = 600, kMinWidth = 420, kMinHeight = 360, kBreakpoint = 760;
constexpr float kRowH = 30, kPad = 12, kGap = 6;           // header rows, dropdown lists
constexpr float kLineH = 22, kLineGap = 4, kSubheadH = 18; // compact rows, group subheadings
constexpr float kTabH = 26;
constexpr float kColumnWidth = 360; // a content column per this much panel width, up to kMaxColumns
constexpr int kMaxColumns = 3;
constexpr float kValueW = 76;       // a compact slider's value text, at least (its longest text widens it)
constexpr float kCurveLabelW = 28;  // the curve's left margin, for its velocity labels
constexpr float kFeltLabelW = 46;   // the felt's graph's left margin, for its values ("+12 dB", "20 N")
constexpr float kTagW = 46;         // a tied control's CC tag beside its label ("CC 127")
/// A submenu of `count` rows beside row `row` of the open `list`: to its right (to its left
/// where the view's right edge `viewRight` leaves no room), its first row level with that row,
/// moved up to stay above `viewBottom` (but not above `viewTop`); taller than the view: cut to it
/// (it scrolls)
/// the header menu's items (its button right of Panic): the zoom (its steps in a submenu), Copy CLI,
/// Reset (armed by a first click, done by the second)
enum MenuItem : int { kMenuZoom, kMenuCopy, kMenuReset, kMenuCount };
std::vector<std::string> menuItems(int zoom, bool resetArmed);
Rect submenuRect(const Rect& list, int row, uint32_t count, float viewTop, float viewBottom, float viewRight);
/// the open dropdown's list below (or above, if it does not fit) `anchor`: the anchor's width, at
/// least 200 and its longest item's (`itemsW`, textWidth) with the text's margins, moved left to
/// end by `viewRight`. Where it fits neither below nor above, on the roomier side, cut to the
/// view (it scrolls, its bar widening it by kListBarW)
Rect dropdownListRect(const Rect& anchor, uint32_t count, float viewTop, float viewBottom, float itemsW, float viewRight);
/// A list's rows in its rect, `scroll` px down from the first (window-logical): a list cut to the
/// view scrolls, its bar along the right edge
constexpr float kListBarW = 10, kListThumbMin = 24;
float listMaxScroll(const Rect& list, uint32_t count);
/// the row under window-logical y (-1: none; the list's rect itself holds x)
int listRowAt(const Rect& list, uint32_t count, float scroll, float y);
/// the scroll with row `row` centered (as far as the list goes; row < 0: the top)
float listScrollTo(const Rect& list, uint32_t count, int row);
/// the scroll after the wheel moved `pixels` (positive: towards the top)
float listWheel(const Rect& list, uint32_t count, float scroll, float pixels);
/// the bar's strip and its thumb (w == 0: the list fits, no bar)
struct ListBar { Rect track, thumb; };
ListBar listBar(const Rect& list, uint32_t count, float scroll);
/// the scroll that puts the thumb's top at window-logical y
float listScrollAtThumb(const Rect& list, uint32_t count, float thumbTop);
Kind paramKind(uint32_t param);
const char* tabName(Tab t);
/// the tab's groups in order; with the header's and the drawing's controls every parameter is in exactly one
const std::vector<TabGroup>& tabGroups(Tab t);
/// the header's controls, in order
const std::vector<uint32_t>& headerParams();
/// the three rows under the drawing
const std::vector<uint32_t>& drawingParams();
/// Morph X and Y: the Morph tab's pad, never a row
const std::vector<uint32_t>& morphParams();
/// a label's width at the theme's font size, estimated (the layout has no font): per character
float textWidth(const char* s);
Layout computeLayout(float width, float height, Tab tab, const UiContext& ctx);
/// the window-logical point is over the content area
bool inContent(const Layout& l, float x, float y);
/// window-logical → layout coordinates: the scroll added for a point in the content (`inContent`:
/// where it is, or for a running drag where it was pressed)
void toLayout(const Layout& l, float scroll, bool inContent, float& x, float& y);
/// the index of the row under the window-logical point (-1: none)
int rowAt(const Layout& l, float x, float y, float scroll);
/// the drawing's view under the window-logical point, where it is shown: 0 from above, 1 from the
/// front, -1 none (a drawing in the scrolled content counts inside the content area alone)
int viewAt(const Layout& l, float x, float y, float scroll);
/// the tab button under the window-logical point (kTabCount: none)
Tab tabAt(const Layout& l, float x, float y);
/// a rect as shown (window-logical) at this scroll
Rect shown(const Rect& r, bool scrolls, float scroll);
float maxScroll(const Layout& l);
/// the scroll after the wheel moved `pixels` (positive: towards the top) with the pointer at the
/// window-logical (x, y): the content moves when the pointer is over it and it overflows; nothing else
float wheelScroll(const Layout& l, float x, float y, float scroll, float pixels);

} // namespace pe
