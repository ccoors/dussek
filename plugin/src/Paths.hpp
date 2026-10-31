#pragma once
#include <functional>
#include <string>

namespace pe {

enum class Platform { Linux, MacOS, Windows };

/// the platform this build runs on
Platform hostPlatform();

/// the folders for the user's own files (an empty string: the environment names none)
struct DataDirs {
    std::string data;   ///< instruments added, snapshots, presets
    std::string config; ///< MIDI maps
};

/// Linux: $XDG_DATA_HOME/dussek and $XDG_CONFIG_HOME/dussek (absolute ones only), else
/// ~/.local/share/dussek and ~/.config/dussek; macOS: ~/Library/Application Support/Dussek
/// for both; Windows: %APPDATA%\Dussek for both. `env` reads a variable (nullptr: unset).
DataDirs dataDirs(const std::function<const char*(const char*)>& env, Platform platform);

/// the process's: its environment, this platform
DataDirs dataDirs();

/// The standalone's executable, for Copy CLI's command line: `self` the running binary
/// (DPF's getBinaryFilename) and whether it is the standalone; in a plugin, the standalone next to its
/// bundle (links resolved: `just install` links the bundles to plugin/build/bin), up to four folders
/// up (VST3's Contents/<arch>, a macOS bundle's Contents/MacOS), then in the folders of `path` (the
/// PATH variable; null: none). Empty: none found.
std::string standaloneExecutable(const std::string& self, bool selfIsStandalone, const char* path);

} // namespace pe
