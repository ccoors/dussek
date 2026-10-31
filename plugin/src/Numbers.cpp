#include "Numbers.hpp"

#include <locale>
#include <sstream>
#include <string>

namespace pe {

const char* parseFloat(const char* first, const char* last, float& out)
{
    if (first == last || !(*first == '-' || *first == '.' || (*first >= '0' && *first <= '9')))
        return nullptr;
    std::istringstream in(std::string(first, last));
    in.imbue(std::locale::classic());
    float v = 0;
    if (!(in >> v))
        return nullptr;
    out = v;
    return in.eof() ? last : first + static_cast<std::ptrdiff_t>(in.tellg());
}

} // namespace pe
