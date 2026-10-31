#include "FileWatch.hpp"

#include <algorithm>
#include <chrono>
#include <filesystem>

namespace pe {

FileStamp statStamp(const std::string& path)
{
    namespace fs = std::filesystem;
    std::error_code ec;
    const fs::file_status st = fs::status(path, ec);
    if (ec || !fs::exists(st))
        return {};
    const fs::file_time_type t = fs::last_write_time(path, ec);
    if (ec)
        return {};
    // the size of a regular file (a folder has none to compare)
    const uintmax_t size = fs::is_regular_file(st) ? fs::file_size(path, ec) : 0;
    if (ec)
        return {};
    return { (int64_t)std::chrono::duration_cast<std::chrono::nanoseconds>(t.time_since_epoch()).count(), (int64_t)size };
}

std::vector<std::string> listDirs(const std::string& path)
{
    namespace fs = std::filesystem;
    std::vector<std::string> out;
    std::error_code ec;
    for (fs::directory_iterator it(path, ec), end; !ec && it != end; it.increment(ec)) {
        // a folder counts once its instrument.json is there (the editor renames it in last: a
        // rescan before it would leave the folder out of the menu until the next change)
        std::error_code e2;
        if (fs::is_directory(it->path(), e2) && fs::exists(it->path() / "instrument.json", e2))
            out.push_back(it->path().filename().string());
    }
    std::sort(out.begin(), out.end());
    return out;
}

std::vector<FileStamp> ReloadWatch::stamps(const StampFn& stamp) const
{
    std::vector<FileStamp> s;
    for (const std::string& f : fFiles)
        s.push_back(stamp(f));
    return s;
}

void ReloadWatch::loaded(const std::vector<std::string>& dirs, const StampFn& stamp, double now)
{
    fFiles.clear();
    for (const std::string& d : dirs) {
        if (d.empty())
            continue; // an instrument not found: nothing to watch
        fFiles.push_back(d + "/instrument.json");
        fFiles.push_back(d + "/data.bin");
    }
    fLoaded = fSeen = stamps(stamp);
    fChangedAt = now;
}

bool ReloadWatch::poll(const StampFn& stamp, double now)
{
    const std::vector<FileStamp> cur = stamps(stamp);
    if (cur != fSeen) {
        fSeen = cur;
        fChangedAt = now;
        return false;
    }
    if (cur == fLoaded || now - fChangedAt < kStableS)
        return false;
    fLoaded = cur;
    return true;
}

bool ListingWatch::poll(const std::vector<std::string>& paths, const std::function<std::vector<std::string>(const std::string&)>& list)
{
    std::vector<std::string> cur;
    for (const std::string& p : paths)
        for (const std::string& n : list(p))
            cur.push_back(p + "/" + n);
    if (!fPrev) {
        fPrev = cur;
        return false;
    }
    if (cur == *fPrev)
        return false;
    fPrev = cur;
    return true;
}

} // namespace pe
