// One dark theme: colors and sizes (logical units; the UI scales them).
#pragma once
#include "Color.hpp"

namespace pe::theme {
inline const DGL_NAMESPACE::Color bg { 29, 31, 34 }, panel { 37, 40, 44 }, line { 51, 54, 59 };
inline const DGL_NAMESPACE::Color text { 216, 218, 221 }, dim { 154, 160, 166 }, faint { 110, 115, 121 };
inline const DGL_NAMESPACE::Color accent { 91, 154, 194 }, accentText { 156, 204, 228 };
inline const DGL_NAMESPACE::Color wood { 58, 53, 48 }, woodEdge { 107, 96, 85 }, keys { 85, 85, 85 }, marker { 90, 94, 99 };
constexpr float fontSize = 13, smallFont = 11, radius = 4, track = 4, knob = 7;
}
