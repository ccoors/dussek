// The instruments found in the search paths: what the menu lists and the "instrument" state
// names (by directory name, never by position, so a rescan changes nothing else).
#pragma once
#include "dussek.h"

#include <memory>
#include <map>
#include <string>
#include <vector>

namespace pe {

/// One instrument directory found in the search paths.
struct InstrumentEntry {
    std::string id;    // the directory's name (the "instrument" state)
    std::string dir;   // its full path
    std::string name;  // the menu's name (the id without a menu block)
    std::string group; // the menu's group ("Other" without a menu block)
    bool hidden = false;
    PeInstrumentInfo info {};
    /// the menu block's label for a parameter symbol (nullptr: none)
    const PeLabel* label(const char* symbol) const;
};

/// The instruments found, in menu order (hidden ones kept, for a session that names one).
class Catalog {
public:
    /// the search paths, in precedence order: $DUSSEK_INSTRUMENTS, <resources>/instruments, the
    /// standalone's own folder's instruments (pe::portableFolder: an unpacked archive), the
    /// system folders' instruments (pe::systemDataDirs: an installer's), the user data folder's
    /// instruments (pe::dataDirs: ~/.local/share/dussek/instruments on Linux), then its snapshots
    /// folder (proto/snapshot.py); unset ones left out
    static std::vector<std::string> searchPaths(const char* resources);
    /// every subdirectory with an instrument.json (that the engine's report accepts) and a data.bin
    /// (present); an id seen in an earlier path shadows later ones; rejected files are left out
    /// (one stderr line each)
    static Catalog scan(const std::vector<std::string>& paths);
    /// directory `id` under `path` as an entry (the scan's test: a plain name, both files, the
    /// report); false with a message when the files are there but the report rejects them, false
    /// with none when it is no instrument directory
    static bool entryAt(const std::string& path, const std::string& id, InstrumentEntry& out, std::string& message);
    /// an instrument directory kept anywhere (`id` its absolute path: the "instrument" state of one
    /// opened from a folder) as an entry, made on first use and kept for the process (the same
    /// pointer every time); nullptr when it is none
    static const InstrumentEntry* outside(const std::string& id);
    /// the aliases an instrument folder declares (its aliases.txt: "old-id new-id" per line, `#`
    /// comments): a renamed or removed instrument's saved sessions find its successor
    static std::map<std::string, std::string> aliasesIn(const std::string& path);
    /// `id`, then the ids its aliases lead to (up to 8 hops; a cycle ends it), in order
    static std::vector<std::string> aliasChain(const std::map<std::string, std::string>& aliases, const std::string& id);

    /// the entry of a listed id, else of the first listed id its aliases lead to (a real folder wins
    /// over an alias), else of an absolute path (`outside`)
    const InstrumentEntry* find(const std::string& id) const;
    /// the listed entries (not hidden), in menu order
    std::vector<const InstrumentEntry*> menu() const;
    /// the groups of the listed entries, in menu order
    std::vector<std::string> groups() const;
    /// the default choice: the first listed entry (nullptr: none found)
    const InstrumentEntry* first() const;
    /// one line per problem the scan found (shadowed, rejected), as printed
    const std::vector<std::string>& messages() const { return fMessages; }

private:
    std::vector<InstrumentEntry> fEntries; // menu order
    std::map<std::string, std::string> fAliases; // old id → new (the first path's wins)
    std::vector<std::string> fMessages;
};

using CatalogPtr = std::shared_ptr<const Catalog>;
/// the process-wide catalog, scanned on the first call (thread-safe)
CatalogPtr sharedCatalog(const char* resources);
/// scans anew and replaces the shared catalog (holders of the old one keep it); unused so far
CatalogPtr rescanCatalog(const char* resources);

/// The chosen instrument: the state's id and its entry (nullptr: missing).
struct Choice {
    std::string id;
    const InstrumentEntry* entry = nullptr;
};
/// the state's value as a choice: empty → the catalog's first; an id not found stays, entry nullptr
Choice choose(const Catalog& c, const std::string& state);

/// `dussek list`: a line per instrument in the menu's order, "<id>\t<group>\t<name>\t<folder>\n",
/// then the scan's messages as "# <message>\n"; empty without instruments and messages
std::string listingText(const Catalog& c);

} // namespace pe
