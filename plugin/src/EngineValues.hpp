/*
 * The parameters as the engine's values: the live parameters (PeParams), the string tuning
 * (PeTuning), the instrument's directory. One mapping for the DSP and the UI's Copy CLI
 * button (the midi2wav command line for what plays). Pure, any thread.
 */
#pragma once
#include "Params.hpp"
#include "dussek.h"

#include <string>
#include <utility>
#include <vector>

namespace pe {

/// a level in dB as a linear gain; at or below kOffDb off (0)
float dbGain(float db);

/// the wear (0 … 1) the Condition slider (0 … 100 %) gives aspect `index` (kWearHammers …
/// kWearSoundboard), linear between its zones' points: the voicing (each key's level and hardness)
/// Mint 0 % → 0, Normal 10 % → 0.05, Worn 35 % → 0.2, Broken 100 % → 1; every other aspect none
/// through Normal, then 0.2 at 35 %, 1 at 100 %
double conditionWear(double percent, uint32_t index);

/// the zone a Condition value lies in: "Mint" (under 1 %), "Normal" (to 10 %), "Worn" (to 35 %),
/// "Broken"
const char* conditionZone(double percent);

/// a zone's name (any case) as the Condition value at its upper edge; false if `name` is none
bool conditionZoneValue(const std::string& name, float& out);

/// the live parameters
PeParams engineParams(const float* values);

/// the lid as the engine takes it: 0 closed … 1 open (Lid), 2 taken off (Lid removed)
double engineLid(const float* values);

/// the string tuning. What does nothing at the microphone position stays at its default (the
/// lid and the spread keys at the recording's; the free pair elsewhere), and so does the bolt's
/// position without a bolt: changing those builds no new engine, nor shows in the CLI line
PeTuning engineTuning(const float* values);

/// every field equal
bool sameTuning(const PeTuning& a, const PeTuning& b);

/// the directory of instrument `id` (a directory's name, the "instrument" state): the catalog's
/// entry for it, the first search path (Catalog::searchPaths: $DUSSEK_INSTRUMENTS,
/// `resources`/instruments, the user's data directory) where it is an instrument the engine
/// accepts (Catalog::entryAt); empty if none is
std::string instrumentDir(const char* resources, const char* id);

/// the shell command that renders a MIDI file with these settings (the standalone's `render`, which is
/// midi2wav; `midi2wav` takes the same arguments):
/// `dussek render '<dir>' in.mid out.wav --rate <rate> [--velocity-curve …] [--param …] [--tuning …] [--latch]`
/// (`curve`: the velocity curve, a state; null: straight)
/// `morph`: each partner's directory and weight (one --morph per weight above 0); `felt`: the Felt tab's
/// "felt" state (--felt where not empty); `scale`: the "scale_file" state (--scl with the temperament key
/// where the temperament is its "Scala file"); `program`: the standalone's path (standaloneExecutable;
/// empty: `dussek`, found on PATH)
std::string midi2wavLine(const float* values, const std::string& dir, double rate, const PeVelocityCurve* curve = nullptr,
    const std::vector<std::pair<std::string, float>>& morph = {}, const std::string& felt = std::string(),
    const std::string& scale = std::string(), const std::string& program = std::string());

/// Copy CLI's line for the plugin's defaults with the controls named by symbol set to the given
/// values (in the controls' own units: dB, %, …), as `dussek render`'s options; empty, with `error`
/// naming it, for an unknown symbol or a value outside the control's range. The website's clips
/// (site/render.py) name their settings so.
std::string settingsLine(const std::string& dir, const std::vector<std::pair<std::string, float>>& settings, std::string& error);

} // namespace pe
