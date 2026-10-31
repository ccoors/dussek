// The player's felt on the DSP side (design 2026-10-22-felt-controls): the "felt" state as the
// worker builds it, the live Hammer hardness beside it, the state's keeping across a restore, and the
// Original toggle's input (kDiagFeltOriginalIn). Pure: tested in ui_tests.
#pragma once
#include "pianoengine.h"

#include <string>

namespace pe {

/// What the worker builds: the curves of `text` (none while `original`, or for unreadable text) and
/// Hammer hardness (dB). The engine bends the felt of a hammer piano only; elsewhere it ignores this.
inline PeFelt feltForBuild(const std::string& text, bool original, float hardnessDb)
{
    PeFelt f {};
    if (!original)
        pe_felt_parse(text.c_str(), &f); // unreadable: none
    f.hardness_db = hardnessDb;
    return f;
}

/// The live hardness factor: 1 on a hammer piano, where the build carries it (loudness kept), else
/// the knob's.
inline float liveHardness(float factor, bool hammerPiano) { return hammerPiano ? 1.0f : factor; }

/// A build just done needs another for the hardness: a hammer piano built at `builtDb` while the knob
/// moved to `nowDb` (a move asks for a build only while a hammer piano plays: the first build after a
/// switch from another instrument missed it).
inline bool hardnessMovedDuringBuild(bool hammerPiano, float builtDb, float nowDb) { return hammerPiano && builtDb != nowDb; }

/// The "felt" state's text. A restore sets "instrument" and "felt" in the host's order, so the
/// instrument's state keeps it; a switch the user makes clears it from the UI, one MIDI makes here.
class FeltState {
public:
    void setFelt(const std::string& text) { fText = text; }
    void setInstrument(const std::string&) {}
    void midiSwitch() { fText.clear(); }
    const std::string& text() const { return fText; }

private:
    std::string fText;
};

/// The Original toggle's input, flipped at each click (as Panic's, PanicTrigger): a value a host
/// restores is the baseline, off; each change turns Original on or off. Its state goes out on
/// kDiagFeltOriginalOut, which the UI shows (a restored input never shows lit while the bends play).
class OriginalToggle {
public:
    void activate(float now)
    {
        fSeen = now;
        fOn = false;
    }
    /// the input now; true while the shipped felt plays
    bool set(float v)
    {
        if (v != fSeen) {
            fSeen = v;
            fOn = !fOn;
        }
        return fOn;
    }

private:
    float fSeen = 0.0f;
    bool fOn = false;
};

} // namespace pe
