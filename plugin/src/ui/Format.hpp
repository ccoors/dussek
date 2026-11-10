// Parameter values as text and back, and the slider mapping (pure; tested in ui_tests).
#pragma once
#include <cstdint>
#include <string>

namespace pe {
std::string formatValue(uint32_t param, float value);
/// text → value: a number with an optional unit, "off" for levels, an enumeration's label
/// (case-insensitive) or index, "on"/"off" for switches; clamped to the range, integers
/// rounded. false (out untouched): nothing usable, or not finite.
bool parseValue(uint32_t param, const std::string& text, float& out);
/// the mic distance as the engine uses it: `effective` (from pe_mic_geometry) marked
/// "(default)" for the parameter's 0 and "(limited)" where a limit moved it; no pair
/// ("Recording"): the parameter as it is
std::string formatMicDistance(float value, bool hasPair, double effective);
/// the spacing a pair has, cm: the set one, or the position's widest marked "(limited)" (a bell's Player at the ears)
std::string formatMicSpacing(float value, bool hasPair, double effective);
float toNormalized(uint32_t param, float value);
float fromNormalized(uint32_t param, float norm);
}
