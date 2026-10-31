// The standalone's command line: `pianoengine render INSTRUMENT IN.mid OUT.wav [options]` renders a
// MIDI file as midi2wav does (pe_cli, the engine's code), then exits; anything else starts the
// standalone as usual. It runs before DPF's main (which has no hook for it): a constructor on Linux
// and macOS (glibc and dyld pass it the arguments), a static initializer on Windows. Linked into the
// standalone only.
#include "pianoengine.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

namespace {

void dispatch(int argc, char** argv)
{
    if (argc < 2 || argv == nullptr || argv[1] == nullptr || std::strcmp(argv[1], "render") != 0)
        return;
#ifdef _WIN32
    // a window program: print into the console it was started from, if any
    if (AttachConsole(ATTACH_PARENT_PROCESS)) {
        (void)std::freopen("CONOUT$", "w", stdout);
        (void)std::freopen("CONOUT$", "w", stderr);
    }
#endif
    const int code = pe_cli(argc - 1, argv + 1);
    std::fflush(stdout);
    std::exit(code);
}

#ifdef _WIN32
const int kDispatched = (dispatch(__argc, __argv), 0);
#else
__attribute__((constructor)) void dispatchAtStart(int argc, char** argv, char**)
{
    dispatch(argc, argv);
}
#endif

} // namespace
