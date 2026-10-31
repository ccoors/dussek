#pragma once
// The plugin state's version (the `_state_version` key): a state saved by an older release is
// migrated key by key as it is restored, a newer one read as far as it is understood
// (docs/compatibility.md). DPF restores it before the others: LV2 in declared order (it is declared
// first), VST3 and CLAP in key order (their std::map; the leading `_` sorts it first).
#include <array>
#include <cstdint>
#include <string>

namespace pe {

/// the state's form this build writes; raised with a migration in migrateState
constexpr uint32_t kStateVersion = 1;
/// the first released form: what a value without a version is (a preset without its line)
constexpr uint32_t kFirstStateVersion = 1;
constexpr const char* kStateVersionKey = "_state_version";

/// the plugin's state keys, in declared order (PianoPlugin::initState)
constexpr std::array<const char*, 7> stateKeys()
{
    return { kStateVersionKey, "ui", "instrument", "velocity_curve", "morph", "felt", "scale_file" };
}

/// a saved version: a number as given (also a newer one); missing, empty or not a number:
/// kFirstStateVersion
uint32_t parseStateVersion(const char* value);

/// a key's value saved at version `from`, in today's form. Each migration (when one comes: a renamed
/// key's value, a changed form) leaves a value already in today's form unchanged, so a version left
/// over from an earlier restore never harms a live change.
std::string migrateState(uint32_t from, const std::string& key, std::string value);

/// one of the plugin's state keys (unknown ones are ignored, as a newer release's)
bool stateKnown(const std::string& key);

/// a restore's keys as they come: the version key sets the version the others are migrated from (it
/// stays until the next one; a fresh instance's is today's: a live change before any restore)
class StateRestore {
public:
    std::string apply(const std::string& key, const std::string& value);
    uint32_t version() const { return fVersion; }

private:
    uint32_t fVersion = kStateVersion;
};

} // namespace pe
