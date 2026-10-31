// The host's thread → audio thread handover of the velocity curve (the "velocity_curve" state):
// the host publishes, the audio thread takes it for its engine once per change and again for
// every new engine (an instrument switch or a session restore builds one without pe_adopt).
// One writer, one reader; a sequence lock over atomic words: the audio thread never waits or
// allocates (a copy that a publish overlapped counts as nothing; the next block takes it).
#pragma once
#include "pianoengine.h"

#include <atomic>
#include <cstdint>
#include <cstring>

namespace pe {

class CurveSlot {
public:
    CurveSlot() { write(straight()); }

    /// host's thread: the curve from now on
    void publish(const PeVelocityCurve& c)
    {
        beginPublish();
        endPublish(c);
    }
    /// the two halves of a publish (apart: for the test of a publish the host's thread is
    /// preempted in): the sequence odd, its words not yet visible before it (the fence), …
    void beginPublish()
    {
        fSeq.fetch_add(1, std::memory_order_relaxed);
        std::atomic_thread_fence(std::memory_order_release);
    }
    /// … the words, then the sequence even again
    void endPublish(const PeVelocityCurve& c)
    {
        write(c);
        fSeq.fetch_add(1, std::memory_order_release);
    }
    /// audio thread: the engine of the last take is gone (dropped, or about to be freed): the
    /// next take hands the curve to whatever engine comes, even one at the same address
    void forget() { fTaken = false; }

    /// audio thread: true (and the curve in `out`) when it changed since the last take, or
    /// `engine` is not the engine of the last take
    bool take(PeEngine* engine, PeVelocityCurve& out)
    {
        const uint32_t s = fSeq.load(std::memory_order_acquire);
        if (fTaken && s == fTakenSeq && engine == fTakenEngine)
            return false;
        if (s & 1u)
            return false; // a publish under way: the next block
        uint32_t words[kWords];
        for (int i = 0; i < kWords; ++i)
            words[i] = fWords[i].load(std::memory_order_relaxed);
        std::atomic_thread_fence(std::memory_order_acquire);
        if (fSeq.load(std::memory_order_relaxed) != s)
            return false; // overlapped by a publish: the next block
        std::memcpy(&out, words, sizeof out);
        fTaken = true;
        fTakenSeq = s;
        fTakenEngine = engine;
        return true;
    }

    static PeVelocityCurve straight()
    {
        PeVelocityCurve c {};
        c.n = 2;
        c.vin[0] = c.vout[0] = 1;
        c.vin[1] = c.vout[1] = 127;
        return c;
    }

private:
    static constexpr int kWords = (sizeof(PeVelocityCurve) + 3) / 4;
    static_assert(sizeof(PeVelocityCurve) == 20, "PeVelocityCurve layout (pianoengine.h)");

    void write(const PeVelocityCurve& c)
    {
        uint32_t words[kWords] = {};
        std::memcpy(words, &c, sizeof c);
        for (int i = 0; i < kWords; ++i)
            fWords[i].store(words[i], std::memory_order_relaxed);
    }

    std::atomic<uint32_t> fSeq { 0 };
    std::atomic<uint32_t> fWords[kWords] {};
    // the audio thread's own
    bool fTaken = false;
    uint32_t fTakenSeq = 0;
    PeEngine* fTakenEngine = nullptr;
};

} // namespace pe
