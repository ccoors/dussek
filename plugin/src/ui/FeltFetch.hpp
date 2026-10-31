// The Felt tab's shipped felt, read off the UI thread (pe_felt_shipped builds the instrument or the
// morph: up to a few hundred ms): one thread for the UI's life, the newest request replacing one
// still waiting, the UI taking the result when it polls (its idle) and never waiting on it.
#pragma once
#include "dussek.h"

#include <condition_variable>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace pe {

class FeltFetch {
public:
    FeltFetch()
        : fThread([this] { run(); })
    {
    }
    ~FeltFetch()
    {
        {
            std::lock_guard<std::mutex> lock(fMutex);
            fQuit = true;
        }
        fWake.notify_one();
        fThread.join();
    }
    FeltFetch(const FeltFetch&) = delete;
    FeltFetch& operator=(const FeltFetch&) = delete;

    /// the felt of the instrument between `dirs` at `weights` (the main one first), from now on
    void request(std::vector<std::string> dirs, std::vector<float> weights)
    {
        {
            std::lock_guard<std::mutex> lock(fMutex);
            fDirs = std::move(dirs);
            fWeights = std::move(weights);
            ++fWanted;
        }
        fWake.notify_one();
    }
    /// the newest request's felt, once (false: none new); `loaded` false: the instrument did not
    /// load (`out` untouched)
    bool poll(PeFeltKeys& out, bool* loaded = nullptr)
    {
        std::lock_guard<std::mutex> lock(fMutex);
        if (!fReady)
            return false;
        fReady = false;
        if (fLoaded)
            out = fKeys;
        if (loaded != nullptr)
            *loaded = fLoaded;
        return true;
    }

private:
    void run()
    {
        std::unique_lock<std::mutex> lock(fMutex);
        uint64_t done = 0;
        while (true) {
            fWake.wait(lock, [&] { return fQuit || fWanted != done; });
            if (fQuit)
                return;
            const uint64_t job = fWanted;
            const std::vector<std::string> dirs = fDirs;
            const std::vector<float> weights = fWeights;
            lock.unlock();
            std::vector<const char*> ptrs;
            for (const std::string& d : dirs)
                ptrs.push_back(d.c_str());
            PeFeltKeys k {};
            const bool ok = !ptrs.empty() && ptrs.size() == weights.size()
                && pe_felt_shipped(ptrs.data(), weights.data(), (uint32_t)ptrs.size(), &k, nullptr, 0);
            lock.lock();
            done = job;
            if (job == fWanted) { // a newer request waits: this one is old news
                fKeys = k;
                fLoaded = ok;
                fReady = true;
            }
        }
    }

    std::mutex fMutex;
    std::condition_variable fWake;
    std::vector<std::string> fDirs;
    std::vector<float> fWeights;
    uint64_t fWanted = 0;
    PeFeltKeys fKeys {};
    bool fReady = false, fLoaded = false, fQuit = false;
    std::thread fThread; // last: started once the rest is set up
};

} // namespace pe
