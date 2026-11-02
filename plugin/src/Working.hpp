// The bottom bar's "updating…": whether a change is still on its way to the sound (pure; tested in
// ui_tests). The DSP says when work is pending (kDiagWorkingOut, set at every block); the UI shows
// it only once it has lasted a moment, and then for a moment at least, so a quick change shows
// nothing and a slow one no flicker.
#pragma once
#include <cstdint>

namespace pe {

/// The DSP side: a change asked for an engine (`requested`, the worker's request count) that the
/// worker has not built and offered yet (`settled`: the last request whose engine it offered, or
/// found none for), a felt's leveling still to be measured after a quick build (`refineDue`: that
/// engine plays, the measured one follows), or an engine offered that the audio thread has not taken
/// yet (`offered`). A tuning change becomes a request when the worker next looks (within 50 ms).
inline bool workPending(uint64_t requested, uint64_t settled, bool refineDue, bool offered)
{
    return requested != settled || refineDue || offered;
}

constexpr const char* kWorkingLabel = "updating…";
constexpr uint64_t kWorkingDelayMs = 200; // pending this long before it shows
constexpr uint64_t kWorkingHoldMs = 500;  // once shown, at least this long

/// The UI side: fed the DSP's flag at every idle, says whether "updating…" shows.
class WorkingIndicator {
public:
    /// the DSP's flag at `nowMs`; true when what shows changed (a repaint)
    bool update(bool pending, uint64_t nowMs)
    {
        const bool before = fShown;
        if (pending && !fPending)
            fSince = nowMs;
        fPending = pending;
        if (!fShown && fPending && nowMs - fSince >= kWorkingDelayMs) {
            fShown = true;
            fShownAt = nowMs;
        } else if (fShown && !fPending && nowMs - fShownAt >= kWorkingHoldMs) {
            fShown = false;
        }
        return fShown != before;
    }
    bool visible() const { return fShown; }

private:
    bool fPending = false, fShown = false;
    uint64_t fSince = 0, fShownAt = 0;
};

} // namespace pe
