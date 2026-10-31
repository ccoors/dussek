#pragma once

namespace pe {

/// the number at the start of [first, last), read as std::from_chars reads it: a point whatever the
/// locale, the longest number there (no leading '+' or space). Its end, or nullptr if there is none.
/// std::from_chars for floating point is missing from the libc++ of older macOS releases.
const char* parseFloat(const char* first, const char* last, float& out);

} // namespace pe
