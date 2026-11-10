#pragma once
// The two plugins built from these sources: Dussek (the keyboard instruments) and Weidinger (the
// brass, later the winds). Each target's DistrhoPluginInfo.h (src/plugins/<name>/) defines
// PE_PLUGIN_KIND; the shared code (pe_common) takes the kind as a value.
#include <cstdint>

namespace pe {

enum class PluginKind : uint32_t { Keys = 0, Winds = 1 };

/// what tells the two plugins apart by name: the host's, the files' and the folders'
struct Identity {
    const char* name;
    const char* description;
    const char* uri;    // = DISTRHO_PLUGIN_URI (checked at compile time)
    const char* clapId; // = DISTRHO_PLUGIN_CLAP_ID
    const char* midiMapFile;     // in the user's data directory
    const char* presetFolder;    // in the user's data directory
    const char* presetExtension; // a user preset's file
    const char* executable;      // the standalone's
    unsigned audioInputs;        // = DISTRHO_PLUGIN_NUM_INPUTS: the strings hear an input (Audio input), a bell none
};

constexpr const char* kPluginBrand = "Christian Friedrich Coors";

constexpr Identity identity(PluginKind k)
{
    return k == PluginKind::Winds
        ? Identity{ "Weidinger", "Brass, physically modeled: trumpet, trombones, horn and tuba, played as lines",
                    "https://ccoors.de/plugins/weidinger", "de.ccoors.weidinger", "weidinger-midi-map.txt",
                    "weidinger-presets", ".weidinger-preset", "weidinger", 0 }
        : Identity{ "Dussek",
                    "Physically modeled pianos, harpsichords, a clavichord, harps, guitars, zithers and "
                    "percussion, played in real time: no samples, each instrument measured from recordings "
                    "of a real one. Strings with hammer, plectrum or mallet, soundboard, room, dampers, "
                    "pedals and sympathetic resonance.",
                    "https://ccoors.de/plugins/dussek", "de.ccoors.dussek", "midi-map.txt", "presets",
                    ".dussek-preset", "dussek", 2 };
}

constexpr bool sameText(const char* a, const char* b)
{
    while (*a && *a == *b) {
        ++a;
        ++b;
    }
    return *a == *b;
}
}
