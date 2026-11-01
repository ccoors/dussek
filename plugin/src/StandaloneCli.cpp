// The standalone's command line: `dussek render INSTRUMENT IN.mid OUT.wav [options]` renders a
// MIDI file as midi2wav does (pe_cli, the engine's code), then exits; `dussek list` prints the
// instruments the standalone finds (exit 1: none); anything else starts the standalone as usual. It runs before DPF's main (which has no hook for it): a constructor on Linux
// and macOS (glibc and dyld pass it the arguments), a static initializer on Windows. Linked into the
// standalone only.
#include "Catalog.hpp"
#include "Paths.hpp"
#include "dussek.h"

#include <cstdio>
#include <string>
#include <cstdlib>
#include <cstring>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

namespace {

// the standalone's resources folder, as DPF's getResourcePath gives it: <its folder>/resources, on
// macOS the app's Contents/Resources (empty: the executable not found)
std::string resourcesFolder()
{
    const std::string self = pe::executablePath();
    const size_t cut = self.find_last_of("/\\");
    if (cut == std::string::npos)
        return std::string();
#ifdef __APPLE__
    return self.substr(0, cut) + "/../Resources";
#else
    return self.substr(0, cut) + "/resources";
#endif
}

void dispatch(int argc, char** argv)
{
    if (argc < 2 || argv == nullptr || argv[1] == nullptr)
        return;
    const bool list = std::strcmp(argv[1], "list") == 0;
    if (!list && std::strcmp(argv[1], "render") != 0)
        return;
#ifdef _WIN32
    // a window program started from a console has no standard output (a pipe or a file it keeps): it
    // prints into that console; C's stdio and the handles the engine (Rust) writes through
    const HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    if ((out == nullptr || out == INVALID_HANDLE_VALUE || GetFileType(out) == FILE_TYPE_UNKNOWN)
        && AttachConsole(ATTACH_PARENT_PROCESS)) {
        const HANDLE con = CreateFileA("CONOUT$", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
            OPEN_EXISTING, 0, nullptr);
        if (con != INVALID_HANDLE_VALUE) {
            SetStdHandle(STD_OUTPUT_HANDLE, con);
            SetStdHandle(STD_ERROR_HANDLE, con);
        }
        (void)std::freopen("CONOUT$", "w", stdout);
        (void)std::freopen("CONOUT$", "w", stderr);
    }
#endif
    int code;
    if (list) {
        const std::string res = resourcesFolder();
        const pe::Catalog c = pe::Catalog::scan(pe::Catalog::searchPaths(res.empty() ? nullptr : res.c_str()));
        std::fputs(pe::listingText(c).c_str(), stdout);
        code = c.menu().empty() ? 1 : 0;
    } else {
        code = pe_cli(argc - 1, argv + 1);
    }
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
