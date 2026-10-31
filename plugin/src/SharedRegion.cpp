#include "SharedRegion.hpp"

#include "dussek.h"

#include <cerrno>
#include <chrono>
#include <csignal>
#include <cstdio>
#include <cstring>
#include <random>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>
#endif
#ifdef __linux__
#include <dirent.h>
#endif

namespace pe {

std::string SharedRegion::nameForToken(uint32_t token)
{
    char buf[16];
    std::snprintf(buf, sizeof buf, "/pe-%06x", token & 0xFFFFFF);
    return buf;
}

uint32_t SharedRegion::randomToken() noexcept
{
    static thread_local std::mt19937 rng = [] {
        try {
            return std::mt19937 { std::random_device {}() }; // can throw when the system has no random source
        } catch (...) {
            static char anchor;
            const auto now = (uint64_t)std::chrono::steady_clock::now().time_since_epoch().count();
#ifdef _WIN32
            const uint64_t pid = GetCurrentProcessId();
#else
            const uint64_t pid = (uint64_t)getpid();
#endif
            return std::mt19937 { (uint32_t)(now ^ (now >> 32) ^ (pid << 16) ^ (uint64_t)(uintptr_t)&anchor) };
        }
    }();
    return std::uniform_int_distribution<uint32_t>(1, 0xFFFFFF)(rng);
}

#ifdef _WIN32

namespace {
/// "/pe-abcdef" → "Local\pe-abcdef": the session's namespace, as POSIX names are the user's
std::string mappingName(const std::string& name)
{
    return "Local\\" + (name.empty() || name[0] != '/' ? name : name.substr(1));
}
}

std::unique_ptr<SharedRegion> SharedRegion::create(const std::string& name, size_t size)
{
    HANDLE h = CreateFileMappingA(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, (DWORD)((uint64_t)size >> 32),
                                  (DWORD)((uint64_t)size & 0xFFFFFFFFu), mappingName(name).c_str());
    if (h == nullptr)
        return nullptr;
    if (GetLastError() == ERROR_ALREADY_EXISTS) { // the name is taken: as O_EXCL
        CloseHandle(h);
        return nullptr;
    }
    void* p = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, size);
    if (p == nullptr) {
        CloseHandle(h);
        return nullptr;
    }
    return std::unique_ptr<SharedRegion>(new SharedRegion(p, size, std::string(), h));
}

std::unique_ptr<SharedRegion> SharedRegion::open(const std::string& name, size_t size)
{
    HANDLE h = OpenFileMappingA(FILE_MAP_ALL_ACCESS, FALSE, mappingName(name).c_str());
    if (h == nullptr)
        return nullptr;
    void* p = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, 0);
    MEMORY_BASIC_INFORMATION mi;
    if (p == nullptr || VirtualQuery(p, &mi, sizeof mi) == 0 || mi.RegionSize < size) {
        if (p != nullptr)
            UnmapViewOfFile(p);
        CloseHandle(h);
        return nullptr;
    }
    return std::unique_ptr<SharedRegion>(new SharedRegion(p, size, std::string(), h));
}

int SharedRegion::sweepStale()
{
    return 0; // a mapping goes with its last handle
}

SharedRegion::~SharedRegion()
{
    UnmapViewOfFile(fData);
    CloseHandle((HANDLE)fHandle);
}

#else

std::unique_ptr<SharedRegion> SharedRegion::create(const std::string& name, size_t size)
{
    const int fd = shm_open(name.c_str(), O_CREAT | O_EXCL | O_RDWR, 0600);
    if (fd < 0)
        return nullptr;
    int rc;
    do
        rc = ftruncate(fd, (off_t)size);
    while (rc != 0 && errno == EINTR);
    if (rc != 0) {
        close(fd);
        shm_unlink(name.c_str());
        return nullptr;
    }
    void* p = mmap(nullptr, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (p == MAP_FAILED) {
        shm_unlink(name.c_str());
        return nullptr;
    }
    return std::unique_ptr<SharedRegion>(new SharedRegion(p, size, name));
}

std::unique_ptr<SharedRegion> SharedRegion::open(const std::string& name, size_t size)
{
    const int fd = shm_open(name.c_str(), O_RDWR, 0600);
    if (fd < 0)
        return nullptr;
    struct stat st;
    if (fstat(fd, &st) != 0 || (size_t)st.st_size < size) {
        close(fd);
        return nullptr;
    }
    void* p = mmap(nullptr, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (p == MAP_FAILED)
        return nullptr;
    return std::unique_ptr<SharedRegion>(new SharedRegion(p, size, std::string()));
}

int SharedRegion::sweepStale()
{
#ifdef __linux__
    DIR* dir = opendir("/dev/shm");
    if (!dir)
        return 0;
    int removed = 0;
    const size_t full = pe_telemetry_region_size();
    while (const dirent* de = readdir(dir)) {
        const char* n = de->d_name;
        bool ours = std::strlen(n) == 9 && std::strncmp(n, "pe-", 3) == 0;
        for (int i = 3; ours && i < 9; ++i)
            ours = (n[i] >= '0' && n[i] <= '9') || (n[i] >= 'a' && n[i] <= 'f');
        if (!ours)
            continue;
        const std::string path = std::string("/dev/shm/") + n;
        struct stat st;
        if (lstat(path.c_str(), &st) != 0 || !S_ISREG(st.st_mode) || st.st_uid != geteuid())
            continue;
        const std::string name = std::string("/") + n;
        std::unique_ptr<SharedRegion> r = open(name, full); // null: too small, or not openable by us
        if (!r)
            continue;
        const uint32_t pid = pe_telemetry_owner(r->data(), r->size()); // 0: no valid header, or no owner yet
        if (pid == 0 || pid > 0x7FFFFFFFu)
            continue;
        if (kill((pid_t)pid, 0) == 0 || errno != ESRCH)
            continue; // alive (EPERM: someone else's live process)
        r.reset();
        if (shm_unlink(name.c_str()) == 0)
            ++removed;
    }
    closedir(dir);
    return removed;
#else
    return 0; // no portable directory of shared-memory names
#endif
}

SharedRegion::~SharedRegion()
{
    munmap(fData, fSize);
    if (!fUnlink.empty())
        shm_unlink(fUnlink.c_str());
}

#endif

} // namespace pe
