// The standalone's command line: `dussek render INSTRUMENT IN.mid OUT.wav [options]` renders a
// MIDI file as midi2wav does (pe_cli, the engine's code), then exits; `dussek list` prints the
// instruments the standalone finds (exit 1: none); anything else starts the standalone as usual. It runs before DPF's main (which has no hook for it): a constructor on Linux
// and macOS (glibc and dyld pass it the arguments), a static initializer on Windows. Linked into the
// standalone only. On Linux it also links the standalone's outputs to the playback (`autoconnect`).
#include "Autoconnect.hpp"
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
#ifdef __linux__
#include <chrono>
#include <dlfcn.h>
#include <thread>
#include <vector>
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

#ifdef __linux__
// Under PipeWire nothing links a JACK client's ports, and DPF's standalone links none: it played into
// nothing unless run-standalone.sh linked it. A second JACK client (libjack loaded as DPF loads it)
// waits up to 10 s for the standalone's outputs and, if nothing else linked them within half a second
// of their coming (a session manager, the user's routing), links them to the first playback device
// (pe::autoconnectPlan), then closes. No JACK server (the native fallback): nothing.
void autoconnect()
{
    using Open = void* (*)(const char*, int, int*, ...);
    using GetPorts = const char** (*)(void*, const char*, const char*, unsigned long);
    using Connect = int (*)(void*, const char*, const char*);
    using ByName = void* (*)(void*, const char*);
    using Connected = int (*)(const void*);
    using Free = void (*)(void*);
    using Close = int (*)(void*);
    void* lib = dlopen("libjack.so.0", RTLD_NOW | RTLD_LOCAL);
    if (lib == nullptr)
        return;
    const auto jackOpen = reinterpret_cast<Open>(dlsym(lib, "jack_client_open"));
    const auto jackGetPorts = reinterpret_cast<GetPorts>(dlsym(lib, "jack_get_ports"));
    const auto jackConnect = reinterpret_cast<Connect>(dlsym(lib, "jack_connect"));
    const auto jackPortByName = reinterpret_cast<ByName>(dlsym(lib, "jack_port_by_name"));
    const auto jackConnected = reinterpret_cast<Connected>(dlsym(lib, "jack_port_connected"));
    const auto jackFree = reinterpret_cast<Free>(dlsym(lib, "jack_free"));
    const auto jackClose = reinterpret_cast<Close>(dlsym(lib, "jack_client_close"));
    if (!jackOpen || !jackGetPorts || !jackConnect || !jackPortByName || !jackConnected || !jackFree || !jackClose)
        return;
    constexpr int kNoStartServer = 0x01;
    constexpr unsigned long kIsInput = 0x1, kIsOutput = 0x2, kIsPhysical = 0x4;
    constexpr const char* kAudio = "32 bit float mono audio";
    int status = 0;
    void* client = jackOpen("Dussek autoconnect", kNoStartServer, &status);
    if (client == nullptr)
        return;
    const auto names = [&](const char* pattern, unsigned long flags) {
        std::vector<std::string> out;
        if (const char** p = jackGetPorts(client, pattern, kAudio, flags)) {
            for (const char** q = p; *q != nullptr; ++q)
                out.emplace_back(*q);
            jackFree(p);
        }
        return out;
    };
    const auto unlinked = [&](const std::vector<std::string>& ports) {
        for (const std::string& p : ports)
            if (void* port = jackPortByName(client, p.c_str()); port == nullptr || jackConnected(port) > 0)
                return false;
        return true;
    };
    // the standalone's client is named Dussek (JACK numbers a second one: Dussek-01)
    const char* const ours = "^Dussek(-[0-9]+)?:audio_out_[0-9]+$";
    for (int i = 0; i < 100; ++i) {
        std::vector<std::string> outs = names(ours, kIsOutput);
        if (outs.size() >= 2) {
            std::this_thread::sleep_for(std::chrono::milliseconds(500));
            outs = names(ours, kIsOutput);
            if (outs.size() >= 2 && unlinked(outs))
                for (const auto& [from, to] : pe::autoconnectPlan({ outs[0], outs[1] }, names(nullptr, kIsInput | kIsPhysical)))
                    jackConnect(client, from.c_str(), to.c_str());
            break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    jackClose(client);
}
#endif

#ifdef _WIN32
const int kDispatched = (dispatch(__argc, __argv), 0);
#else
__attribute__((constructor)) void dispatchAtStart(int argc, char** argv, char**)
{
    dispatch(argc, argv);
#ifdef __linux__
    if (pe::autoconnectWanted(std::getenv("DUSSEK_AUTOCONNECT")))
        std::thread(autoconnect).detach();
#endif
}
#endif

} // namespace
