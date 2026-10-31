#include "TelemetryClient.hpp"

#include <algorithm>
#include <cstdint>

namespace pe {

bool TelemetryClient::open(uint32_t token, bool ownsFlag)
{
    if (fRegion && token == fToken && ownsFlag == fOwnsFlag)
        return true;
    close();
    fOwnsFlag = ownsFlag;
    fToken = token;
    if (token == 0)
        return false;
    fRegion = SharedRegion::open(SharedRegion::nameForToken(token), pe_telemetry_region_size());
    // start at the writer's position: audio and notes written before the open are old, not for the picture
    fCursor = fRegion ? pe_telemetry_audio_head(fRegion->data(), fRegion->size()) : 0;
    fNotesCursor = fRegion ? pe_telemetry_notes_head(fRegion->data(), fRegion->size()) : 0;
    return fRegion != nullptr;
}

bool TelemetryClient::clearFlag(uint32_t token)
{
    if (token == 0)
        return false;
    auto r = SharedRegion::open(SharedRegion::nameForToken(token), pe_telemetry_region_size());
    return r && pe_telemetry_set_enabled(r->data(), r->size(), false);
}

void TelemetryClient::close()
{
    if (fRegion && fOwnsFlag)
        pe_telemetry_set_enabled(fRegion->data(), fRegion->size(), false);
    fRegion.reset();
}

void TelemetryClient::setEnabled(bool on)
{
    if (fRegion)
        pe_telemetry_set_enabled(fRegion->data(), fRegion->size(), on);
}

bool TelemetryClient::pollFrame(PeTelemetryFrame& f)
{
    return fRegion && pe_telemetry_read_frame(fRegion->data(), fRegion->size(), &f);
}

size_t TelemetryClient::pollAudio(float* out, size_t maxFrames)
{
    if (!fRegion)
        return 0;
    return pe_telemetry_read_audio(fRegion->data(), fRegion->size(), &fCursor, out, (uint32_t)std::min<size_t>(maxFrames, UINT32_MAX));
}

size_t TelemetryClient::pollNotes(uint8_t* velocities, size_t max)
{
    if (!fRegion)
        return 0;
    return pe_telemetry_read_notes(fRegion->data(), fRegion->size(), &fNotesCursor, velocities, (uint32_t)std::min<size_t>(max, UINT32_MAX));
}

bool TelemetryClient::readValue(uint32_t index, uint32_t& seq, float& value) const
{
    return fRegion && pe_telemetry_read_value(fRegion->data(), fRegion->size(), index, &seq, &value);
}

uint32_t TelemetryClient::dropped() const
{
    return fRegion ? pe_telemetry_dropped(fRegion->data(), fRegion->size()) : 0;
}

} // namespace pe
