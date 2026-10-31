// Instruments that change on disk (design log steps 117–118): the
// editor writes a rolled piano's preview where the plugin looks, and the plugin plays the new one.
// ReloadWatch: the loaded instruments' files changed and then stable for 0.5 s (a writer between
// its two renames is waited out); ListingWatch: a search path's folders changed (rescan the menu).
// Pure logic over a stamp function (tested in ui_tests); the worker polls them twice a second.
#pragma once
#include <cstdint>
#include <functional>
#include <optional>
#include <string>
#include <vector>

namespace pe {

struct FileStamp {
    int64_t mtimeNs = 0;
    int64_t size = -1; // −1: missing
    bool operator==(const FileStamp& o) const { return mtimeNs == o.mtimeNs && size == o.size; }
    bool operator!=(const FileStamp& o) const { return !(*this == o); }
};

using StampFn = std::function<FileStamp(const std::string& path)>;

/// stat(): the file's modification time and size (missing: size −1)
FileStamp statStamp(const std::string& path);
/// the names of the folders (and links to folders) in `path` that hold an instrument.json
std::vector<std::string> listDirs(const std::string& path);

class ReloadWatch {
public:
    static constexpr double kStableS = 0.5;
    /// the instruments in `dirs` as loaded now (call before reading them: a change during the read
    /// is then seen)
    void loaded(const std::vector<std::string>& dirs, const StampFn& stamp, double now);
    /// true once when the files differ from those loaded and have not changed for kStableS
    bool poll(const StampFn& stamp, double now);

private:
    std::vector<FileStamp> stamps(const StampFn& stamp) const;
    std::vector<std::string> fFiles;
    std::vector<FileStamp> fLoaded, fSeen;
    double fChangedAt = 0.0;
};

class ListingWatch {
public:
    /// true when the folders under any of `paths` differ from the previous poll's (never on the first)
    bool poll(const std::vector<std::string>& paths, const std::function<std::vector<std::string>(const std::string&)>& list);

private:
    std::optional<std::vector<std::string>> fPrev;
};

} // namespace pe
