// What the UI knows about an instrument: its catalog entry (the engine's report and the
// export's menu block, Catalog.hpp), never its name.
#pragma once
#include "Catalog.hpp"

#include <bitset>
#include <cstdint>
#include <string>

namespace pe {
/// the microphone geometry's body (pe_mic_geometry's number: engine/src/mic.rs BODIES)
enum class BodyKind : uint32_t { Grand = 0, Upright = 1, Harp = 2, LeverHarp = 3, Vibraphone = 4, Marimba = 5, Xylophone = 6, Bells = 7, Crotales = 8,
    Guitar = 9, Chimes = 10, HandChimes = 11, Kalimba = 12, Balafon = 13,
    Psaltery = 14, DanTranh = 15, Strumstick = 16 };
/// what the UI shows for a missing instrument: no stops, no mallets, no preparations, a grand's body
const InstrumentEntry& noInstrument();
BodyKind entryBody(const InstrumentEntry& e);
/// the instrument's own mallets (the Mallet menu's values 0 … count-1; 0: no choice)
uint32_t entryMallets(const InstrumentEntry& e);
/// the menu's label for mallet i: the menu block's display name, else the file's name with its
/// first letter capitalized
std::string malletLabel(const InstrumentEntry& e, uint32_t i);
/// the mallet the instrument plays for the stored value (its first past its count)
uint32_t effectiveMallet(const InstrumentEntry& e, uint32_t value);
/// the lowest and highest recorded key (MIDI): the strip tints the keys beyond as extrapolated
void entryRecorded(const InstrumentEntry& e, int& lo, int& hi);
/// the soft pedal acts (some note is struck: the engine ignores it on plucked notes)
bool entrySoftPedal(const InstrumentEntry& e);
/// the label the UI shows (the instrument's own words, the exciter's; wear aspects without the "Wear: " prefix)
const char* displayName(uint32_t param, const InstrumentEntry& e);
/// The tooltip's text for param on this instrument (its own where the generic one does not fit).
const char* displayDescription(uint32_t param, const InstrumentEntry& e);
/// false: the control has no effect now (the UI hides it). The instrument's part is the engine's
/// report (engine/src/info.rs, checked by engine/tests/no_effect.rs); spacing and distance do
/// nothing at "Recording"; each preparation's own controls work only while it is the chosen one
/// (`prep`: the Preparation parameter).
bool paramActive(uint32_t param, const InstrumentEntry& e, uint32_t micPosition, uint32_t prep);
/// what the shown controls depend on besides the window: the instrument (nullptr: missing), the
/// microphone position and the Preparation parameter (as stored); MIDI learn: the parameters
/// tied to a CC (their labels leave room for the tag), the number of ties and whether Next /
/// Previous instrument are among them (the MIDI tab)
struct UiContext {
    const InstrumentEntry* inst = nullptr;
    uint32_t micPosition = 0, prep = 0;
    std::bitset<128> tied; // by parameter index (kParamCount fits: Params.hpp)
    uint32_t midiTies = 0;
    bool nextTied = false, previousTied = false, knobTied = false;
    uint32_t partners = 0; // the morph's partners (the Morph tab's rows after the main one)
};
/// the preparations that act on an instrument (Prep indices: 1 plucked, 2 muted, 3 harmonic,
/// 4 bolt, 5 paper; None, 0, is always there and not listed), in menu order, into `values`
/// (room for 5); the count
uint32_t entryPreparations(const InstrumentEntry& e, uint32_t* values);
/// the preparation an instrument plays for the Preparation parameter's value: the value where
/// the instrument has it, else None (the parameter keeps its value: switching back restores it)
uint32_t effectivePrep(const InstrumentEntry& e, uint32_t value);
/// shown: the control applies now (paramActive with the effective preparation)
bool paramVisible(uint32_t param, const UiContext& c);
/// the 8' II and the lute pluck the same strings: switching one on switches this one off
/// (kParamCount: none)
uint32_t exclusiveStop(uint32_t param);
}
