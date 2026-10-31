// The About tab's text (pure; tested in ui_tests): the plugin's name, version and commit, the
// project's link, and an instrument's LICENSE.md as plain lines wrapped to the tab's width.
#pragma once
#include <cstdint>
#include <string>
#include <vector>

namespace pe {
/// the project's page (the repository until its site is up): the About tab's link
constexpr const char* kProjectUrl = "https://github.com/ccoors/dussek";
/// the plugin's license, in one line
constexpr const char* kPluginLicenseLine = "Free software under the GNU Affero General Public License, version 3 or later (AGPL-3.0-or-later).";

/// the plugin's version: the project's in plugin/CMakeLists.txt (the hosts' too, getVersion)
struct Version { uint32_t major, minor, patch; };
Version pluginVersion();
/// "0.3.0"
std::string versionText();
/// the commit the plugin was built from (git describe --always --dirty), "unknown" outside a git
/// checkout (a source tarball)
const char* buildCommit();

/// a line of plain text: a heading's (the Markdown's `#` lines) or the body's
struct TextLine {
    std::string text;
    bool heading = false;
    bool operator==(const TextLine& o) const { return text == o.text && heading == o.heading; }
};
/// Markdown as plain lines: the heading marks, `**` and backticks dropped; a paragraph's lines
/// joined into one, a list item ("- ") one line from its bullet ("• "), a blank line between
/// blocks (several as one, none at the ends); CRLF read as LF
std::vector<TextLine> markdownLines(const std::string& markdown);
/// the lines wrapped to `width` (textWidth, Layout.hpp) at spaces, a word longer than a line
/// broken where it reaches the end; a list item's further lines indented under its text; a blank
/// line kept
std::vector<TextLine> wrapLines(const std::vector<TextLine>& lines, float width);
/// the file's text into `out`; false when it cannot be read (`out` empty)
bool readTextFile(const std::string& path, std::string& out);
} // namespace pe
