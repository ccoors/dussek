// The plugin's Copy CLI line for named controls (pe::settingsLine), for scripts that render what the
// plugin plays (site/render.py):
//   settings_line <instrument dir> [symbol=value ...]
// prints the `dussek render` line; exit 1 for an unknown symbol or an unreadable value.
#include "EngineValues.hpp"

#include <cstdio>
#include <cstdlib>
#include <string>
#include <utility>
#include <vector>

int main(int argc, char** argv)
{
    if (argc < 2) {
        std::fprintf(stderr, "usage: settings_line <instrument dir> [symbol=value ...]\n");
        return 2;
    }
    std::vector<std::pair<std::string, float>> settings;
    for (int a = 2; a < argc; ++a) {
        const std::string arg = argv[a];
        const size_t eq = arg.find('=');
        char* end = nullptr;
        const float value = eq == std::string::npos ? 0.0f : std::strtof(arg.c_str() + eq + 1, &end);
        if (eq == std::string::npos || end == arg.c_str() + eq + 1 || *end != '\0') {
            std::fprintf(stderr, "settings_line: `%s` is not symbol=value\n", arg.c_str());
            return 1;
        }
        settings.emplace_back(arg.substr(0, eq), value);
    }
    std::string error;
    const std::string line = pe::settingsLine(argv[1], settings, error);
    if (line.empty()) {
        std::fprintf(stderr, "settings_line: %s\n", error.c_str());
        return 1;
    }
    std::printf("%s\n", line.c_str());
    return 0;
}
