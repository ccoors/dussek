// The sustain pedal as the engine hears it: MIDI CC64, the UI's pedal lever (a hidden
// parameter) and the sustain latch (a setting: fully down while on) combine as the deepest; the
// engine is told only when the result changes.
// Pure (no DPF, no engine), so the whole state machine is tested in ui_tests. The plugin calls
// pe_pedal_depth only for a returned Call.
//
// The stale-value rule (F4): a non-zero UI pedal value without a drag in progress is stale (a host
// snapshot taken mid-drag and restored later). The first value read after construction, after
// activate() and after a MIDI panic is muted (counts as 0) until the parameter changes again; a
// drag sets it live. Known limitation: a drag that starts at exactly the stale value does not change
// the parameter, so it stays muted until the pointer moves to another depth.
#pragma once
#include <cstdint>

namespace pe {

class PedalMixer {
public:
    struct Call {
        bool call = false; ///< tell the engine
        float depth = 0;   ///< the new depth, 0 … 1
    };

    /// CC64: a new value (0 … 127) is applied; the same value again is not a change
    Call setCc(uint8_t value);
    /// the UI pedal parameter, already clamped to 0 … 1; read once per block
    Call setUi(float value);
    /// the sustain latch (the Sustain latch parameter): fully down while on; a setting, so neither
    /// panic() nor activate() clears it
    Call setLatch(bool on);
    /// MIDI CC120/123 (the caller also releases the engine's keys): the CC pedal and the applied
    /// depth are back at 0 and a held UI pedal is muted; the engine's own reset is the caller's, so
    /// a call comes out only for the latch (the engine reset its pedal: down again)
    Call panic();
    /// the plugin is (re)activated: as panic, from the UI value read now (already clamped 0 … 1)
    Call activate(float uiNow);
    /// the depth the engine was last told (0 after construction, activate and panic)
    float applied() const { return fApplied; }
    /// a new engine that did not adopt the old one's state must be told the applied depth again
    Call replayForNewEngine() const { return { fApplied != 0.0f, fApplied }; }

private:
    Call combine();
    uint8_t fCc = 0;
    bool fLatch = false;
    // fUi: the last value read (-1: none yet, never combined); fMute: a value ignored until the
    // parameter changes (-2: none)
    float fUi = -1.0f, fMute = -2.0f, fApplied = 0;
};

/// The UI's Panic button, from its hidden input (kDiagPanicIn): the UI flips the value at each
/// click (panicNext), the audio side silences the engine on any change (fire). Activation takes the
/// value as it is, so a session restored with the input at 1 does not fire.
class PanicTrigger {
public:
    void activate(float value) { fSeen = value; }
    bool fire(float value)
    {
        if (value == fSeen)
            return false;
        fSeen = value;
        return true;
    }

private:
    float fSeen = 0.0f;
};

/// the value the UI sends at its next Panic click
inline float panicNext(float current) { return current >= 0.5f ? 0.0f : 1.0f; }

} // namespace pe
