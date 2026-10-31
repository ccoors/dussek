#include "Paths.hpp"

#include <cstdlib>
#include <filesystem>

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
            d.data = d.config = app + "/Dussek";
        break;
    case Platform::MacOS:
        if (const std::string home = get("HOME"); !home.empty())
            d.data = d.config = home + "/Library/Application Support/Dussek";
        break;
    case Platform::Linux: {
        // the XDG spec: a relative path is invalid and ignored
        const auto xdg = [&](const char* name, const char* fallback) {
            if (const std::string v = get(name); !v.empty() && v[0] == '/')
                return v + "/dussek";
            if (const std::string home = get("HOME"); !home.empty())
                return home + fallback;
            return std::string();
        };
        d.data = xdg("XDG_DATA_HOME", "/.local/share/dussek");
        d.config = xdg("XDG_CONFIG_HOME", "/.config/dussek");
        break;
    }
    }
    return d;
}

std::string standaloneExecutable(const std::string& self, bool selfIsStandalone, const char* path)
{
    namespace fs = std::filesystem;
    if (self.empty())
        return std::string();
    if (selfIsStandalone)
        return self;
#ifdef _WIN32
    const char* const names[] = { "dussek.exe" };
    const char sep = ';';
#else
    const char* const names[] = { "dussek", "dussek.app/Contents/MacOS/dussek" };
    const char sep = ':';
#endif
    std::error_code ec;
    const fs::path me = fs::weakly_canonical(fs::path(self), ec);
    const auto standalone = [&](const fs::path& dir) -> std::string {
        for (const char* n : names) {
            const fs::path c = dir / n;
            std::error_code e;
            // not the plugin itself (a macOS VST3's or CLAP's binary is named dussek too)
            if (fs::is_regular_file(c, e) && !fs::equivalent(c, me, e))
                return c.string();
        }
        return std::string();
    };
    fs::path dir = me.parent_path();
    for (int up = 0; up < 5 && !dir.empty(); ++up, dir = dir.parent_path()) {
        if (std::string s = standalone(dir); !s.empty())
            return s;
        if (dir == dir.root_path())
            break;
    }
    for (std::string rest = path != nullptr ? path : ""; !rest.empty();) {
        const size_t end = rest.find(sep);
        const std::string d = rest.substr(0, end);
        rest = end == std::string::npos ? std::string() : rest.substr(end + 1);
        if (!d.empty())
            if (std::string s = standalone(d); !s.empty())
                return s;
    }
    return std::string();
}

DataDirs dataDirs()
{
    return dataDirs([](const char* name) { return static_cast<const char*>(std::getenv(name)); }, hostPlatform());
}

std::string folderOf(const std::string& file)
{
#ifdef _WIN32
    const size_t cut = file.find_last_of("/\\");
#else
    const size_t cut = file.find_last_of('/');
#endif
    if (cut == std::string::npos)
        return std::string();
    if (cut == 0)
        return file.substr(0, 1); // "/"
#ifdef _WIN32
    if (cut == 2 && file[1] == ':')
        return file.substr(0, 3); // "C:\"
#endif
    return file.substr(0, cut);
}

std::string fileNameOf(const std::string& file)
{
#ifdef _WIN32
    const size_t cut = file.find_last_of("/\\");
#else
    const size_t cut = file.find_last_of('/');
#endif
    return cut == std::string::npos ? file : file.substr(cut + 1);
}

} // namespace pe
