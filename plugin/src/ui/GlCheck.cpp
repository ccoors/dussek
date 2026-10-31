#include "GlCheck.hpp"

#include <atomic>
#include <cstdio>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

namespace pe {

const char* openGlMissingText()
{
    return "Dussek's window needs OpenGL 2 or newer, and this display offers an older one (Remote Desktop, "
           "or a virtual machine without 3D acceleration): the window stays black. The sound still plays. On "
           "the computer itself, with its graphics driver, the window draws as usual.";
}

void reportMissingOpenGl()
{
    static std::atomic<bool> told { false };
    if (told.exchange(true))
        return;
    std::fprintf(stderr, "Dussek: %s\n", openGlMissingText());
#ifdef _WIN32
    MessageBoxA(nullptr, openGlMissingText(), "Dussek", MB_OK | MB_ICONWARNING);
#endif
}

} // namespace pe
