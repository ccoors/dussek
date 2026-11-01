#include "Catalog.hpp"
#include "Paths.hpp"

#include <sstream>
#include <fstream>
#include <algorithm>
#include <filesystem>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <memory>
#include <mutex>

namespace pe {

namespace {

bool isFile(const std::string& path)
{
    std::error_code ec;
    return std::filesystem::is_regular_file(path, ec);
}

/// the subdirectories' names of `path`, sorted (none when it is no directory)
std::vector<std::string> subdirectories(const std::string& path)
{
    namespace fs = std::filesystem;
    std::vector<std::string> out;
    std::error_code ec;
    for (fs::directory_iterator it(path, ec), end; !ec && it != end; it.increment(ec)) {
        const std::string name = it->path().filename().string();
        std::error_code e2;
        if (!name.empty() && name[0] != '.' && fs::is_directory(it->path(), e2))
            out.push_back(name);
    }
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
    if (const char* env = std::getenv("DUSSEK_INSTRUMENTS"); env != nullptr && *env)
        p.push_back(env);
    if (resources != nullptr)
        p.push_back(std::string(resources) + "/instruments");
    // an installer's, before the user's: an old copy there (a test build's) does not hide them
    for (const std::string& dir : pe::systemDataDirs())
        p.push_back(dir + "/instruments");
    if (const std::string data = pe::dataDirs().data; !data.empty()) {
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
        message = std::string("Dussek: ") + err;
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
        for (const auto& [from, to] : aliasesIn(path))
            c.fAliases.emplace(from, to); // an earlier path's stays
        for (const std::string& id : subdirectories(path)) {
            InstrumentEntry e;
            std::string message;
            if (!entryAt(path, id, e, message)) {
                if (!message.empty())
                    c.fMessages.push_back(message);
                continue;
            }
            // shadowed by a real folder of this id only: an alias never hides one (Catalog::find)
            const auto seen = std::find_if(c.fEntries.begin(), c.fEntries.end(), [&](const InstrumentEntry& x) { return x.id == id; });
            if (seen != c.fEntries.end()) {
                c.fMessages.push_back("Dussek: " + e.dir + " shadowed by " + seen->dir);
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
    for (const std::string& a : aliasChain(fAliases, id))
        for (const InstrumentEntry& e : fEntries)
            if (e.id == a)
                return &e;
    return outside(id);
}

std::map<std::string, std::string> Catalog::aliasesIn(const std::string& path)
{
    std::map<std::string, std::string> out;
    std::ifstream f(path + "/aliases.txt");
    std::string line;
    while (std::getline(f, line)) {
        std::istringstream in(line);
        std::string from, to;
        if (in >> from >> to && from[0] != '#')
            out.emplace(from, to);
    }
    return out;
}

std::vector<std::string> Catalog::aliasChain(const std::map<std::string, std::string>& aliases, const std::string& id)
{
    std::vector<std::string> chain { id };
    for (int hop = 0; hop < 8; ++hop) {
        const auto it = aliases.find(chain.back());
        if (it == aliases.end() || std::find(chain.begin(), chain.end(), it->second) != chain.end())
            break;
        chain.push_back(it->second);
    }
    return chain;
}

const InstrumentEntry* Catalog::outside(const std::string& id)
{
    // an absolute path: "/…", on Windows "C:\…" or "C:/…" (a backslash is a separator there only)
#ifdef _WIN32
    const char* const seps = "/\\";
#else
    const char* const seps = "/";
#endif
    if (id.size() < 2 || !std::filesystem::path(id).is_absolute())
        return nullptr;
    static std::mutex mutex;
    static std::map<std::string, std::unique_ptr<InstrumentEntry>> kept; // never freed: pointers stay valid
    std::lock_guard<std::mutex> lock(mutex);
    std::string dir = id;
    while (dir.size() > 1 && std::strchr(seps, dir.back()) != nullptr)
        dir.pop_back();
    if (const auto it = kept.find(dir); it != kept.end())
        return it->second.get();
    const size_t slash = dir.find_last_of(seps);
    if (slash == std::string::npos)
        return nullptr;
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

std::string listingText(const Catalog& c)
{
    std::string out;
    for (const InstrumentEntry* e : c.menu())
        out += e->id + "\t" + e->group + "\t" + e->name + "\t" + e->dir + "\n";
    for (const std::string& m : c.messages())
        out += "# " + m + "\n";
    return out;
}

} // namespace pe
