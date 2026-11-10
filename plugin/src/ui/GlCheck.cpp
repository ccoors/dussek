#include "GlCheck.hpp"

#include <atomic>
#include <cstdio>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

namespace pe {

std::string openGlMissingText(PluginKind k)
{
    return std::string(identity(k).name) + "'s window needs OpenGL 2 or newer, and this display offers an older one (Remote Desktop, "
           "or a virtual machine without 3D acceleration): the window stays black. The sound still plays. On "
           "the computer itself, with its graphics driver, the window draws as usual.";
}

void reportMissingOpenGl(PluginKind k)
{
    static std::atomic<bool> told { false };
    if (told.exchange(true))
        return;
    const std::string text = openGlMissingText(k);
    std::fprintf(stderr, "%s: %s\n", identity(k).name, text.c_str());
#ifdef _WIN32
    MessageBoxA(nullptr, text.c_str(), identity(k).name, MB_OK | MB_ICONWARNING);
#endif
}

} // namespace pe
