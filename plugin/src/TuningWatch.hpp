// Whether the parameters' tuning (EngineValues.hpp engineTuning: what an engine is built for) has
// changed (tested in ui_tests). Any thread that stores a parameter marks the watch; only the worker
// compares, so the host's thread and the audio thread (MIDI learn) never race on the last tuning.
#pragma once
#include "EngineValues.hpp"
#include "Params.hpp"

#include <atomic>

namespace pe {

class TuningWatch {
public:
    /// the tuning the first engine is built for (before the worker starts)
    void reset(const PeTuning& t) { fLast = t; }
    /// any thread, after storing the parameter in the shared values
    void mark() { fMarked.store(true, std::memory_order_release); }
    /// worker: when marked, the shared values' tuning; true (into `out`) when it differs from the
    /// last one taken
    bool take(const std::atomic<float>* shared, PeTuning& out)
    {
        if (!fMarked.exchange(false, std::memory_order_acq_rel))
            return false;
        float v[kParamCount];
        for (uint32_t i = 0; i < kParamCount; ++i)
            v[i] = shared[i].load(std::memory_order_relaxed);
        const PeTuning t = engineTuning(v);
        if (sameTuning(t, fLast))
            return false;
        fLast = out = t;
        return true;
    }
    /// worker: the last tuning taken
    const PeTuning& last() const { return fLast; }

private:
    std::atomic<bool> fMarked { false };
    PeTuning fLast {};
};

} // namespace pe
