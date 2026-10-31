// A named shared-memory region (the one platform-specific piece of the diagnostics view):
// the DSP creates it, the UI opens it by name. POSIX on Linux and macOS (names ≤ 31
// characters, sized once); a named file mapping on Windows ("Local\pe-…", freed by the
// system when its last handle closes).
#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <utility>

namespace pe {

class SharedRegion {
public:
    /// a new region: false-y (null) if the name exists or anything fails; the creator unlinks the name
    static std::unique_ptr<SharedRegion> create(const std::string& name, size_t size);
    /// an existing region (null if there is none or it is smaller than `size`)
    static std::unique_ptr<SharedRegion> open(const std::string& name, size_t size);
    /// "/pe-" and the token as six hex digits
    static std::string nameForToken(uint32_t token);
    /// a random 24-bit token, never 0. Does not throw: if the system's random source is
    /// unavailable it seeds from the clock, the pid and an address instead.
    static uint32_t randomToken() noexcept;
    /// Remove regions left behind by dead processes (each leaks ~280 kB of tmpfs until reboot):
    /// every "pe-" + six hex digits regular file in /dev/shm owned by this user whose header
    /// records an owner pid (`pe_telemetry_owner`) that no longer exists (`kill(pid, 0)` fails
    /// with ESRCH). Never touches a file without a valid header, with owner 0 (an instance
    /// still starting up), with a live owner (a pid we may not signal counts as alive), or with
    /// another name. Returns the number removed. Not real-time: call from a constructor. Linux
    /// only (it scans /dev/shm); elsewhere it does nothing and returns 0 (Windows frees a mapping
    /// with its last handle, so a dead process leaves none).
    ///
    /// Limits, accepted: (1) the pid is judged in OUR pid namespace. An instance in a Flatpak or
    /// container that shares /dev/shm but not the pid namespace sees the pids of other instances as
    /// dead (ESRCH) and may unlink their live regions; the blast radius is that other instance's UI,
    /// which can then not open its diagnostics (its audio and strip are unaffected). (2) Two
    /// sweepers can race: B and C both judge region X dead, B unlinks it, D creates a region with the
    /// same random name, C unlinks D's (about 1 in 16 million: a 24-bit name, and shm_unlink has no
    /// conditional form). (3) The sweep maps each region read-write (it goes through
    /// SharedRegion::open, which has no read-only mode) but writes nothing.
    static int sweepStale();

    ~SharedRegion();
    SharedRegion(const SharedRegion&) = delete;
    SharedRegion& operator=(const SharedRegion&) = delete;
    void* data() const { return fData; }
    size_t size() const { return fSize; }

private:
    SharedRegion(void* data, size_t size, std::string unlinkName, void* handle = nullptr)
        : fData(data), fSize(size), fUnlink(std::move(unlinkName)), fHandle(handle) {}
    void* fData;
    size_t fSize;
    std::string fUnlink; // set for the creator (POSIX)
    void* fHandle;       // the mapping's handle (Windows)
};

} // namespace pe
