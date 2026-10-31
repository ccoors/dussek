// MIDI learn's map (pure; tested in ui_tests): which CC drives which control, the program change
// switch, the settings file they live in (one for every instance and session: it describes the
// controller, not a sound), and a CC's value as the control's value.
#pragma once
#include "Instruments.hpp"
#include "Params.hpp"

#include <cstdint>
#include <functional>
#include <string>
#include <vector>

namespace pe {

/// a target: a parameter index, or one of the instrument actions (a press steps on, back; a relative
/// knob steps either way)
constexpr uint32_t kTargetNext = kParamCount, kTargetPrevious = kParamCount + 1, kTargetKnob = kParamCount + 2,
                   kTargetCount = kParamCount + 3;

/// false for the plugin's own CCs (64 sustain, 66 sostenuto, 67 soft, 120 and 123 panic) and past 127
bool learnableCc(uint32_t cc);
/// the parameter's symbol, "next_instrument", "previous_instrument", "instrument_knob" (nullptr: no target)
const char* targetSymbol(uint32_t target);
/// the parameter's name, "Next instrument", "Previous instrument", "Instrument (relative knob)" (nullptr: no target)
const char* targetName(uint32_t target);

/// one CC's tie; a CC value counts as a press when it is not 0 (a pad hit softly sends a low value)
struct Tie {
    uint8_t channel; // 0–15
    uint8_t cc;
    uint32_t target;
    /// Next / Previous instrument: steps on the value 0 (learned on it), else on every other value;
    /// so one CC whose + button sends 127 and − button 0 carries both, one on each side
    bool zero = false;
    /// an endless knob in relative mode: each value a move (`relativeSteps`) from where the control
    /// is, not a place; it takes the whole CC (the instrument knob's always)
    bool relative = false;
    /// a switch flipped at each press (a value but 0; a momentary pad), the release passed over
    bool flip = false;
};

/// a relative knob's value as steps: 1–63 up by that many, 65–127 down by 128 − it (127: one down);
/// 0 one up (sent at the slowest turn by some), 64 64 down
int relativeSteps(uint8_t value);
/// a relative knob's steps per entry of a list (and per whole number of an integer slider, per step
/// of the instrument): a slight touch does not skip entries
constexpr int kRelativeStepsPerEntry = 3;

/// whether the target steps the instrument (Next, Previous, the knob)
bool stepTarget(uint32_t target);

class MidiMap {
public:
    /// program change selects the instrument (off by default: many MIDI files start with one)
    bool programChange = false;
    /// soft takeover: a knob or fader tied absolute moves a slider only once it reaches the slider's
    /// value (the slider moved otherwise: the mouse, the host, a preset), so the slider never jumps
    bool softTakeover = true;

    const std::vector<Tie>& ties() const { return fTies; }
    /// ties the CC to target: the target's old tie and the CC's old one go first (one CC drives one
    /// control, a control has one CC; Next and Previous share a CC on its two sides, `zero` the side
    /// a step target was learned on, ignored for a control); an unlearnable CC or a bad target
    /// changes nothing; `relative`: an endless knob's (both sides, no `zero`); `flip`: a pad flipping a
    /// switch (ignored for anything else)
    void tie(uint32_t target, uint8_t channel, uint8_t cc, bool zero = false, bool relative = false, bool flip = false);
    /// false: target had no tie
    bool forget(uint32_t target);
    const Tie* find(uint32_t target) const;

    /// "program_change on|off", "soft_takeover on|off" and one "cc <channel 1–16> <cc 0–127> <symbol>"
    /// per tie, "… zero" for a step on the value 0, "… relative" for an endless knob's, "… flip" for a
    /// pad flipping a switch
    std::string text() const;
    /// unreadable lines, unknown symbols and unlearnable CCs are left out; the lines apply in order
    static MidiMap parse(const std::string& text);
    /// creates missing directories, writes a temporary file beside it and renames it; false on failure
    bool save(const std::string& path) const;
    /// missing or unreadable: no ties, the switch off
    static MidiMap load(const std::string& path);
    /// `edit` applied to the file as it is now (another instance may have written it since this map
    /// was read) and saved: this map becomes the result; false (the file not written): unchanged
    bool update(const std::string& path, const std::function<void(MidiMap&)>& edit);

private:
    std::vector<Tie> fTies;
};

/// midi-map.txt in the user config folder (pe::dataDirs: ~/.config/pianoengine on Linux; empty: none)
std::string midiMapPath();
/// "CC 21", "CC 21 at 0" (a step on the value 0), "CC 21 ±" (a relative knob), "CC 21 flip" (a pad flipping a switch)
std::string tieLabel(const Tie& t);
/// "1" … "16"
std::string tieChannel(const Tie& t);

/// a control's right-click menu: untied {"Learn MIDI", and for a switch "Learn MIDI (pad flips it)", else
/// "Learn MIDI (relative knob)"}, tied {"Learn again", "Learn again (…)", "Forget CC 21"}
std::vector<std::string> learnMenuItems(const MidiMap& m, uint32_t target);

/// the value mirror (pe_telemetry_set_value): a slot holds a value MIDI set that the host has not
/// heard of, NaN once the host sent a newer one. Whether the UI applies a slot: a set, live value
/// other than its own, with a sequence it has not seen (`firstRead`, the region just opened: any)
bool mirrorApplies(bool firstRead, uint32_t seq, uint32_t seenSeq, float mirror, float current);

/// a learned CC as one number for a parameter: 1 + zero × 2048 + channel × 128 + cc (0: none; zero:
/// caught on the value 0)
uint32_t packLearned(uint8_t channel, uint8_t cc, bool zero = false);
/// false for 0 or out of range
bool unpackLearned(float value, uint8_t& channel, uint8_t& cc, bool& zero);

/// a list's values in the order its menu shows them (count 0: not a list)
struct ListChoices {
    static constexpr uint32_t kMax = 16; // the longest list (Temperament key: 12)
    uint32_t count = 0;
    uint8_t values[kMax] {};
};
/// the menu the user sees: Mallet the instrument's own (at least {0}), Preparation None and the
/// instrument's own, the other lists all their entries
ListChoices listChoices(uint32_t param, const InstrumentEntry& e);
/// a CC value as param's value: a list in equal bands over `lists`, a switch off at 0 and on at 1–127
/// (a pad hit softly), a slider over its travel (0 the left end, 127 the right)
float ccToValue(uint32_t param, uint8_t value, const ListChoices& lists);
/// param's value `steps` from `current` (a relative knob): a switch on (up) or off (down), a list
/// that many entries along its menu (`lists`; a value not in it from the nearest), an integer slider
/// that many whole numbers, a slider 1/127 of its travel per step; held at the ends
float nudgeValue(uint32_t param, float current, int steps, const ListChoices& lists);

} // namespace pe
