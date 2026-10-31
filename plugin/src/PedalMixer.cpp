#include "PedalMixer.hpp"

#include <algorithm>

namespace pe {

PedalMixer::Call PedalMixer::combine()
{
    const float ui = fUi < 0.0f || fUi == fMute ? 0.0f : fUi; // the -1 sentinel never reaches the engine
    const float depth = fLatch ? 1.0f : std::max(fCc / 127.0f, ui);
    if (depth == fApplied)
        return {};
    fApplied = depth;
    return { true, depth };
}

PedalMixer::Call PedalMixer::setCc(uint8_t value)
{
    if (value == fCc)
        return {};
    fCc = value;
    return combine();
}

PedalMixer::Call PedalMixer::setUi(float value)
{
    if (value == fUi)
        return {};
    // the very first read is stale (muted); any later change is live (the mute is always the old value)
    fMute = fUi < 0.0f ? value : -2.0f;
    fUi = value;
    return combine();
}

PedalMixer::Call PedalMixer::setLatch(bool on)
{
    if (on == fLatch)
        return {};
    fLatch = on;
    return combine();
}

PedalMixer::Call PedalMixer::panic()
{
    fCc = 0;
    fApplied = 0;
    fMute = fUi; // a held UI pedal does not survive the reset (it is stale)
    return combine();
}

PedalMixer::Call PedalMixer::activate(float uiNow)
{
    fCc = 0;
    fApplied = 0;
    fUi = fMute = uiNow;
    return combine();
}

} // namespace pe
