#include "Paths.hpp"

#include <cstdlib>

namespace pe {

Platform hostPlatform()
{
#if defined(_WIN32)
    return Platform::Windows;
#elif defined(__APPLE__)
    return Platform::MacOS;
#else
    return Platform::Linux;
#endif
}

DataDirs dataDirs(const std::function<const char*(const char*)>& env, Platform platform)
{
    const auto get = [&](const char* name) {
        const char* v = env(name);
        return std::string(v != nullptr ? v : "");
    };
    DataDirs d;
    switch (platform) {
    case Platform::Windows:
        if (const std::string app = get("APPDATA"); !app.empty())
            d.data = d.config = app + "/PianoEngine";
        break;
    case Platform::MacOS:
        if (const std::string home = get("HOME"); !home.empty())
            d.data = d.config = home + "/Library/Application Support/PianoEngine";
        break;
    case Platform::Linux: {
        // the XDG spec: a relative path is invalid and ignored
        const auto xdg = [&](const char* name, const char* fallback) {
            if (const std::string v = get(name); !v.empty() && v[0] == '/')
                return v + "/pianoengine";
            if (const std::string home = get("HOME"); !home.empty())
                return home + fallback;
            return std::string();
        };
        d.data = xdg("XDG_DATA_HOME", "/.local/share/pianoengine");
        d.config = xdg("XDG_CONFIG_HOME", "/.config/pianoengine");
        break;
    }
    }
    return d;
}

DataDirs dataDirs()
{
    return dataDirs([](const char* name) { return static_cast<const char*>(std::getenv(name)); }, hostPlatform());
}

} // namespace pe
