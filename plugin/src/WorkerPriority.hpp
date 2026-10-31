// The build worker's share of the machine: below the host's other work, and a core left to the
// audio thread. A felt measured at the build fills every core for 3–7 s; the engine's build threads,
// spawned from the worker, inherit its niceness (Linux: per thread). The audio thread is the host's
// and keeps its own priority, but shares the caches, the cores' siblings and the clock: a build on
// every logical CPU slowed its blocks 2–3× at their tail (pe_set_build_threads).
#pragma once

#include <algorithm>
#include <fstream>
#include <set>
#include <string>
#include <thread>
#include <utility>

#ifdef __linux__
#include <sys/resource.h>
#include <sys/syscall.h>
#include <unistd.h>
#endif

namespace pe {

/// the physical cores (Linux: distinct core and package ids; elsewhere the logical CPUs)
inline unsigned physicalCores()
{
    const unsigned logical = std::max(1u, std::thread::hardware_concurrency());
#ifdef __linux__
    std::set<std::pair<int, int>> cores;
    for (unsigned i = 0; i < logical; ++i) {
        const std::string dir = "/sys/devices/system/cpu/cpu" + std::to_string(i) + "/topology/";
        std::ifstream core(dir + "core_id"), package(dir + "physical_package_id");
        int c = -1, p = -1;
        if (!(core >> c) || !(package >> p))
            return logical;
        cores.insert({ p, c });
    }
    return std::max(1u, std::min(logical, (unsigned)cores.size()));
#else
    return logical;
#endif
}

/// a build's threads on `cores` physical cores: all but one (the audio thread's), at least one
inline unsigned buildThreadsFor(unsigned cores) { return cores > 1 ? cores - 1 : 1; }

/// the worker's niceness (at least; a process run niced keeps its own)
constexpr int kWorkerNice = 10;

/// the calling thread's niceness (0 where unknown)
inline int workerNice()
{
#ifdef __linux__
    return getpriority(PRIO_PROCESS, (id_t)syscall(SYS_gettid));
#else
    return 0;
#endif
}

/// the calling thread (the build worker) to kWorkerNice, unless it is nicer already
inline void lowerWorkerPriority()
{
#ifdef __linux__
    if (workerNice() < kWorkerNice)
        setpriority(PRIO_PROCESS, (id_t)syscall(SYS_gettid), kWorkerNice);
#endif
}

} // namespace pe
