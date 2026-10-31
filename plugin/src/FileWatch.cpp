#include "FileWatch.hpp"

#include <algorithm>
#include <dirent.h>
#include <sys/stat.h>

namespace pe {

FileStamp statStamp(const std::string& path)
{
    struct stat st {};
    if (stat(path.c_str(), &st) != 0)
        return {};
    return { (int64_t)st.st_mtim.tv_sec * 1000000000 + st.st_mtim.tv_nsec, (int64_t)st.st_size };
}

std::vector<std::string> listDirs(const std::string& path)
{
    std::vector<std::string> out;
    if (DIR* d = opendir(path.c_str())) {
        while (const dirent* e = readdir(d)) {
            const std::string name = e->d_name;
            struct stat st {};
            // a folder counts once its instrument.json is there (the editor renames it in last: a
            // rescan before it would leave the folder out of the menu until the next change)
            if (name != "." && name != ".." && stat((path + "/" + name).c_str(), &st) == 0 && S_ISDIR(st.st_mode)
                && stat((path + "/" + name + "/instrument.json").c_str(), &st) == 0)
                out.push_back(name);
        }
        closedir(d);
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
