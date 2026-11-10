// Presets (spec 2026-10-27-presets-design): a saved set of values covering part of the plugin.
// Picking one changes only what it covers: the microphones (the Mics tab's microphones, for one
// body type: Custom positions are in the body's coordinates) or everything (every user parameter
// and the sound-carrying states, the instrument among them). Shipped presets are defined here;
// the user's are files in presetDir(), one per preset, a key = value per line, parameters by symbol.
// Pure (no UI, no DSP): tested by ui_tests.
#pragma once
#include "PluginKind.hpp"
#include "StateVersion.hpp"

#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace pe {

enum class PresetScope : uint32_t { Mics, All };

struct Preset {
    std::string name;
    /// the state version it was written at (its file's `version`; without one the first): its states
    /// are migrated from it (StateVersion.hpp)
    uint32_t version = kFirstStateVersion;
    PresetScope scope = PresetScope::Mics;
    /// the body type a microphone preset belongs to (pe_mic_geometry's index; ignored for All)
    uint32_t body = 0;
    /// parameter symbol → value (only the scope's; an unknown symbol is skipped when applied)
    std::vector<std::pair<std::string, float>> values;
    /// state key → value (All only: the instrument, the velocity curve, the morph, the felt, the Scala file)
    std::vector<std::pair<std::string, std::string>> states;
    bool shipped = false;
    /// the user's preset's file (empty: shipped)
    std::string file;
};

/// the parameter is a microphone preset's (the Mics tab's microphones: position, distance, spacing,
/// the Custom pair, Distance level; not the lid, the pillow, spread keys, the room)
bool micScoped(uint32_t param);
/// the states a full preset carries (not the UI's)
bool presetState(const std::string& key);

/// a body type's name in a preset file (the engine's, pe_body_name: as an instrument.json names it),
/// and back (false: unknown)
const char* bodyName(uint32_t body);
bool bodyIndex(const std::string& name, uint32_t& body);

/// a plugin's shipped presets: Dussek's the grand's and the upright's microphones, then the full presets
/// (Default, then the repository's plugin/presets/<plugin>/*, embedded by the build)
const std::vector<Preset>& shippedPresets(PluginKind k);
/// the shipped full presets, Default first, then in their files' order (the plugin's programs)
std::vector<const Preset*> shippedFullPresets(PluginKind k);

/// a preset as its file's text ("# <plugin> preset" first), and read back (false: not a preset: no name or
/// no scope, a mic preset without a known body, a text the other plugin wrote)
std::string formatPreset(PluginKind k, const Preset& p);
bool parsePreset(PluginKind k, const std::string& text, Preset& out);

/// the user's presets' folder: the user data folder's identity(k).presetFolder (pe::dataDirs; empty: none)
std::string presetDir(PluginKind k);
/// the user's presets in `dir` (unreadable or foreign files skipped), by name
std::vector<Preset> loadUserPresets(PluginKind k, const std::string& dir);
/// a file name for a preset's name in `dir`: the name made safe (letters, digits, - _ and spaces kept)
std::string presetFile(PluginKind k, const std::string& dir, const std::string& name);
/// writes the preset to presetFile(dir, p.name) (the folder made if missing); false and `error` on failure
bool saveUserPreset(PluginKind k, const std::string& dir, const Preset& p, std::string& error);
bool deleteUserPreset(const Preset& p);

/// the presets offered for a scope and body: the shipped first, then the user's, each by name
std::vector<const Preset*> presetsFor(PluginKind k, PresetScope scope, uint32_t body, const std::vector<Preset>& user);

/// a preset's values as (parameter index, value), its unknown symbols and the parameters outside the
/// plugin's ports skipped; a full preset's are every parameter's, the plugin's default where the file has none
std::vector<std::pair<uint32_t, float>> presetParams(PluginKind k, const Preset& p);
/// a full preset's states, every one it carries, the default (empty) where the file has none (a mic
/// preset: none)
std::vector<std::pair<std::string, std::string>> presetStates(const Preset& p);
/// the preset whose values all equal `values` (indexed by parameter) within half a step of each's
/// display, among `offered` (-1: none): the preset the controls still have
int matchingPreset(PluginKind k, const std::vector<const Preset*>& offered, const float* values);

/// a Preset row's menu: the offered presets by name, "Save as…", and for the user's preset the
/// controls still have (`current`) a delete, armed by a first click (the second deletes)
enum : int { kPresetSaveAs = -1, kPresetDelete = -2 };
struct PresetMenu {
    std::vector<std::string> items;
    /// per item: the offered preset's index, kPresetSaveAs or kPresetDelete
    std::vector<int> actions;
};
PresetMenu presetMenu(const std::vector<const Preset*>& offered, int current, bool deleteArmed);
/// the row's value: the preset the controls still have, else "None"
std::string presetLabel(const std::vector<const Preset*>& offered, int current);

/// a preset of `scope` from the current values (`values` by parameter index; the plugin's ports only) and,
/// for All, the states
Preset presetFrom(PluginKind k, const std::string& name, PresetScope scope, uint32_t body, const float* values,
                  const std::vector<std::pair<std::string, std::string>>& states);

} // namespace pe
