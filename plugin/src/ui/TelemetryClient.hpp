// The UI's side of the diagnostics region: open it by the DSP's token, switch the
// engine's telemetry on or off, take frames and audio. All reader logic is in the engine
// library (pe_telemetry_*), so the protocol lives in one place.
#pragma once
#include "SharedRegion.hpp"
#include "dussek.h"

#include <memory>

namespace pe {

class TelemetryClient {
public:
    ~TelemetryClient() { close(); }
    /// false: token 0, or no such region (sandbox, other machine). First closes the previous
    /// region, which CLEARS ITS ENABLED FLAG: after a successful open() the caller must call
    /// setEnabled(true) again (the new region's flag is whatever it was). The audio cursor
    /// starts at the writer's current position: only audio written after the open is returned.
    /// `ownsFlag` false: a reader beside the diagnostics (the Touch tab's notes): close() and the
    /// destructor leave the enabled flag as it is. The token already open (as an LV2 host re-sends
    /// the port unchanged): nothing changes, the cursors keep their place.
    bool open(uint32_t token, bool ownsFlag = true);
    void close();              // clears the enabled flag (when it owns it)
    /// Switch the engine's telemetry off for a region this client never opened (e.g. a UI that
    /// crashed left the flag on): opens it, clears the flag, unmaps. false: no such region.
    static bool clearFlag(uint32_t token);
    bool isOpen() const { return fRegion != nullptr; }
    void setEnabled(bool on);
    /// the newest frame not yet taken
    bool pollFrame(PeTelemetryFrame& f);
    /// interleaved stereo output frames since the last call (<= maxFrames, clamped to 2^32-1); skips ahead when overrun
    size_t pollAudio(float* interleaved, size_t maxFrames);
    uint32_t dropped() const;
    /// the incoming note-on velocities since the last call (or the open), oldest first (<= max)
    size_t pollNotes(uint8_t* velocities, size_t max);
    /// the value mirror's slot index: its sequence (0: never set) and value; false: not open, out of range
    bool readValue(uint32_t index, uint32_t& seq, float& value) const;

private:
    std::unique_ptr<SharedRegion> fRegion;
    uint64_t fCursor = 0;
    uint64_t fNotesCursor = 0;
    bool fOwnsFlag = true;
    uint32_t fToken = 0;
};

} // namespace pe
