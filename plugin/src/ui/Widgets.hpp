// Drawing of the header's controls and the dropdown lists (no state; PianoUI owns interaction).
#pragma once
#include "Layout.hpp"
#include "NanoVG.hpp"

#include <string>
#include <vector>

namespace pe {
struct ControlLook { const char* label; std::string value; float norm; bool active; bool hot; bool editing; const std::string* editText; };
void drawSlider(DGL_NAMESPACE::NanoVG& vg, const Rect& r, const ControlLook& c);
void drawToggle(DGL_NAMESPACE::NanoVG& vg, const Rect& r, const ControlLook& c);
void drawDropdown(DGL_NAMESPACE::NanoVG& vg, const Rect& r, const ControlLook& c);
/// the list's rows from `scroll` px down (cut to its rect), its bar where it scrolls
void drawDropdownList(DGL_NAMESPACE::NanoVG& vg, const Rect& list, const std::vector<std::string>& items, int selected, int hot,
    float scroll = 0);
}
