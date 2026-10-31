#include "About.hpp"
#include "Layout.hpp"

#include <fstream>
#include <sstream>

namespace pe {
namespace {
/// the line without the spaces (and tabs) at its ends
std::string trimmed(const std::string& s)
{
    const size_t a = s.find_first_not_of(" \t"), b = s.find_last_not_of(" \t");
    return a == std::string::npos ? std::string() : s.substr(a, b - a + 1);
}

/// the inline marks out: `**` and backticks
std::string plain(const std::string& s)
{
    std::string r;
    for (size_t i = 0; i < s.size(); ++i) {
        if (s[i] == '`')
            continue;
        if (s[i] == '*' && i + 1 < s.size() && s[i + 1] == '*') {
            ++i;
            continue;
        }
        r += s[i];
    }
    return r;
}

/// the length in bytes of the UTF-8 character starting at s[i]
size_t charLength(const std::string& s, size_t i)
{
    size_t n = 1;
    while (i + n < s.size() && (s[i + n] & 0xC0) == 0x80)
        ++n;
    return n;
}

const std::string kBullet = "• ";
} // namespace

std::vector<TextLine> markdownLines(const std::string& markdown)
{
    std::vector<TextLine> out;
    bool open = false; // the last line is a paragraph or a list item that further lines join
    std::istringstream in(markdown);
    for (std::string raw; std::getline(in, raw);) {
        const std::string t = trimmed(raw.empty() || raw.back() != '\r' ? raw : raw.substr(0, raw.size() - 1));
        if (t.empty()) { // between blocks: one blank line, none at the start
            if (!out.empty() && !out.back().text.empty())
                out.push_back({});
            open = false;
            continue;
        }
        if (const size_t marks = t.find_first_not_of('#'); marks > 0 && marks != std::string::npos && t[marks] == ' ') {
            out.push_back({ plain(trimmed(t.substr(marks))), true });
            open = false;
        } else if (t.size() > 2 && (t[0] == '-' || t[0] == '*') && t[1] == ' ') {
            out.push_back({ kBullet + plain(trimmed(t.substr(2))), false });
            open = true;
        } else if (open) {
            out.back().text += " " + plain(t);
        } else {
            out.push_back({ plain(t), false });
            open = true;
        }
    }
    while (!out.empty() && out.back().text.empty())
        out.pop_back();
    return out;
}

std::vector<TextLine> wrapLines(const std::vector<TextLine>& lines, float width)
{
    std::vector<TextLine> out;
    auto fits = [width](const std::string& s) { return textWidth(s.c_str()) <= width; };
    for (const TextLine& line : lines) {
        if (line.text.empty()) {
            out.push_back(line);
            continue;
        }
        const bool item = line.text.rfind(kBullet, 0) == 0;
        const std::string indent = item ? "  " : ""; // a list item's further lines start under its text
        std::string cur;   // the line being filled
        bool empty = true; // no word in it yet (only the indent)
        auto next = [&] {
            out.push_back({ cur, line.heading });
            cur = indent;
            empty = true;
        };
        std::istringstream in(line.text);
        std::vector<std::string> words;
        for (std::string w; in >> w;)
            words.push_back(w);
        if (item && words.size() > 1) { // the bullet stays with the first word
            words[1] = words[0] + " " + words[1];
            words.erase(words.begin());
        }
        for (const std::string& w : words) {
            if (fits(empty ? cur + w : cur + " " + w)) {
                cur = empty ? cur + w : cur + " " + w;
                empty = false;
                continue;
            }
            if (!empty)
                next();
            if (fits(cur + w)) {
                cur += w;
                empty = false;
                continue;
            }
            // longer than a line: broken where it reaches the end (a character a line at least)
            for (size_t i = 0; i < w.size();) {
                const std::string c = w.substr(i, charLength(w, i));
                if (!fits(cur + c) && !empty)
                    next();
                if (!fits(cur + c) && empty)
                    cur.clear(); // no room for the indent either
                cur += c;
                empty = false;
                i += c.size();
            }
        }
        if (!empty)
            out.push_back({ cur, line.heading });
    }
    return out;
}

bool readTextFile(const std::string& path, std::string& out)
{
    out.clear();
    std::ifstream f(path, std::ios::binary);
    if (!f)
        return false;
    std::ostringstream s;
    s << f.rdbuf();
    out = s.str();
    return true;
}
} // namespace pe
