// MIDI learn on the DSP side (tested in ui_tests): the ties as the audio thread reads them (a
// lock-free table the worker fills from the settings file), the current instrument's Mallet and
// Preparation menus (their CC bands follow them), and the router that decides what an incoming
// CC or program change does: caught for a control that waits, a tie's value, an instrument step,
// or the plugin's built-in handling.
#pragma once
#include "MidiMap.hpp"

#include <atomic>
#include <cstdint>

namespace pe {

/// kDiagLearnIn's value as the router takes it: 0 (no wait) … kTargetCount (target + 1); a host's
/// value outside (a saved session, garbage) is no wait
uint32_t learnInput(float v);

/// channel × CC × side (the value 0, any other) → target, -1 none: a control on both sides, a step
/// target on the side it was learned on, a relative knob's on both (its flag in the same word);
/// writer: the worker, reader: the audio thread (each entry atomic: a reload seen half done mixes old
/// and new entries, never a torn one)
class TieTable {
public:
    TieTable();
    void load(const MidiMap& m);
    struct Entry {
        int target; // -1: none
        bool relative, flip;
    };
    Entry entry(uint8_t channel, uint8_t cc, uint8_t value) const
    {
        const int16_t e = fTarget[value == 0][channel & 15][cc & 127].load(std::memory_order_relaxed);
        if (e < 0)
            return { -1, false, false };
        return { e & ~(kRelative | kFlip), (e & kRelative) != 0, (e & kFlip) != 0 };
    }
    int target(uint8_t channel, uint8_t cc, uint8_t value) const { return entry(channel, cc, value).target; }
    /// whether either side of the CC is tied
    bool tied(uint8_t channel, uint8_t cc) const { return target(channel, cc, 0) >= 0 || target(channel, cc, 1) >= 0; }
    bool programChange() const { return fProgramChange.load(std::memory_order_relaxed); }
    bool softTakeover() const { return fSoftTakeover.load(std::memory_order_relaxed); }

private:
    static constexpr int16_t kRelative = 0x4000, kFlip = 0x2000;
    std::atomic<int16_t> fTarget[2][16][128];
    std::atomic<bool> fProgramChange { false };
    std::atomic<bool> fSoftTakeover { true };
};

/// the current instrument's Mallet and Preparation menus; writer: the worker, reader: the audio thread
class InstrumentLists {
public:
    InstrumentLists();
    void publish(const InstrumentEntry& e);
    /// listChoices for the published instrument (count 0: not a list)
    ListChoices choices(uint32_t param) const;

private:
    std::atomic<uint32_t> fMallets { 1 }, fPreps { 1 };
    std::atomic<uint8_t> fPrep[ListChoices::kMax];
};

/// Soft takeover for absolute ties to sliders (audio thread): after the slider moved otherwise (the
/// mouse, the host, a preset: its value is not the one MIDI set last) a CC moves it only once it comes
/// within kPickup of the slider or passes it between two of its values; lists and switches take every
/// value
class Takeover {
public:
    static constexpr float kPickup = 0.02f; // of the slider's travel
    Takeover();
    /// whether `next` (the CC's value as the slider's) may set `param` now at `current`
    bool accept(uint32_t param, float next, float current);

private:
    float fSet[kParamCount]; // the value MIDI set last (NaN: none yet)
    float fLast[kParamCount]; // the CC's last value, normalized (NaN: none yet)
};

/// audio thread only
class MidiRouter {
public:
    struct Action {
        enum Kind { None, Builtin, Learned, Set, Nudge, Flip, Step, Program } kind = None;
        uint32_t param = 0;   // Set, Nudge, Flip
        float value = 0;      // Set
        int step = 0;         // Step: +n next, -n previous; Nudge: steps (nudgeValue)
        uint32_t program = 0; // Program: 0-based
        uint32_t learned = 0; // Learned: packLearned
    };
    /// learnIn: the kDiagLearnIn parameter (0: nothing waits, else target + 1). While a control
    /// waits and nothing was caught for it yet, a learnable CC is caught (Learned, with the side its
    /// value is on) and an unlearnable one passes (Builtin); otherwise a tied CC sets its control
    /// (Set) or steps the instrument once per value on its tie's side (Step; the other side None),
    /// an untied one passes (Builtin). A relative knob's moves its control (Nudge) or the instrument
    /// (Step), a list's and the instrument's every kRelativeStepsPerEntry steps (a turn back starts
    /// over: it answers at once). A flipping pad's press flips its switch (Flip), its release None
    Action cc(uint8_t channel, uint8_t cc, uint8_t value, uint32_t learnIn, const TieTable& t, const InstrumentLists& l);
    /// kDiagLearnIn at every block, CCs or not: a wait that ends and starts again for the same
    /// control between two CCs catches anew
    void setLearn(uint32_t learnIn);
    /// Program while the switch is on, else None
    Action program(uint8_t program, const TieTable& t) const;

private:
    uint32_t fLearnIn = 0;
    bool fCaught = false;
    int fGathered[kTargetCount] {}; // a relative knob's steps short of an entry, per target
    /// steps gathered to whole entries (returned; the rest kept), a turn back starting over
    int gather(uint32_t target, int steps);
};

} // namespace pe
