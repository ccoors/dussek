// The JACK standalone's outputs to the system's playback: which ports to connect (pure; the JACK
// calls are StandaloneCli.cpp's). Under PipeWire nothing links a JACK client by itself.
#pragma once
#include <string>
#include <utility>
#include <vector>

namespace pe {
/// The connections for the standalone's outputs `outs` (left, right): to the first device's
/// playback ports among `physical` (JACK's physical inputs, in its order: the default sink's first
/// under PipeWire, system:playback_1/2 under JACK), output i to its port i; a device with one port
/// (mono) takes both. None without a device.
std::vector<std::pair<std::string, std::string>> autoconnectPlan(const std::vector<std::string>& outs,
    const std::vector<std::string>& physical);
/// whether the standalone links its outputs itself: unless $DUSSEK_AUTOCONNECT is "0"
bool autoconnectWanted(const char* env);
}
