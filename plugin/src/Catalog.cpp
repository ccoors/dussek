#include "Catalog.hpp"

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dirent.h>
#include <map>
#include <memory>
#include <mutex>
#include <sys/stat.h>

namespace pe {

namespace {

bool isFile(const std::string& path)
{
    struct stat st;
    return stat(path.c_str(), &st) == 0 && S_ISREG(st.st_mode);
}

/// the subdirectories' names of `path`, sorted (none when it is no directory)
std::vector<std::string> subdirectories(const std::string& path)
{
    std::vector<std::string> out;
    DIR* d = opendir(path.c_str());
    if (d == nullptr)
        return out;
    while (const dirent* e = readdir(d)) {
        if (e->d_name[0] == '.')
            continue;
        struct stat st;
        if (stat((path + "/" + e->d_name).c_str(), &st) == 0 && S_ISDIR(st.st_mode))
            out.push_back(e->d_name);
    }
    closedir(d);
    std::sort(out.begin(), out.end());
    return out;
}

// the groups the shipped instruments use, in menu order; others after them, "Other" last
const char* const kKnownGroups[] = { "Grand pianos", "Upright pianos", "Rolled pianos", "Harpsichords", "Clavichords", "Harps", "Guitars",
    "Percussion", "Thumb pianos", "Zithers" };

/// a group's place: the known ones first, then the unknown ones (alphabetically, by the caller's
/// tie-break), "Other" last
int groupRank(const std::string& g)
{
    for (size_t i = 0; i < sizeof(kKnownGroups) / sizeof(*kKnownGroups); ++i)
        if (g == kKnownGroups[i])
            return (int)i;
    return g == "Other" ? 1000 : 100;
}

} // namespace

const PeLabel* InstrumentEntry::label(const char* symbol) const
{
    for (uint32_t i = 0; i < info.label_count && i < PE_INFO_LABELS; ++i)
        if (std::strcmp(info.labels[i].symbol, symbol) == 0)
            return &info.labels[i];
    return nullptr;
}

std::vector<std::string> Catalog::searchPaths(const char* resources)
{
    std::vector<std::string> p;
    if (const char* env = std::getenv("PIANOENGINE_INSTRUMENTS"); env != nullptr && *env)
        p.push_back(env);
    if (resources != nullptr)
        p.push_back(std::string(resources) + "/instruments");
    std::string data;
    if (const char* xdg = std::getenv("XDG_DATA_HOME"); xdg != nullptr && *xdg)
        data = std::string(xdg) + "/pianoengine";
    else if (const char* home = std::getenv("HOME"))
        data = std::string(home) + "/.local/share/pianoengine";
    if (!data.empty()) {
        p.push_back(data + "/instruments");
        // instruments from git refs (proto/snapshot.py), for development; missing folders are skipped
        p.push_back(data + "/snapshots");
    }
    return p;
}

bool Catalog::entryAt(const std::string& path, const std::string& id, InstrumentEntry& out, std::string& message)
{
    message.clear();
    if (path.empty() || id.empty() || id == "." || id == ".." || id.find('/') != std::string::npos)
        return false;
    const std::string dir = path + "/" + id;
    if (!isFile(dir + "/instrument.json") || !isFile(dir + "/data.bin"))
        return false;
    InstrumentEntry e;
    char err[512] = {};
    if (!pe_instrument_info(dir.c_str(), &e.info, err, sizeof(err))) {
        message = std::string("PianoEngine: ") + err;
        return false;
    }
    e.id = id;
    e.dir = dir;
    e.name = e.info.has_menu ? e.info.name : id;
    e.group = e.info.has_menu ? e.info.group : "Other";
    e.hidden = e.info.has_menu && e.info.hidden;
    out = e;
    return true;
}

Catalog Catalog::scan(const std::vector<std::string>& paths)
{
    Catalog c;
    for (const std::string& path : paths) {
        if (path.empty())
            continue;
        for (const std::string& id : subdirectories(path)) {
            InstrumentEntry e;
            std::string message;
            if (!entryAt(path, id, e, message)) {
                if (!message.empty())
                    c.fMessages.push_back(message);
                continue;
            }
            if (const InstrumentEntry* seen = c.find(id)) {
                c.fMessages.push_back("PianoEngine: " + e.dir + " shadowed by " + seen->dir);
                continue;
            }
            c.fEntries.push_back(e);
        }
    }
    std::stable_sort(c.fEntries.begin(), c.fEntries.end(), [](const InstrumentEntry& a, const InstrumentEntry& b) {
        const int ra = groupRank(a.group), rb = groupRank(b.group);
        if (ra != rb)
            return ra < rb;
        if (a.group != b.group)
            return a.group < b.group;
        if (a.name != b.name)
            return a.name < b.name;
        return a.id < b.id;
    });
    for (const std::string& m : c.fMessages)
        std::fprintf(stderr, "%s\n", m.c_str());
    return c;
}

const InstrumentEntry* Catalog::find(const std::string& id) const
{
    for (const InstrumentEntry& e : fEntries)
        if (e.id == id)
            return &e;
    return outside(id);
}

const InstrumentEntry* Catalog::outside(const std::string& id)
{
    if (id.size() < 2 || id[0] != '/')
        return nullptr;
    static std::mutex mutex;
    static std::map<std::string, std::unique_ptr<InstrumentEntry>> kept; // never freed: pointers stay valid
    std::lock_guard<std::mutex> lock(mutex);
    std::string dir = id;
    while (dir.size() > 1 && dir.back() == '/')
        dir.pop_back();
    if (const auto it = kept.find(dir); it != kept.end())
        return it->second.get();
    const size_t slash = dir.find_last_of('/');
    InstrumentEntry e;
    std::string message;
    if (!entryAt(slash == 0 ? std::string("/") : dir.substr(0, slash), dir.substr(slash + 1), e, message)) {
        if (!message.empty())
            std::fprintf(stderr, "%s\n", message.c_str());
        return nullptr;
    }
    e.id = dir; // the state names it by its path
    e.group = "Opened";
    return (kept[dir] = std::make_unique<InstrumentEntry>(e)).get();
}

std::vector<const InstrumentEntry*> Catalog::menu() const
{
    std::vector<const InstrumentEntry*> m;
    for (const InstrumentEntry& e : fEntries)
        if (!e.hidden)
            m.push_back(&e);
    return m;
}

std::vector<std::string> Catalog::groups() const
{
    std::vector<std::string> g;
    for (const InstrumentEntry* e : menu())
        if (g.empty() || g.back() != e->group)
            g.push_back(e->group);
    return g;
}

const InstrumentEntry* Catalog::first() const
{
    for (const InstrumentEntry& e : fEntries)
        if (!e.hidden)
            return &e;
    return nullptr;
}

namespace {
std::mutex gCatalogMutex;
CatalogPtr gCatalog;
} // namespace

CatalogPtr sharedCatalog(const char* resources)
{
    std::lock_guard<std::mutex> lock(gCatalogMutex);
    if (!gCatalog)
        gCatalog = std::make_shared<const Catalog>(Catalog::scan(Catalog::searchPaths(resources)));
    return gCatalog;
}

CatalogPtr rescanCatalog(const char* resources)
{
    CatalogPtr fresh = std::make_shared<const Catalog>(Catalog::scan(Catalog::searchPaths(resources)));
    std::lock_guard<std::mutex> lock(gCatalogMutex);
    gCatalog = fresh;
    return fresh;
}

Choice choose(const Catalog& c, const std::string& state)
{
    if (state.empty()) {
        const InstrumentEntry* f = c.first();
        return { f != nullptr ? f->id : std::string(), f };
    }
    return { state, c.find(state) };
}

} // namespace pe
