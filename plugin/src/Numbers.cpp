#include "Numbers.hpp"

#include <locale>
#include <sstream>
#include <string>

namespace pe {

namespace {
bool digit(const char* p, const char* last) { return p < last && *p >= '0' && *p <= '9'; }
}

const char* parseFloat(const char* first, const char* last, float& out)
{
    // the number's extent first ([-]digits[.digits][e[±]digits]): a stream given more reads past it on
    // libc++ (macOS), which takes letters of a hex float ("1.5x") into the number and then fails
    const char* p = first;
    if (p < last && *p == '-')
        ++p;
    const char* digits = p;
    while (digit(p, last))
        ++p;
    if (p < last && *p == '.')
        for (++p; digit(p, last);)
            ++p;
    if (p == digits || (p == digits + 1 && *digits == '.'))
        return nullptr; // no digit
    if (p < last && (*p == 'e' || *p == 'E')) {
        const char* e = p + 1;
        if (e < last && (*e == '+' || *e == '-'))
            ++e;
        if (digit(e, last)) {
            while (digit(e, last))
                ++e;
            p = e;
        }
    }
    std::istringstream in(std::string(first, p));
    in.imbue(std::locale::classic());
    float v = 0;
    if (!(in >> v))
        return nullptr;
    out = v;
    return p;
}

} // namespace pe
