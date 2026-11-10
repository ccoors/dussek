#pragma once
#include <string>
// A display that offers no OpenGL 2 (Windows' own 1.1 renderer: Remote Desktop, a virtual machine
// without 3D acceleration): NanoVG gets no context and the window stays black. The player is told.

#include "PluginKind.hpp"

namespace pe {

/// what the player is told
std::string openGlMissingText(PluginKind k);

/// the text on standard error and, on Windows, in a message box (once per process)
void reportMissingOpenGl(PluginKind k);

} // namespace pe
