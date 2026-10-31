// The worker → audio thread handover of engines (PianoPlugin): the worker offers a new engine
// or drops the one playing (the chosen instrument does not load: silence, not the previous
// instrument); the audio thread polls at the start of a block and gives back what it played.
// Each slot has one writer and one reader: the audio thread never blocks, allocates or frees.
#pragma once
#include <algorithm>
#include <atomic>
#include <cstdint>

namespace pe {

template <class T>
class EngineHandoff {
public:
    enum class Change { None, Next, Drop };

    /// worker: the next engine; one the audio thread has not taken yet comes back (to free)
    T* offer(T* e)
    {
        T* const old = fNext.exchange(e);
        fDrop.store(false);
        return old;
    }
    /// worker: the audio thread drops its engine; one it has not taken yet comes back (to free)
    T* drop()
    {
        T* const old = fNext.exchange(nullptr);
        fDrop.store(true);
        return old;
    }
    /// worker: the engine the audio thread gave back (to free; null: none)
    T* retired() { return fRetired.exchange(nullptr); }

    /// audio thread: what changes now (nothing while the worker has not freed the last one given
    /// back): a new engine in `next`, or none (drop the one playing)
    Change poll(T*& next)
    {
        next = nullptr;
        if (fRetired.load() != nullptr)
            return Change::None;
        if ((next = fNext.exchange(nullptr)) != nullptr)
            return Change::Next;
        return fDrop.exchange(false) ? Change::Drop : Change::None;
    }
    /// audio thread: gives back the engine it played (after the new one adopted from it)
    void retire(T* old) { fRetired.store(old); }
    /// audio thread: whether the worker freed the last engine given back (another may go)
    bool idle() const { return fRetired.load() == nullptr; }

private:
    std::atomic<T*> fNext { nullptr };    // worker → audio
    std::atomic<T*> fRetired { nullptr }; // audio → worker
    std::atomic<bool> fDrop { false };    // worker → audio
};

/// The old engine's fade when the new one could not take its sounding notes over (another
/// instrument, another morph mix): its output ramps from 1 to 0 over `length` samples, across
/// blocks, instead of stopping dead (a click at every rebuild while the pad is dragged).
class FadeOut {
public:
    void start(uint32_t length)
    {
        fLength = std::max(1u, length);
        fLeft = fLength;
    }
    bool active() const { return fLeft > 0; }
    void stop() { fLeft = 0; }
    /// adds the sources × the ramp to the outputs, `frames` samples (nothing once it is over)
    void addTo(const float* srcL, const float* srcR, float* dstL, float* dstR, uint32_t frames)
    {
        const uint32_t n = std::min(frames, fLeft);
        for (uint32_t i = 0; i < n; ++i) {
            const float g = (float)(fLeft - i - 1) / (float)fLength;
            dstL[i] += g * srcL[i];
            dstR[i] += g * srcR[i];
        }
        fLeft -= n;
    }

private:
    uint32_t fLength = 1, fLeft = 0;
};

} // namespace pe
