#include "check.hpp"
#include "About.hpp"
#include "Catalog.hpp"
#include "CurveSlot.hpp"
#include "CurveEdit.hpp"
#include "FeltEditor.hpp"
#include "FeltFetch.hpp"
#include "NoteHistory.hpp"
#include "EngineHandoff.hpp"
#include "EngineValues.hpp"
#include "FeltWatch.hpp"
#include "FileWatch.hpp"
#include <cstddef>
#include <map>
#include "Params.hpp"
#include "Paths.hpp"
#include "Numbers.hpp"
#include "PedalMixer.hpp"
#include "Presets.hpp"
#include "Format.hpp"
#include "Instruments.hpp"
#include "Layout.hpp"
#include "MidiMap.hpp"
#include "MidiRoute.hpp"
#include "Morph.hpp"
#include "TuningWatch.hpp"
#include "MicMap.hpp"
#include "SharedRegion.hpp"
#include "Spectro.hpp"
#include "Strip.hpp"
#include "StripInteraction.hpp"
#include "TelemetryClient.hpp"
#include "Viz.hpp"
#include "Tooltip.hpp"
#include "UiState.hpp"
#include "WorkerPriority.hpp"
#include "dussek.h"
#include "DistrhoPluginInfo.h"
#include "StateVersion.hpp"
#include "GlCheck.hpp"

#include <algorithm>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <thread>
#include <cmath>
#include <cstdio>
#include <clocale>
#include <cstdlib>
#include <cstring>
#include <set>
#include <string>
#include <utility>
#include <vector>

#include <cerrno>
#include <csignal>
#ifndef _WIN32
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>
#endif

using namespace pe;

/// a new empty folder in the system's temporary folder, `prefix` and a random token its name
static std::string tempDir(const char* prefix)
{
    for (int tries = 0; tries < 100; ++tries) {
        const std::filesystem::path p = std::filesystem::temp_directory_path()
            / (std::string(prefix) + std::to_string(SharedRegion::randomToken()));
        std::error_code ec;
        if (std::filesystem::create_directory(p, ec))
            return p.generic_string();
    }
    return std::string();
}

/// setenv and unsetenv (POSIX; Windows' _putenv_s, where an empty value removes the variable)
static void setEnv(const char* name, const char* value)
{
#ifdef _WIN32
    _putenv_s(name, value);
#else
    setenv(name, value, 1);
#endif
}
static void unsetEnv(const char* name)
{
#ifdef _WIN32
    _putenv_s(name, "");
#else
    unsetenv(name);
#endif
}

/// whether a folder made read-only refuses new files to this process (not to root; not on Windows)
static bool readOnlyFoldersRefuse()
{
#ifdef _WIN32
    return false;
#else
    return geteuid() != 0;
#endif
}

// The committed instruments' catalog entries, by their old menu index: the tests written before
// the catalog keep their vocabulary (profileByDir, profileBars, paramActive(param, profile, …)),
// every function below reads the entry of instruments/<dir> (its report and its menu block).
static const char* const kProfiles[] = { "grand_a", "grand_b", "grand_f", "grand_e", "grand_d", "grand_c", "grand_g", "grand_h",
    "upright_a", "upright_b", "upright_c", "harpsichord", "flemish", "french", "italian", "harpsichord_e", "harpsichord_f", "clavichord", "harp", "leverharp",
    "vibraphone", "marimba", "xylophone", "bells", "crotales", "chimes", "handchimes", "kalimba", "mbira", "mavembe", "nyunga", "balafon", "guitar",
    "strumstick", "psaltery", "dantranh" };
constexpr uint32_t kProfileCount = sizeof(kProfiles) / sizeof(*kProfiles);

/// the folders of instruments/ whose instrument.json git tracks (the editor's preview and a rolled
/// piano the user made and has not committed are not the tests' instruments)
static std::set<std::string> trackedInstruments()
{
    std::set<std::string> out;
    // double quotes: the shell's and cmd.exe's (_popen on Windows)
    const std::string cmd = std::string("git -C \"") + PE_INSTRUMENTS_DIR + "\" ls-files -- \"*/instrument.json\"";
#ifdef _WIN32
    if (FILE* f = _popen(cmd.c_str(), "r")) {
#else
    if (FILE* f = popen(cmd.c_str(), "r")) {
#endif
        char line[1024];
        while (std::fgets(line, sizeof(line), f) != nullptr) {
            std::string l(line);
            if (const size_t slash = l.find('/'); slash != std::string::npos)
                out.insert(l.substr(0, slash));
        }
#ifdef _WIN32
        _pclose(f);
#else
        pclose(f);
#endif
    }
    return out;
}

/// the committed instruments: those of instruments/ that git tracks (scanned through a folder of
/// links to them, or copies where links cannot be made: Windows without its developer mode;
/// removed at exit)
static const Catalog& committed()
{
    static const Catalog c = [] {
        static const std::string links = tempDir("pe-committed-");
        const std::set<std::string> tracked = trackedInstruments();
        for (const std::string& name : pe::listDirs(PE_INSTRUMENTS_DIR))
            if (tracked.count(name) != 0) {
                const std::filesystem::path from = std::filesystem::path(PE_INSTRUMENTS_DIR) / name, to = std::filesystem::path(links) / name;
                std::error_code ec;
                std::filesystem::create_directory_symlink(from, to, ec);
                if (ec)
                    std::filesystem::copy(from, to, std::filesystem::copy_options::recursive, ec);
            }
        Catalog scanned = Catalog::scan({ links });
        std::atexit([] {
            std::error_code ec;
            std::filesystem::remove_all(links, ec); // the links, not what they point at
        });
        return scanned;
    }();
    return c;
}
/// the entry of an old index (past the list: noInstrument)
static const InstrumentEntry& entryOf(uint32_t p)
{
    if (p >= kProfileCount)
        return noInstrument();
    const InstrumentEntry* e = committed().find(kProfiles[p]);
    if (e == nullptr) {
        std::fprintf(stderr, "no instrument %s under %s\n", kProfiles[p], PE_INSTRUMENTS_DIR);
        std::abort();
    }
    return *e;
}
static uint32_t profileByDir(const char* dir)
{
    for (uint32_t i = 0; i < kProfileCount; ++i)
        if (std::strcmp(kProfiles[i], dir) == 0)
            return i;
    return kProfileCount;
}
static UiContext ctxOf(uint32_t p, uint32_t mic, uint32_t prep) { return { &entryOf(p), mic, prep }; }
static bool paramActive(uint32_t param, uint32_t p, uint32_t mic, uint32_t prep) { return pe::paramActive(param, entryOf(p), mic, prep); }
static const char* displayName(uint32_t param, uint32_t p) { return pe::displayName(param, entryOf(p)); }
static const char* displayDescription(uint32_t param, uint32_t p) { return pe::displayDescription(param, entryOf(p)); }
static std::string tooltipTitle(uint32_t param, uint32_t p) { return pe::tooltipTitle(param, entryOf(p)); }
static uint32_t effectivePrep(uint32_t p, uint32_t v) { return pe::effectivePrep(entryOf(p), v); }
static uint32_t profilePreparations(uint32_t p, const uint32_t** values)
{
    static uint32_t v[5];
    *values = v;
    return entryPreparations(entryOf(p), v);
}
static void profileRecorded(uint32_t p, int& lo, int& hi) { entryRecorded(entryOf(p), lo, hi); }
static BodyKind profileBody(uint32_t p) { return entryBody(entryOf(p)); }
static uint32_t exciter(uint32_t p) { return entryOf(p).info.exciter; }
static bool profileHasRegisters(uint32_t p) { return entryOf(p).info.registers > 1; }
/// plucked by quills, fingers or picks (the harpsichords, the harps, the guitars, the zithers)
static bool profilePlucked(uint32_t p) { return exciter(p) == PE_EXCITER_QUILL || exciter(p) == PE_EXCITER_FINGER || exciter(p) == PE_EXCITER_PICK; }
static bool profileFingered(uint32_t p) { return exciter(p) == PE_EXCITER_FINGER || exciter(p) == PE_EXCITER_PICK; }
static bool profileHarp(uint32_t p) { return profileBody(p) == BodyKind::Harp || profileBody(p) == BodyKind::LeverHarp; }
static bool profileGuitar(uint32_t p) { return profileBody(p) == BodyKind::Guitar; }
static bool profileStrumstick(uint32_t p) { return profileBody(p) == BodyKind::Strumstick; }
static bool profileZither(uint32_t p) { return exciter(p) == PE_EXCITER_PICK; }
static bool profileBends(uint32_t p) { return entryOf(p).info.bend; }
static bool profileHasBuff(uint32_t p) { return entryOf(p).info.buff; }
static bool profileHasMotor(uint32_t p) { return entryOf(p).info.motor; }
static bool profileHasMallets(uint32_t p) { return entryMallets(entryOf(p)) > 0; }
static bool profileClavichord(uint32_t p) { return entryOf(p).info.bebung; }
static bool profileBars(uint32_t p) { return entryOf(p).info.bars; }
static bool profileVibraphone(uint32_t p) { return profileBody(p) == BodyKind::Vibraphone; }
static bool profileMarimba(uint32_t p) { return profileBody(p) == BodyKind::Marimba; }
static bool profileXylophone(uint32_t p) { return profileBody(p) == BodyKind::Xylophone; }
static bool profileBells(uint32_t p) { return profileBody(p) == BodyKind::Bells; }
static bool profileCrotales(uint32_t p) { return profileBody(p) == BodyKind::Crotales; }
static bool profileChimes(uint32_t p) { return profileBody(p) == BodyKind::Chimes; }
static bool profileHandChimes(uint32_t p) { return profileBody(p) == BodyKind::HandChimes; }
static bool profileKalimba(uint32_t p) { return profileBody(p) == BodyKind::Kalimba; }
static bool profileBalafon(uint32_t p) { return profileBody(p) == BodyKind::Balafon; }
/// the menu's group of an old index: its place in the catalog's groups (0 past the list)
static uint32_t groupOf(uint32_t p)
{
    const std::vector<std::string> g = committed().groups();
    for (uint32_t i = 0; i < g.size(); ++i)
        if (p < kProfileCount && g[i] == entryOf(p).group)
            return i;
    return 0;
}

// profiles by directory, so tests survive menu reordering
static const uint32_t kUprA = profileByDir("upright_a"), kUprB = profileByDir("upright_b"), kUprC = profileByDir("upright_c"),
    kHarpA = profileByDir("harpsichord"), kHarpB = profileByDir("flemish"), kHarpC = profileByDir("french"),
    kHarpD = profileByDir("italian"), kHarpE = profileByDir("harpsichord_e"), kHarpF = profileByDir("harpsichord_f"),
    kHarp = profileByDir("harp"), kHarpB2 = profileByDir("leverharp"),
    kVib = profileByDir("vibraphone"), kMar = profileByDir("marimba"), kXyl = profileByDir("xylophone"),
    kBel = profileByDir("bells"), kCro = profileByDir("crotales"), kGui = profileByDir("guitar"),
    kCla = profileByDir("clavichord"), kChi = profileByDir("chimes"), kHch = profileByDir("handchimes"), kKal = profileByDir("kalimba"), kBal = profileByDir("balafon"),
    kMbA = profileByDir("mbira"), kMbB = profileByDir("mavembe"), kNyu = profileByDir("nyunga"),
    kStr = profileByDir("strumstick"), kPsa = profileByDir("psaltery"), kDan = profileByDir("dantranh");

TEST(phantom_partials_are_a_defect)
{
    // the Condition once drove them: loud treble notes 20–30 dB over their neighbors (design log step 190)
    CHECK(paramInfo(kPhantom).group == kGroupDefects && paramInfo(kPhantom).def == 0.0f && !wearAspect(kPhantom));
    std::vector<float> vals(kParamCount);
    for (uint32_t i = 0; i < kParamCount; ++i)
        vals[i] = paramInfo(i).def;
    vals[kCondition] = 100;
    CHECK(engineParams(vals.data()).phantom == 0.0f && engineParams(vals.data()).hammer_wear == 1.0f);
    vals[kPhantom] = 40;
    CHECK(std::fabs(engineParams(vals.data()).phantom - 0.4f) < 1e-6f);
    bool listed = false;
    for (const TabGroup& g : tabGroups(kTabExtras))
        for (uint32_t p : g.params)
            listed = listed || (p == kPhantom && g.title && std::string(g.title) == "Defects");
    CHECK(listed);
}

TEST(lid_and_pillow_parameters)
{
    CHECK(paramInfo(kLid).group == kGroupMics && paramInfo(kPillow).group == kGroupMics);
    CHECK(paramInfo(kLid).def == 100.0f && paramInfo(kPillow).def == 0.0f);
    CHECK(!paramInfo(kLid).automatable && !paramInfo(kPillow).automatable);
    // the lid has no geometry to move at the Recording position; the pillow works everywhere
    CHECK(!paramActive(kLid, 0, 0, 0) && paramActive(kLid, 0, 1, 0) && paramActive(kLid, kUprA, 4, 0));
    CHECK(paramActive(kLid, 0, 2, 0));   // Audience
    CHECK(paramActive(kLid, kHarpA, 1, 0));   // a harpsichord at Player
    CHECK(paramActive(kPillow, 0, 0, 0) && paramActive(kPillow, kHarpA, 3, 0));
    CHECK(formatValue(kLid, 0.0f) == "Closed" && formatValue(kLid, 100.0f) == "Open" && formatValue(kLid, 50.0f) == "50 %");
    CHECK(formatValue(kPillow, 0.0f) == "Off" && formatValue(kPillow, 40.0f) == "40 %");
    float v = 0;
    CHECK(parseValue(kLid, "closed", v) && v == 0.0f && parseValue(kLid, "open", v) && v == 100.0f && parseValue(kPillow, "off", v) && v == 0.0f);
    CHECK(parseValue(kLid, "50", v) && v == 50.0f);
    // taking the lid off: a switch beside the Lid, shown where the Lid is, the engine's lid 2 when on
    CHECK(paramInfo(kLidOff).group == kGroupMics && paramInfo(kLidOff).boolean && paramInfo(kLidOff).def == 0.0f);
    CHECK(!paramInfo(kLidOff).automatable);
    for (uint32_t p : { 0u, kUprA, kHarpA, kHarp, kMar })
        for (uint32_t pos = 0; pos <= 5; ++pos)
            CHECK(paramActive(kLidOff, p, pos, 0) == paramActive(kLid, p, pos, 0));
    std::vector<float> vals(kParamCount);
    for (uint32_t i = 0; i < kParamCount; ++i)
        vals[i] = paramInfo(i).def;
    vals[kMicPosition] = 1;
    vals[kLid] = 50;
    CHECK(engineLid(vals.data()) == 0.5 && engineTuning(vals.data()).lid == 0.5);
    vals[kLidOff] = 1;
    CHECK(engineLid(vals.data()) == 2.0 && engineTuning(vals.data()).lid == 2.0);
    vals[kMicPosition] = 0; // the recording's lid is open
    CHECK(engineTuning(vals.data()).lid == 1.0);
}

namespace {
/// one row of plugin/tests/params.lock: a user parameter as the hosts saw it at a release
struct LockRow {
    uint32_t index = 0;
    std::string symbol, unit, flags, line;
    float min = 0, max = 0, def = 0;
};

std::string lockFlags(const pe::ParamInfo& d)
{
    std::string f = std::string(d.automatable ? "a" : "") + (d.integer ? "i" : "") + (d.boolean ? "b" : "");
    return f.empty() ? "-" : f;
}

std::string lockLine(uint32_t i)
{
    const pe::ParamInfo& d = pe::paramInfo(i);
    char nums[96];
    std::snprintf(nums, sizeof(nums), "%.9g %.9g %.9g", (double)d.min, (double)d.max, (double)d.def);
    return std::to_string(i) + " " + d.symbol + " " + nums + " " + (d.unit != nullptr && *d.unit ? d.unit : "-") + " "
        + lockFlags(d);
}

/// the lock's rows (`#` lines and blank lines skipped); false when the file is missing or a row is malformed
bool readLock(const std::string& path, std::vector<LockRow>& out)
{
    std::ifstream f(path);
    if (!f)
        return false;
    std::string line;
    while (std::getline(f, line)) {
        if (line.empty() || line[0] == '#')
            continue;
        std::istringstream in(line);
        in.imbue(std::locale::classic());
        LockRow r;
        r.line = line;
        if (!(in >> r.index >> r.symbol >> r.min >> r.max >> r.def >> r.unit >> r.flags))
            return false;
        out.push_back(r);
    }
    return true;
}
} // namespace

TEST(the_state_carries_its_version)
{
    CHECK(pe::kStateVersion == 1 && pe::kFirstStateVersion == 1);
    // DPF's VST3 and CLAP wrappers keep the state in a std::map: they write and replay it in key
    // order, so the version's key sorts before every other (LV2 restores in declared order: first)
    CHECK(std::string(pe::kStateVersionKey) == "_state_version");
    for (const char* k : pe::stateKeys())
        CHECK(std::strcmp(pe::kStateVersionKey, k) <= 0);
    // a value without a version is the first (no state before it was released), not today's
    CHECK(pe::parseStateVersion(nullptr) == pe::kFirstStateVersion && pe::parseStateVersion("") == pe::kFirstStateVersion
          && pe::parseStateVersion("x") == pe::kFirstStateVersion);
    CHECK(pe::parseStateVersion("1") == 1 && pe::parseStateVersion("7") == 7); // newer: read as far as understood
    CHECK(pe::migrateState(1, "instrument", "grand_a") == "grand_a");
    for (const char* k : { "_state_version", "ui", "instrument", "velocity_curve", "morph", "felt", "scale_file" })
        CHECK(pe::stateKnown(k));
    CHECK(!pe::stateKnown("nonsense"));
    // a migration leaves a value already in today's form as it is: a version left over from an
    // older restore never harms a live change
    for (uint32_t from = 0; from <= pe::kStateVersion; ++from)
        for (const char* k : { "ui", "instrument", "velocity_curve", "morph", "felt", "scale_file" })
            CHECK(pe::migrateState(from, k, "grand_a") == pe::migrateState(pe::kStateVersion, k, "grand_a"));
    // the keys in either order give the same values
    const std::vector<std::pair<std::string, std::string>> keys
        = { { "felt", "s:21=6" }, { "instrument", "grand_b" }, { "_state_version", "1" } };
    std::vector<std::string> forward, backward;
    pe::StateRestore a, b;
    for (const auto& [k, v] : keys)
        forward.push_back(a.apply(k, v));
    for (auto it = keys.rbegin(); it != keys.rend(); ++it)
        backward.insert(backward.begin(), b.apply(it->first, it->second));
    CHECK(forward == backward && forward[0] == "s:21=6" && forward[1] == "grand_b");
    CHECK(a.version() == 1 && b.version() == 1);
    pe::StateRestore c;
    c.apply("_state_version", "7");
    CHECK(c.version() == 7 && c.apply("instrument", "grand_a") == "grand_a");
}

TEST(user_files_carry_a_version)
{
    // presets and MIDI maps are the player's own files: each says which version wrote it; an older
    // or a newer one loads what it understands (docs/compatibility.md)
    pe::Preset p;
    p.name = "Mine";
    p.scope = pe::PresetScope::All;
    p.values = { { "volume", 3.0f } };
    const std::string text = pe::formatPreset(p);
    CHECK(text.find("\nversion = 1\n") != std::string::npos);
    pe::Preset q;
    CHECK(pe::parsePreset("name = Old\nscope = all\nparam.volume = 2\n", q) && q.version == pe::kFirstStateVersion);
    CHECK(pe::Preset {}.version == pe::kFirstStateVersion && MidiMap {}.version == pe::kFirstMidiMapVersion);
    CHECK(pe::parsePreset("# a later release\nversion = 2\nname = New\nscope = all\nfuture = x\nparam.volume = 2\n"
                          "param.not_a_parameter = 5\nstate.instrument = grand_b\n",
              q));
    CHECK(q.version == 2 && q.name == "New" && q.scope == pe::PresetScope::All);
    CHECK(q.values.size() == 2 && q.values[0].first == "volume" && q.states.size() == 1);
    MidiMap m;
    m.tie(kMicLX, 1, 22);
    CHECK(m.text().find("\nversion 1\n") != std::string::npos && MidiMap::parse(m.text()).version == 1);
    const MidiMap later = MidiMap::parse("version 2\nknob 1 2 3\nprogram_change on\ncc 2 22 mic_l_x\n");
    CHECK(later.version == 2 && later.programChange && later.find(kMicLX) != nullptr && later.find(kMicLX)->cc == 22);
    CHECK(MidiMap::parse("cc 2 22 mic_l_x\n").version == pe::kFirstMidiMapVersion);
}

TEST(the_scale_is_kept_for_the_next_window)
{
    // the standalone has no session: the scale last picked is kept in the config folder
    // (window.txt) and is every new window's (a session's own "ui" state wins in a host)
    const std::string dir = tempDir("pe-window-") + "/config/dussek"; // made by the first save
    CHECK(pe::loadSavedZoom(dir) == 100); // nothing saved yet
    CHECK(pe::saveZoom(dir, 150) && pe::loadSavedZoom(dir) == 150);
    CHECK(pe::initialUiState(dir).zoom == 150 && pe::initialUiState(dir).tab == 0);
    CHECK(pe::parseSavedZoom("zoom=175\n") == 175 && pe::parseSavedZoom("zoom=137\n") == 100
          && pe::parseSavedZoom("") == 100 && pe::parseSavedZoom("zoom=x") == 100);
    CHECK(!pe::saveZoom("", 150) && pe::loadSavedZoom("") == 100); // no config folder known
}

TEST(a_display_without_opengl_2_is_named)
{
    // over Remote Desktop, or in a VM without 3D acceleration, Windows offers OpenGL 1.1 only: the
    // window stays black; the player is told why and that the sound still plays
    const std::string m = pe::openGlMissingText();
    CHECK(m.find("OpenGL 2") != std::string::npos && m.find("Remote Desktop") != std::string::npos
          && m.find("sound") != std::string::npos);
}

TEST(a_picked_file_gives_its_folder)
{
    // the instrument picker hands a file (its instrument.json): the folder is what is opened. On
    // Windows (Wine's too) the path comes with backslashes and a drive
    CHECK(pe::folderOf("/hm/u/instruments/chimes/instrument.json") == "/hm/u/instruments/chimes");
    CHECK(pe::folderOf("/instrument.json") == "/" && pe::folderOf("instrument.json").empty());
    CHECK(pe::fileNameOf("/hm/u/scales/werckmeister.scl") == "werckmeister.scl" && pe::fileNameOf("a.scl") == "a.scl");
#ifdef _WIN32
    CHECK(pe::folderOf("Z:\\home\\u\\instruments\\chimes\\instrument.json") == "Z:\\home\\u\\instruments\\chimes");
    CHECK(pe::folderOf("C:\\instrument.json") == "C:\\" && pe::folderOf("C:/x/y.scl") == "C:/x");
    CHECK(pe::fileNameOf("Z:\\scales\\werckmeister.scl") == "werckmeister.scl");
#endif
}

TEST(the_parameter_table_only_grows)
{
    // the user parameters as released: a host saves them by symbol (LV2's ports, DPF's VST3 and CLAP
    // state) and automates them by index (VST3, CLAP). A locked row never changes; a new parameter
    // goes at the end and is recorded with it (PE_WRITE_PARAMS_LOCK=1 appends the new rows, never
    // rewrites one). Changing a locked row is a deliberate edit of the lock (docs/compatibility.md).
    const std::string path = std::string(PE_TESTS_DIR) + "/params.lock";
    std::vector<LockRow> lock;
    const bool read = readLock(path, lock);
    if (const char* w = std::getenv("PE_WRITE_PARAMS_LOCK"); w != nullptr && *w && read == std::ifstream(path).good()) {
        std::ofstream out(path, std::ios::app);
        if (lock.empty())
            out << "# plugin/tests/params.lock: the user parameters' index symbol min max default unit flags\n"
                   "# (a automatable, i integer, b boolean). Append only: docs/compatibility.md.\n";
        for (uint32_t i = (uint32_t)lock.size(); i < pe::kParamCount; ++i)
            out << lockLine(i) << "\n";
        out.close();
        lock.clear();
        CHECK(readLock(path, lock));
    }
    CHECK(read || !lock.empty());
    for (const LockRow& r : lock) {
        if (r.index >= pe::kParamCount) {
            std::printf("  params.lock: row %u (%s) is gone from the table\n", r.index, r.symbol.c_str());
            CHECK(r.index < pe::kParamCount);
            continue;
        }
        const pe::ParamInfo& d = pe::paramInfo(r.index);
        const bool same = r.symbol == d.symbol && r.min == d.min && r.max == d.max && r.def == d.def
            && r.unit == (d.unit != nullptr && *d.unit ? d.unit : "-") && r.flags == lockFlags(d);
        if (!same)
            std::printf("  params.lock: locked \"%s\", the table has \"%s\"\n", r.line.c_str(), lockLine(r.index).c_str());
        CHECK(same);
    }
    for (size_t i = 0; i < lock.size(); ++i)
        CHECK(lock[i].index == i);
    if (lock.size() < pe::kParamCount)
        std::printf("  params.lock: %u new parameter(s) not recorded: PE_WRITE_PARAMS_LOCK=1 plugin/build/ui_tests\n",
            pe::kParamCount - (uint32_t)lock.size());
    CHECK(lock.size() == pe::kParamCount);
}

TEST(no_diagnostic_is_automatable)
{
    // the diagnostics follow the user table and move when it grows: hosts save them by symbol and
    // automate only what is automatable, so they need no lock while none is
    for (uint32_t i = pe::kParamCount; i < pe::kTotalParams; ++i)
        CHECK(!pe::diagInfo(i).automatable);
}

TEST(param_table_is_complete)
{
    CHECK(kParamCount == 78);
    std::set<std::string> symbols;
    for (uint32_t i = 0; i < kParamCount; ++i) {
        const ParamInfo& p = paramInfo(i);
        CHECK(p.name != nullptr && p.symbol != nullptr);
        CHECK(symbols.insert(p.symbol).second);
        CHECK(p.min <= p.def && p.def <= p.max);
        if (const uint32_t n = paramLabelCount(i)) {
            CHECK(p.integer);
            CHECK_NEAR(p.max - p.min + 1, n, 0);
            for (uint32_t v = 0; v < n; ++v)
                CHECK(paramLabel(i, v) != nullptr);
        }
    }
    CHECK(paramLabelCount(kTemperament) == pe_temperament_count() + 1); // the named ones, then "Scala file"
    CHECK(std::strcmp(paramLabel(kTemperament, pe_temperament_count()), "Scala file") == 0);
    // Equal by default; the last entry plays each recorded key at its own pitch
    CHECK(paramInfo(kTemperament).def == 0.0f && std::strcmp(paramLabel(kTemperament, 0), "Equal") == 0);
    CHECK(std::strcmp(paramLabel(kTemperament, pe_temperament_count() - 1), "As recorded") == 0);
    CHECK(std::strcmp(paramInfo(kMicSpacing).unit, "cm") == 0);
    CHECK(paramInfo(kStop8).def == 1.0f && paramInfo(kStop4).def == 0.0f);
    CHECK(paramInfo(kBuff).def == 0.0f && paramInfo(kBuff).boolean);
    CHECK(paramInfo(kStop8b).def == 0.0f && paramInfo(kStop8b).boolean && std::string(paramInfo(kStop8b).symbol) == "stop_8b");
    // soft old felt: down to -24 dB (the engine clamps at 0.05 = -26 dB); hard felt is the hammer wear's
    CHECK(paramInfo(kHammerHardness).min == -24.0f && paramInfo(kHammerHardness).max == 12.0f);
    CHECK(std::pow(10.0f, paramInfo(kHammerHardness).min / 20.0f) >= 0.05f);
    CHECK(paramInfo(kLute).def == 0.0f && paramInfo(kLute).boolean && std::string(paramInfo(kLute).symbol) == "lute");
    CHECK(exclusiveStop(kStop8b) == kLute && exclusiveStop(kLute) == kStop8b && exclusiveStop(kStop8) == kParamCount);
    for (uint32_t i = 1; i < kParamCount; ++i) // the groups are contiguous, in order
        CHECK(paramInfo(i).group == paramInfo(i - 1).group || paramInfo(i).group == paramInfo(i - 1).group + 1);
    CHECK(paramInfo(kParamCount - 1).group == kGroupCount - 1);
}

TEST(custom_mic_parameters)
{
    CHECK(paramLabelCount(kMicPosition) == 6 && std::strcmp(paramLabel(kMicPosition, 5), "Custom") == 0);
    CHECK(paramInfo(kMicLX).group == kGroupMics && paramInfo(kMicWall).group == kGroupMics);
    CHECK(!paramInfo(kMicLX).automatable && !paramInfo(kMicWall).automatable);
    // inactive unless the position is Custom; the wall also needs an upright (profiles 3, 4)
    CHECK(!paramActive(kMicLX, 0, 1, 0) && !paramActive(kMicLX, 0, 0, 0));
    CHECK(paramActive(kMicLX, 0, 5, 0) && paramActive(kMicRZ, kUprA, 5, 0));
    CHECK(!paramActive(kMicWall, 0, 5, 0) && paramActive(kMicWall, kUprA, 5, 0));
    // the preset distance and spacing sliders sleep in Custom
    CHECK(!paramActive(kMicDistance, 0, 5, 0) && !paramActive(kMicSpacing, 0, 5, 0));
    CHECK(paramActive(kMicDistance, 0, 1, 0) && !paramActive(kMicDistance, 0, 0, 0));
    CHECK(paramInfo(kMicLPattern).max == 100 && std::strcmp(paramInfo(kMicLYaw).unit, "°") == 0);
    CHECK(paramActive(kMicRPattern, 0, 5, 0) && !paramActive(kMicRYaw, 0, 2, 0));
}

TEST(instruments_in_menu_order)
{
    // the catalog's menu: the known groups in order, by name inside each
    const char* const want[] = { "Grand A", "Grand B", "Grand C", "Grand D", "Grand E", "Grand F", "Grand G", "Grand H", "Grand I", "Upright A",
        "Upright B", "Upright C", "Upright D", "Harpsichord A", "Harpsichord B", "Harpsichord C", "Harpsichord D", "Harpsichord E",
        "Harpsichord F", "Clavichord", "Harp A",
        "Harp B", "Guitar", "Strumstick", "Balafon", "Bells", "Crotales", "Glockenspiel A", "Glockenspiel B", "Hand chimes", "Marimba", "Tubular bells",
        "Vibraphone", "Xylophone", "Kalimba", "Mbira A", "Mbira B", "Nyunga nyunga", "Dan tranh", "Psaltery" };
    const std::vector<const InstrumentEntry*> m = committed().menu();
    CHECK(m.size() == sizeof(want) / sizeof(*want));
    for (size_t i = 0; i < m.size() && i < sizeof(want) / sizeof(*want); ++i)
        CHECK(m[i]->name == want[i]);
    for (uint32_t i = 0; i < kProfileCount; ++i)
        CHECK(profileByDir(kProfiles[i]) == i && committed().find(kProfiles[i]) != nullptr);
    CHECK(profileByDir("nonesuch") == kProfileCount);
}

TEST(every_menu_instrument_is_exported)
{
    // instruments/ is committed: each menu entry must load from a fresh clone
    for (uint32_t i = 0; i < kProfileCount; ++i) {
        const std::string path = std::string(PE_INSTRUMENTS_DIR) + "/" + kProfiles[i] + "/instrument.json";
        FILE* f = std::fopen(path.c_str(), "r");
        CHECK(f != nullptr);
        if (f) std::fclose(f);
    }
}

TEST(spread_all_keys_is_a_microphone_toggle_where_the_body_has_a_range_of_its_own)
{
    CHECK(paramInfo(kSpreadKeys).boolean && paramInfo(kSpreadKeys).def == 1.0f && !paramInfo(kSpreadKeys).automatable);
    CHECK(paramInfo(kSpreadKeys).group == kGroupMics && paramKind(kSpreadKeys) == Kind::Toggle);
    // the bars, the harps, the guitars and the zithers; not the pianos, nor the harpsichords and
    // the clavichord (the grand's board: their keys along a full bridge already)
    for (uint32_t p = 0; p < kProfileCount; ++p) {
        const bool own = profileBars(p) || profileHarp(p) || profileGuitar(p) || profileStrumstick(p) || profileZither(p);
        CHECK(entryOf(p).info.spread == own);
        for (uint32_t mic = 0; mic < 6; ++mic)
            CHECK(paramActive(kSpreadKeys, p, mic, 0) == (own && mic != 0));
    }
    CHECK(!entryOf(kHarpA).info.spread && !entryOf(kCla).info.spread && !entryOf(0).info.spread && entryOf(kPsa).info.spread);
}

TEST(recorded_ranges_match_the_exports)
{
    // Instruments.cpp's table against instruments/<dir>/instrument.json "recorded"
    for (uint32_t i = 0; i < kProfileCount; ++i) {
        const std::string path = std::string(PE_INSTRUMENTS_DIR) + "/" + kProfiles[i] + "/instrument.json";
        FILE* f = std::fopen(path.c_str(), "r");
        CHECK(f != nullptr);
        if (!f) continue;
        std::string head(512, '\0');
        head.resize(std::fread(&head[0], 1, head.size(), f));
        std::fclose(f);
        const size_t at = head.find("\"recorded\": [");
        CHECK(at != std::string::npos);
        int lo = 0, hi = 0, want_lo = -1, want_hi = -1;
        if (at != std::string::npos)
            std::sscanf(head.c_str() + at, "\"recorded\": [%d, %d]", &want_lo, &want_hi);
        profileRecorded(i, lo, hi);
        CHECK(lo == want_lo && hi == want_hi);
    }
    int lo = 0, hi = 0;
    profileRecorded(kProfileCount, lo, hi);
    CHECK(lo == 21 && hi == 108);
}

TEST(format_shows_units_and_labels)
{
    CHECK(formatValue(kVolume, -6.0f) == "-6.0 dB");
    CHECK(formatValue(kVolume, kOffDb) == "Off");
    CHECK(formatValue(kRoom, 3.0f) == "+3.0 dB");
    CHECK(formatValue(kDynamics, 45.0f) == "45.0 dB"); // a range, not a gain: no sign
    CHECK(formatValue(kMicSpacing, 50.0f) == "50 cm");
    CHECK(formatValue(kMicDistance, 0.0f) == "Default");
    // the free pair's coordinates (and the wall) are plain meters, also zero and negative
    CHECK(formatValue(kMicLX, -0.25f) == "-0.25 m" && formatValue(kMicLY, 0.0f) == "0.00 m" && formatValue(kMicRZ, -0.5f) == "-0.50 m");
    CHECK(formatValue(kMicWall, 1.2f) == "1.20 m");
    CHECK(formatValue(kMicDistance, 1.25f) == "1.25 m");
    CHECK(formatValue(kConcertPitch, 440.0f) == "440.0 Hz");
    CHECK(formatValue(kStretch, 0.0f) == "As recorded" && formatValue(kStretch, 1.0f) == "+1.00" && formatValue(kStretch, -1.0f) == "-1.00");
    CHECK(formatValue(kCondition, 30.0f) == "30 % · Worn");
    CHECK(formatValue(kWearHammers, -20.0f) == "-20 %");
    CHECK(formatValue(kWearHammers, 20.0f) == "+20 %");
    CHECK(formatValue(kMicPosition, 1.0f) == "Player");
    CHECK(formatValue(kStop4, 1.0f) == "On");
    CHECK(formatValue(kStop4, 0.0f) == "Off");
    CHECK(formatValue(kBuff, 1.0f) == "On");
    CHECK(formatValue(kWearVariant, 7.0f) == "7");
}

TEST(parse_accepts_numbers_units_and_labels)
{
    float v = 0;
    CHECK(parseValue(kVolume, "-6", v) && v == -6.0f);
    CHECK(parseValue(kVolume, " -6.5 dB ", v) && v == -6.5f);
    CHECK(parseValue(kVolume, "off", v) && v == kOffDb);
    CHECK(parseValue(kMicPosition, "audience", v) && v == 2.0f);
    CHECK(parseValue(kMicPosition, "3", v) && v == 3.0f);
    CHECK(parseValue(kStop8, "on", v) && v == 1.0f);
    CHECK(parseValue(kWearVariant, "7.6", v) && v == 8.0f); // integers round
}

TEST(parse_rejects_or_clamps_bad_input)
{
    float v = 123.0f;
    CHECK(!parseValue(kCondition, "abc", v) && v == 123.0f);
    CHECK(!parseValue(kCondition, "", v) && v == 123.0f);
    CHECK(!parseValue(kCondition, "nan", v) && v == 123.0f);
    CHECK(!parseValue(kCondition, "inf", v) && v == 123.0f);
    CHECK(parseValue(kCondition, "1e9", v) && v == 100.0f);
    CHECK(parseValue(kCondition, "-5", v) && v == 0.0f);
    CHECK(!parseValue(kCondition, "-5 dB", v)); // another unit
    CHECK(!parseValue(kMicPosition, "nowhere", v));
}

TEST(normalized_round_trips_and_clamps)
{
    for (uint32_t i = 0; i < kParamCount; ++i) {
        const ParamInfo& p = paramInfo(i);
        CHECK_NEAR(fromNormalized(i, toNormalized(i, p.def)), p.def, 1e-4);
        CHECK(fromNormalized(i, -1.0f) == p.min && fromNormalized(i, 2.0f) == p.max);
    }
    CHECK(fromNormalized(kTemperamentKey, 0.52f) == 6.0f);
}

TEST(instrument_capabilities)
{
    for (uint32_t p = 0; p < kProfileCount; ++p) {
        const bool upright = p == kUprA || p == kUprB || p == kUprC, plucked = p == kHarpA || p == kHarpB || p == kHarpC || p == kHarpD || p == kHarpE || p == kHarpF || p == kHarp || p == kHarpB2 || p == kGui
            || p == kStr || p == kPsa || p == kDan;
        CHECK(profileBody(p) == (upright ? BodyKind::Upright : p == kHarp ? BodyKind::Harp : p == kHarpB2 ? BodyKind::LeverHarp
            : p == kVib ? BodyKind::Vibraphone : p == kMar ? BodyKind::Marimba : p == kXyl ? BodyKind::Xylophone
            : p == kBel ? BodyKind::Bells : p == kCro ? BodyKind::Crotales : p == kGui ? BodyKind::Guitar : p == kStr ? BodyKind::Strumstick : p == kPsa ? BodyKind::Psaltery : p == kDan ? BodyKind::DanTranh
            : p == kChi ? BodyKind::Chimes : p == kHch ? BodyKind::HandChimes : p == kKal || p == kMbA || p == kMbB || p == kNyu ? BodyKind::Kalimba : p == kBal ? BodyKind::Balafon : BodyKind::Grand));
        CHECK(profileHarp(p) == (p == kHarp || p == kHarpB2));
        CHECK(profileGuitar(p) == (p == kGui) && profileFingered(p) == (p == kHarp || p == kHarpB2 || p == kGui || p == kStr || p == kPsa || p == kDan));
        CHECK(profileZither(p) == (p == kPsa || p == kDan) && profileStrumstick(p) == (p == kStr));
        CHECK(profileVibraphone(p) == (p == kVib));
        CHECK(profilePlucked(p) == plucked);
        CHECK(profileHasRegisters(p) == (p == kHarpB || p == kHarpE || p == kHarpF));
    }
    CHECK(profileBody(profileByDir("grand_e")) == BodyKind::Grand && !profilePlucked(profileByDir("grand_e")));
    CHECK(std::string(displayName(kHammerHardness, kHarpA)) == "Quill brightness");
    CHECK(std::string(displayName(kHammerHardness, kHarp)) == "Finger brightness");
    CHECK(std::string(displayName(kHammerHardness, kStr)) == "Finger brightness");
    CHECK(std::string(displayName(kHammerHardness, kPsa)) == "Pick brightness");
    CHECK(std::string(displayName(kHammerHardness, kVib)) == "Mallet hardness");
    // the vibraphone (engine/tests/no_effect.rs INACTIVE_VIBRAPHONE): no lid, no buff
    for (uint32_t i : { kSympathetic, kPedalNoise, kPedalRing, kKnock, kRelease, kHammerGrooves, kWearAction, kUnisonWidth, kStretch, kWearUnisons })
        CHECK(!paramActive(i, kVib, 1, 0) && paramActive(i, 0, 1, 0));
    CHECK(!paramActive(kLid, kVib, 1, 0) && !profileHasBuff(kVib) && paramActive(kHammerHardness, kVib, 1, 0));
    CHECK(std::string(displayName(kHammerHardness, 0)) == "Hammer hardness");
    CHECK(std::string(displayName(kWearHammers, 0)) == "Hammers");
    CHECK(std::string(displayName(kWearVariant, 0)) == "Variant");
    // Task 3's harpsichord list (update together with engine/tests/no_effect.rs)
    for (uint32_t p : { kHarpA, kHarpB, kHarpC, kHarpD, kHarpE, kHarpF }) {
        CHECK(!paramActive(kPedalNoise, p, 1, 0) && !paramActive(kPedalRing, p, 1, 0));
        CHECK(!paramActive(kHammerGrooves, p, 1, 0));
        CHECK(!paramActive(kUnisonWidth, p, 1, 0));
        CHECK(!paramActive(kStretch, p, 1, 0));
        CHECK(!paramActive(kWearUnisons, p, 1, 0));
        CHECK(paramActive(kHammerHardness, p, 1, 0));
    }
    for (uint32_t i = 0; i < kParamCount; ++i)
        if (i != kStop8 && i != kStop4 && i != kStop8b && i != kLute && i != kBuff && !(i >= kMicLX && i <= kMicRPitch) // the Custom pair: active only in Custom
            && i != kMotor && i != kMotorSpeed && i != kMotorDepth && i != kMallet // the vibraphone's, the marimba's
            && i != kBebung // the clavichord's
            && i != kBendRange // the dan tranh's
            && i != kMorphX && i != kMorphY // the Morph tab's pad, never a row
            && i != kSpreadKeys // the bars'
            && (i < kPluckPosition || i == kPrepMode)) // the preparations': below
            CHECK(paramActive(i, 0, 1, 0));
    CHECK(!paramActive(kStop4, 0, 1, 0) && paramActive(kStop4, kHarpB, 1, 0));
    CHECK(!paramActive(kStop8b, kHarpA, 1, 0) && paramActive(kStop8b, kHarpB, 1, 0));
    CHECK(!paramActive(kLute, 0, 1, 0) && !paramActive(kLute, kHarpA, 1, 0) && paramActive(kLute, kHarpB, 1, 0));
    // a recorded second 8′ (Harpsichords E and F): the 8′ and the 8′ II, no 4′, no lute
    for (uint32_t p : { kHarpE, kHarpF })
        CHECK(paramActive(kStop8, p, 1, 0) && !paramActive(kStop4, p, 1, 0) && paramActive(kStop8b, p, 1, 0)
            && !paramActive(kLute, p, 1, 0) && paramActive(kBuff, p, 1, 0));
    CHECK(!paramActive(kBuff, 0, 1, 0) && paramActive(kBuff, kHarpA, 1, 0) && paramActive(kBuff, kHarpB, 1, 0));
    // the harp: the harpsichords' inactive set and the release noise (engine/tests/no_effect.rs INACTIVE_HARP), no buff
    CHECK(!paramActive(kBuff, kHarp, 1, 0) && !paramActive(kRelease, kHarp, 1, 0) && paramActive(kRelease, kHarpA, 1, 0));
    // the buff's header button: the harpsichords only (the harp's Buff had stayed in the header, without effect)
    for (uint32_t p = 0; p < kProfileCount; ++p)
        CHECK(profileHasBuff(p) == (p == kHarpA || p == kHarpB || p == kHarpC || p == kHarpD || p == kHarpE || p == kHarpF));
    const auto hasBuffRow = [](uint32_t profile) {
        for (const Row& r : computeLayout(1200, kDefaultHeight, kTabSound, ctxOf(profile, 1, 0)).rows)
            if (r.param == kBuff)
                return true;
        return false;
    };
    CHECK(hasBuffRow(kHarpA) && !hasBuffRow(kHarp) && !hasBuffRow(kHarpB2) && !hasBuffRow(0));
    CHECK(!paramActive(kLid, kHarpB2, 1, 0) && !paramActive(kRelease, kHarpB2, 1, 0) && !paramActive(kBuff, kHarpB2, 1, 0));
    CHECK(std::string(displayName(kHammerHardness, kHarpB2)) == "Finger brightness");
    CHECK(!paramActive(kPedalNoise, kHarp, 1, 0) && !paramActive(kPedalRing, kHarp, 1, 0) && !paramActive(kStretch, kHarp, 1, 0) && paramActive(kHammerHardness, kHarp, 1, 0));
    CHECK(paramActive(kSympathetic, kHarp, 1, 0) && paramActive(kDamperFall, kHarp, 1, 0));
}

TEST(each_preparations_controls_work_only_in_its_own_mode)
{
    // None 0, Plucked 1, Muted 2, Harmonic 3, Bolt 4, Paper 5
    const uint32_t own[][2] = { { kPluckPosition, 1 }, { kPluckBrightness, 1 }, { kMuteStrength, 2 }, { kHarmonicNumber, 3 },
        { kBoltMass, 4 }, { kBoltPosition, 4 }, { kPaperBuzz, 5 } };
    for (const auto& o : own)
        for (uint32_t mode = 0; mode < 6; ++mode)
            CHECK(paramActive(o[0], 0, 1, mode) == (mode == o[1]));
    CHECK(paramActive(kPrepMode, 0, 1, 0) && paramActive(kPrepMode, kHarpA, 1, 3));
    // a harpsichord plucks anyway: no pluck position or brightness, but the rest works
    CHECK(!paramActive(kPluckPosition, kHarpA, 1, 1) && !paramActive(kPluckBrightness, kHarpB, 1, 1));
    CHECK(paramActive(kHarmonicNumber, kHarpA, 1, 3) && paramActive(kPaperBuzz, kHarpB, 1, 5));
    CHECK(paramLabelCount(kPrepMode) == 6 && std::string(paramLabel(kPrepMode, 4)) == "Bolt");
    CHECK(paramInfo(kBoltMass).automatable == false && paramInfo(kPaperBuzz).automatable);
    CHECK(paramInfo(kHarmonicNumber).integer && paramInfo(kHarmonicNumber).min == 2 && paramInfo(kHarmonicNumber).max == 8);
}

TEST(recording_grays_mic_controls)
{
    CHECK(!paramActive(kMicDistance, 0, 0, 0));
    CHECK(!paramActive(kMicSpacing, 0, 0, 0));
    CHECK(paramActive(kMicPosition, 0, 0, 0));
    CHECK(paramActive(kMicDistance, 0, 1, 0) && paramActive(kMicSpacing, 0, 3, 0));
}

static bool inside(const Rect& r, float w, float h) { return r.x >= 0 && r.y >= 0 && r.w > 0 && r.h > 0 && r.x + r.w <= w + 1e-3f && r.y + r.h <= h + 1e-3f; }

TEST(layout_keeps_the_volume_slider_usable)
{
    // the header's stop toggles must not squeeze Volume (Harpsichord B: four stops and Buff)
    for (float w = kBreakpoint; w <= 1200.0f; w += 10.0f)
        for (uint32_t p : { 0u, kHarpA, kHarpB })
            for (const Row& r : computeLayout(w, kDefaultHeight, kTabSound, ctxOf(p, 1, 0)).rows)
                if (r.param == kVolume)
                    CHECK(r.rect.w >= 120.0f);
}

TEST(each_instrument_shows_its_own_mallets)
{
    // the Mallet parameter is the instrument's own index; the menu shows the export's names
    const InstrumentEntry &mar = entryOf(kMar), &xyl = entryOf(kXyl), &bel = entryOf(kBel), &bal = entryOf(kBal);
    CHECK(entryMallets(mar) == 3 && malletLabel(mar, 0) == "Yarn" && malletLabel(mar, 2) == "Rubber");
    CHECK(entryMallets(xyl) == 2 && malletLabel(xyl, 0) == "Rosewood" && malletLabel(xyl, 1) == "Hard rubber");
    CHECK(entryMallets(entryOf(kVib)) == 0 && entryMallets(entryOf(0)) == 0 && malletLabel(xyl, 2).empty());
    // a value it has not plays its first
    CHECK(effectiveMallet(mar, 1) == 1 && effectiveMallet(xyl, 1) == 1 && effectiveMallet(xyl, 2) == 0 && effectiveMallet(xyl, 7) == 0);
    CHECK(profileBody(kXyl) == BodyKind::Xylophone && profileBars(kXyl) && !profileHasMotor(kXyl));
    CHECK(std::string(displayName(kHammerHardness, kXyl)) == "Mallet hardness" && !paramActive(kLid, kXyl, 1, 0));
    CHECK(entryMallets(bel) == 2 && malletLabel(bel, 0) == "Plastic" && malletLabel(bel, 1) == "Brass");
    CHECK(profileBody(kBel) == BodyKind::Bells && profileBars(kBel) && !profileHasMotor(kBel) && !paramActive(kLid, kBel, 1, 0));
    CHECK(!paramActive(kDamperFall, kBel, 1, 0) && !paramActive(kSustainLatch, kBel, 1, 0));
    CHECK(entryMallets(bal) == 3 && malletLabel(bal, 0) == "Traditional" && malletLabel(bal, 1) == "Hard" && malletLabel(bal, 2) == "Soft");
    // the crotales: one mallet, no menu
    CHECK(entryMallets(entryOf(kCro)) == 0 && !paramActive(kMallet, kCro, 1, 0) && profileBody(kCro) == BodyKind::Crotales
        && profileBars(kCro) && !paramActive(kDamperFall, kCro, 1, 0) && !paramActive(kLid, kCro, 1, 0));
    // the host sees numbers (the instrument's own index)
    CHECK(paramLabelCount(kMallet) == kMalletCount && std::string(paramLabel(kMallet, 0)) == "1" && paramInfo(kMallet).max == kMalletCount - 1);
}

TEST(mallet_past_the_count_plays_the_first)
{
    const InstrumentEntry& x = entryOf(kXyl); // two mallets
    CHECK(entryMallets(x) == 2 && effectiveMallet(x, 5) == 0 && effectiveMallet(x, 1) == 1);
    CHECK(effectiveMallet(entryOf(kMar), 2) == 2); // switching back: the stored value plays again
    // the engine gets the stored value and plays its first past its count (Engine::mallet)
    std::vector<float> v(kParamCount);
    for (uint32_t i = 0; i < kParamCount; ++i) v[i] = paramInfo(i).def;
    v[kMallet] = 5;
    CHECK(engineParams(v.data()).mallet == 5);
}

TEST(a_missing_instrument_is_kept)
{
    const Choice c = choose(committed(), "harp2");
    CHECK(c.entry == nullptr && c.id == "harp2");
    // nothing instrument-specific shows; what does not depend on the instrument does
    CHECK(!paramVisible(kBuff, UiContext { nullptr, 0, 0 }) && !paramVisible(kMallet, UiContext { nullptr, 1, 0 }));
    CHECK(paramVisible(kVolume, UiContext { nullptr, 0, 0 }) && paramVisible(kMicDistance, UiContext { nullptr, 1, 0 }));
    int lo = 0, hi = 0;
    entryRecorded(noInstrument(), lo, hi);
    CHECK(lo == 21 && hi == 108);
}

TEST(the_marimba_has_mallets_and_no_dampers)
{
    CHECK(kMar < kProfileCount && profileBody(kMar) == BodyKind::Marimba && groupOf(kMar) == groupOf(kVib));
    CHECK(profileBars(kMar) && profileBars(kVib) && !profileBars(0) && profileMarimba(kMar) && !profileVibraphone(kMar));
    for (uint32_t p = 0; p < kProfileCount; ++p)
        CHECK(profileHasMallets(p) == (p == kMar || p == kXyl || p == kBel || p == kBal)
            && paramActive(kMallet, p, 1, 0) == (p == kMar || p == kXyl || p == kBel || p == kBal));
    CHECK(entryMallets(entryOf(kMar)) == 3 && malletLabel(entryOf(kMar), 0) == "Yarn" && malletLabel(entryOf(kMar), 2) == "Rubber");
    CHECK(paramInfo(kMallet).integer && paramInfo(kMallet).def == 0.0f && paramInfo(kMallet).group == kGroupInstrument);
    CHECK(std::string(displayName(kHammerHardness, kMar)) == "Mallet hardness");
    // engine/tests/no_effect.rs INACTIVE_MARIMBA, the damper fall and the latch (no dampers), no lid, no motor
    for (uint32_t i : { kSympathetic, kPedalNoise, kPedalRing, kKnock, kRelease, kHammerGrooves, kDamperBuzz, kWearDampers, kWearAction,
             kUnisonWidth, kStretch, kWearUnisons, kDamperFall, kSustainLatch, kLid, kMotor, kMotorSpeed })
        CHECK(!paramActive(i, kMar, 1, 0));
    CHECK(paramActive(kHammerHardness, kMar, 1, 0) && paramActive(kRoom, kMar, 1, 0) && paramActive(kDamperFall, kVib, 1, 0));
    // the menu comes before the hardness
    const Layout l = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(kMar, 1, 0));
    float yMallet = -1, yHard = -1;
    for (const Row& r : l.rows) {
        if (r.param == kMallet) yMallet = r.rect.y;
        if (r.param == kHammerHardness) yHard = r.rect.y;
    }
    CHECK(yMallet >= 0 && yHard > yMallet);
}

TEST(the_motor_is_the_vibraphones)
{
    for (uint32_t p = 0; p < kProfileCount; ++p) {
        CHECK(profileHasMotor(p) == (p == kVib));
        for (uint32_t i : { kMotor, kMotorSpeed, kMotorDepth })
            CHECK(paramActive(i, p, 1, 0) == (p == kVib));
    }
    CHECK(paramInfo(kMotor).boolean && paramInfo(kMotor).def == 1.0f && std::string(paramInfo(kMotor).symbol) == "motor");
    CHECK(paramInfo(kMotorSpeed).def == 7.0f && paramInfo(kMotorSpeed).automatable && std::string(paramInfo(kMotorSpeed).unit) == "Hz");
    CHECK(paramInfo(kMotorDepth).def == 70.0f && paramInfo(kMotorDepth).group == kGroupInstrument);
    // the speed and depth follow the hammer hardness; the volume keeps its width beside the motor's toggle
    for (float w : { kBreakpoint, kDefaultWidth, 1200.0f }) {
        const Layout l = computeLayout(w, kDefaultHeight, kTabSound, ctxOf(kVib, 1, 0));
        float yHard = -1, ySpeed = -1, yDepth = -1;
        for (const Row& r : l.rows) {
            if (r.param == kHammerHardness) yHard = r.rect.y;
            if (r.param == kMotorSpeed) ySpeed = r.rect.y;
            if (r.param == kMotorDepth) yDepth = r.rect.y;
            if (r.param == kVolume) CHECK(r.rect.w >= 120.0f);
        }
        CHECK(yHard >= 0 && ySpeed > yHard && yDepth > ySpeed);
    }
}

TEST(the_bebung_is_the_clavichords)
{
    CHECK(kCla < kProfileCount && profileClavichord(kCla) && !profilePlucked(kCla) && !profileHasBuff(kCla));
    for (uint32_t p = 0; p < kProfileCount; ++p)
        CHECK(paramActive(kBebung, p, 1, 0) == (p == kCla));
    CHECK(paramInfo(kBebung).def == 30.0f && paramInfo(kBebung).min == 0.0f && paramInfo(kBebung).automatable
        && paramInfo(kBebung).group == kGroupInstrument && std::string(paramInfo(kBebung).symbol) == "bebung");
    CHECK(std::string(displayName(kHammerHardness, kCla)) == "Tangent brightness");
    // engine/tests/no_effect.rs INACTIVE_CLAVICHORD (double strung: the unisons act)
    for (uint32_t i : { kPedalNoise, kPedalRing, kHammerGrooves, kStretch })
        CHECK(!paramActive(i, kCla, 1, 0));
    for (uint32_t i : { kUnisonWidth, kWearUnisons, kRelease, kKnock, kDamperFall, kSympathetic })
        CHECK(paramActive(i, kCla, 1, 0));
    // engine/tests/no_effect.rs PREPARATIONS: as the harpsichords'
    const uint32_t* v = nullptr;
    CHECK(profilePreparations(kCla, &v) == 4 && v[0] == 2);
    // the depth follows the tangent brightness (the hammer hardness) on the Sound tab
    const Layout l = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(kCla, 1, 0));
    float yHard = -1, yBebung = -1;
    for (const Row& r : l.rows) {
        if (r.param == kHammerHardness) yHard = r.rect.y;
        if (r.param == kBebung) yBebung = r.rect.y;
    }
    CHECK(yHard >= 0 && yBebung > yHard);
}

TEST(the_bend_range_is_the_dan_tranhs)
{
    CHECK(kDan < kProfileCount && profileBends(kDan) && profileZither(kDan) && profileFingered(kDan));
    for (uint32_t p = 0; p < kProfileCount; ++p)
        CHECK(paramActive(kBendRange, p, 1, 0) == (p == kDan));
    CHECK(paramInfo(kBendRange).def == 2.0f && paramInfo(kBendRange).min == 0.0f && paramInfo(kBendRange).max == 12.0f
        && paramInfo(kBendRange).automatable && paramInfo(kBendRange).group == kGroupInstrument
        && std::string(paramInfo(kBendRange).symbol) == "bend_range");
    CHECK(std::string(displayName(kHammerHardness, kDan)) == "Pick brightness" && !paramActive(kLid, kDan, 1, 0));
    // engine/tests/no_effect.rs INACTIVE_HARP
    for (uint32_t i : { kPedalNoise, kPedalRing, kRelease, kHammerGrooves, kUnisonWidth, kStretch, kWearUnisons })
        CHECK(!paramActive(i, kDan, 1, 0));
    int lo = 0, hi = 0;
    profileRecorded(kDan, lo, hi);
    CHECK(lo == 47 && hi == 83);
    // with the core controls on the Sound tab
    const Layout l = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(kDan, 1, 0));
    bool shown = false;
    for (const Row& r : l.rows)
        shown = shown || r.param == kBendRange;
    CHECK(shown);
}

TEST(the_tubular_bells_are_bars_with_a_damper_bar)
{
    CHECK(kChi < kProfileCount && profileChimes(kChi) && profileBars(kChi) && profileBody(kChi) == BodyKind::Chimes);
    CHECK(!profileHasMotor(kChi) && !profileHasMallets(kChi) && !paramActive(kMallet, kChi, 1, 0) && !paramActive(kLid, kChi, 1, 0));
    // engine/tests/no_effect.rs INACTIVE_CHIMES; the damper bar: its fall and the latch act
    for (uint32_t i : { kSympathetic, kPedalNoise, kPedalRing, kKnock, kRelease, kHammerGrooves, kWearAction, kUnisonWidth, kStretch, kWearUnisons,
             kWearFalseBeats })
        CHECK(!paramActive(i, kChi, 1, 0));
    for (uint32_t i : { kDamperFall, kSustainLatch, kWearDampers, kDamperBuzz, kHammerHardness, kRoom })
        CHECK(paramActive(i, kChi, 1, 0));
    CHECK(std::string(displayName(kHammerHardness, kChi)) == "Mallet hardness");
    int lo = 0, hi = 0;
    profileRecorded(kChi, lo, hi);
    CHECK(lo == 60 && hi == 77);
}

TEST(the_hand_chimes_are_bars_damped_as_the_vibraphone)
{
    CHECK(kHch < kProfileCount && profileHandChimes(kHch) && profileBars(kHch) && profileBody(kHch) == BodyKind::HandChimes);
    CHECK(!profileHasMotor(kHch) && !profileHasMallets(kHch) && !paramActive(kMallet, kHch, 1, 0) && !paramActive(kLid, kHch, 1, 0));
    // engine/tests/no_effect.rs: INACTIVE_VIBRAPHONE; the damper: its fall and the latch act; metal tubes keep their
    // pitch, and have no soundboard, strings or false beats to wear (`Wear::on`)
    for (uint32_t i : { kSympathetic, kPedalNoise, kPedalRing, kKnock, kRelease, kHammerGrooves, kWearAction, kUnisonWidth, kStretch, kWearUnisons,
             kWearFalseBeats, kWearSoundboard, kWearStrings, kWearTuning })
        CHECK(!paramActive(i, kHch, 1, 0));
    for (uint32_t i : { kDamperFall, kSustainLatch, kWearDampers, kDamperBuzz, kHammerHardness, kRoom })
        CHECK(paramActive(i, kHch, 1, 0));
    CHECK(std::string(displayName(kHammerHardness, kHch)) == "Mallet hardness");
    int lo = 0, hi = 0;
    profileRecorded(kHch, lo, hi);
    CHECK(lo == 60 && hi == 96);
}

TEST(the_kalimba_is_plucked_tines_without_dampers)
{
    CHECK(kKal < kProfileCount && profileKalimba(kKal) && profileBars(kKal) && profileBody(kKal) == BodyKind::Kalimba);
    CHECK(!profileHasMotor(kKal) && !profileHasMallets(kKal) && !paramActive(kLid, kKal, 1, 0) && groupOf(kKal) == 7);
    // engine/tests/no_effect.rs: INACTIVE_MARIMBA (no dampers) but the knock (the thumb's click) and the
    // sympathetic resonance (the other tines ringing along); the tines drift in their bridge, but have no soundboard,
    // strings or false beats to wear (`Wear::on`)
    for (uint32_t i : { kPedalNoise, kPedalRing, kRelease, kHammerGrooves, kDamperBuzz, kWearDampers, kUnisonWidth,
             kStretch, kWearUnisons, kDamperFall, kSustainLatch, kWearFalseBeats, kWearSoundboard, kWearStrings })
        CHECK(!paramActive(i, kKal, 1, 0));
    for (uint32_t i : { kHammerHardness, kRoom, kWearTuning, kKnock, kWearAction, kSympathetic })
        CHECK(paramActive(i, kKal, 1, 0));
    CHECK(std::string(displayName(kHammerHardness, kKal)) == "Thumb brightness" && std::string(displayName(kKnock, kKal)) == "Thumb click");
    CHECK(std::string(displayName(kSympathetic, kKal)) == "Ringing tines");
    CHECK(std::string(displayDescription(kSympathetic, kKal)).find("tines") != std::string::npos);
    CHECK(std::string(displayDescription(kSympathetic, 0)) == paramInfo(kSympathetic).description);
    int lo = 0, hi = 0;
    profileRecorded(kKal, lo, hi);
    CHECK(lo == 43 && hi == 97);
}

TEST(the_mbiras_are_tines_like_the_kalimba)
{
    // the kalimba's tines with buzzers on the board (the membrane at each tine's measured level)
    for (uint32_t p : { kMbA, kMbB, kNyu }) {
        CHECK(p < kProfileCount && profileKalimba(p) && profileBars(p) && groupOf(p) == 7);
        for (uint32_t i : { kPedalNoise, kRelease, kDamperFall, kSustainLatch, kWearSoundboard, kWearStrings })
            CHECK(!paramActive(i, p, 1, 0));
        for (uint32_t i : { kHammerHardness, kKnock, kSympathetic, kWearTuning })
            CHECK(paramActive(i, p, 1, 0));
        CHECK(std::string(displayName(kSympathetic, p)) == "Ringing tines");
    }
    int lo = 0, hi = 0;
    profileRecorded(kMbA, lo, hi);
    CHECK(lo == 47 && hi == 85);
    profileRecorded(kNyu, lo, hi);
    CHECK(lo == 53 && hi == 77);
}

TEST(the_balafon_has_three_mallets_and_no_dampers)
{
    CHECK(kBal < kProfileCount && profileBalafon(kBal) && profileBars(kBal) && profileBody(kBal) == BodyKind::Balafon);
    CHECK(!profileHasMotor(kBal) && profileHasMallets(kBal) && paramActive(kMallet, kBal, 1, 0) && !paramActive(kLid, kBal, 1, 0));
    // engine/tests/no_effect.rs: INACTIVE_MARIMBA but the knock and the action's wear (the mallet's contact on the
    // wood) and the sympathetic resonance (the gourds' buzz)
    for (uint32_t i : { kPedalNoise, kPedalRing, kRelease, kHammerGrooves, kDamperBuzz, kWearDampers, kUnisonWidth,
             kStretch, kWearUnisons, kDamperFall, kSustainLatch })
        CHECK(!paramActive(i, kBal, 1, 0));
    CHECK(paramActive(kKnock, kBal, 1, 0) && paramActive(kWearAction, kBal, 1, 0) && std::string(displayName(kKnock, kBal)) == "Mallet click");
    CHECK(paramActive(kSympathetic, kBal, 1, 0) && std::string(displayName(kSympathetic, kBal)) == "Gourd buzz");
    CHECK(std::string(displayName(kHammerHardness, kBal)) == "Mallet hardness");
    int lo = 0, hi = 0;
    profileRecorded(kBal, lo, hi);
    CHECK(lo == 61 && hi == 89);
}

TEST(the_strumstick_and_the_psaltery_are_fingered_strings)
{
    // engine/tests/no_effect.rs INACTIVE_HARP (no dampers but the hand, no mechanism coming back up)
    for (uint32_t p : { kStr, kPsa }) {
        for (uint32_t i : { kPedalNoise, kPedalRing, kRelease, kHammerGrooves, kUnisonWidth, kStretch, kWearUnisons })
            CHECK(!paramActive(i, p, 1, 0));
        for (uint32_t i : { kSympathetic, kKnock, kDamperFall, kHammerHardness, kRoom })
            CHECK(paramActive(i, p, 1, 0));
        CHECK(!paramActive(kLid, p, 1, 0) && !profileHasBuff(p) && !profileBars(p)
            && profileBody(p) == (p == kStr ? BodyKind::Strumstick : BodyKind::Psaltery));
    }
    int lo = 0, hi = 0;
    profileRecorded(kStr, lo, hi);
    CHECK(lo == 50 && hi == 81);
    profileRecorded(kPsa, lo, hi);
    CHECK(lo == 70 && hi == 90);
}

TEST(layout_breakpoint)
{
    CHECK(!computeLayout(kBreakpoint - 1, 600, kTabSound, ctxOf(0, 0, 0)).wide);
    CHECK(computeLayout(kBreakpoint, 600, kTabSound, ctxOf(0, 0, 0)).wide);
}

TEST(param_kinds)
{
    CHECK(paramKind(kInstrumentRow) == Kind::Dropdown && paramKind(kMicPosition) == Kind::Dropdown);
    CHECK(paramKind(kStop8) == Kind::Toggle);
    CHECK(paramKind(kVolume) == Kind::Slider && paramKind(kWearVariant) == Kind::Slider);
}

static PeMicGeometry geom(uint32_t body, uint32_t pos, double spacing, double distance)
{
    PeMicGeometry g {};
    CHECK(pe_mic_geometry(body, pos, spacing, distance, 1.0, nullptr, &g));
    return g;
}

TEST(view_round_trip_and_fit)
{
    const PeMicGeometry g = geom(0, 1, 0.5, 0.0);
    const Rect r { 10, 20, 300, 200 };
    for (View v : { kTop, kFront }) {
        const ViewTransform t = fitView(g, v, r);
        double a = 0, b = 0;
        t.toWorld(t.toScreen(0.3, -0.4), a, b);
        CHECK_NEAR(a, 0.3, 1e-4); CHECK_NEAR(b, -0.4, 1e-4);
        // the pair and the case are inside the rect
        const Point2 m = v == kTop ? t.toScreen(g.mics[0][0], g.mics[0][1]) : t.toScreen(g.mics[0][0], g.mics[0][2]);
        CHECK(r.contains(m.x, m.y));
    }
    // from the front, up is up
    const ViewTransform f = fitView(g, kFront, r);
    CHECK(f.toScreen(0, 1).y < f.toScreen(0, 0).y);
}

TEST(drag_player_distance_and_spacing)
{
    const PeMicGeometry g = geom(0, 1, 0.5, 0.0); // grand player: distance along −y, spread along x
    MicDrag d;
    CHECK(d.begin(g, kTop, 1, g.mics[1][0], g.mics[1][1]));
    DragValues v = d.update(g.mics[1][0] + 0.1, g.mics[1][1] - 0.5);
    CHECK_NEAR(v.distance, g.distance + 0.5, 1e-6);
    CHECK_NEAR(v.spacingCm, 50 + 20, 1e-4);
    CHECK(d.begin(g, kTop, 0, g.mics[0][0], g.mics[0][1]));
    v = d.update(g.mics[0][0] + 0.1, g.mics[0][1]);
    CHECK_NEAR(v.spacingCm, 50 - 20, 1e-4);
    v = d.update(g.mics[0][0], g.mics[0][1] + 5.0); // towards the piano, past the limit
    CHECK_NEAR(v.distance, g.distance_min, 1e-6);
    CHECK(!d.begin(g, kFront, 0, g.mics[0][0], g.mics[0][2])); // not a height
}

TEST(drag_close_height_from_the_front)
{
    const PeMicGeometry g = geom(0, 3, 0.5, 0.0); // grand hammers: height
    MicDrag d;
    CHECK(d.begin(g, kFront, 0, g.mics[0][0], g.mics[0][2]));
    DragValues v = d.update(g.mics[0][0], g.mics[0][2] + 0.1);
    CHECK_NEAR(v.distance, g.distance + 0.1, 1e-6);
    v = d.update(g.mics[0][0], g.mics[0][2] + 5.0);
    CHECK_NEAR(v.distance, g.distance_max, 1e-6);
    // from above only the spacing moves
    CHECK(d.begin(g, kTop, 1, g.mics[1][0], g.mics[1][1]));
    v = d.update(g.mics[1][0] + 0.1, g.mics[1][1] + 0.3);
    CHECK_NEAR(v.distance, g.distance, 1e-6);
    CHECK_NEAR(v.spacingCm, 70, 1e-4);
    v = d.update(g.mics[1][0] + 5.0, g.mics[1][1]);
    CHECK_NEAR(v.spacingCm, g.spacing_max * 100, 1e-3);
}

TEST(harpsichords_have_reversed_keys)
{
    // the projects' menu blocks: dark naturals and light sharps on the harpsichords' strip
    for (uint32_t p : { kHarpA, kHarpB, kHarpC, kHarpD })
        CHECK(entryOf(p).info.reversed_keys);
    for (uint32_t p : { 0u, kUprA, kHarp, kMar })
        CHECK(!entryOf(p).info.reversed_keys);
}

TEST(harp_has_no_lid_and_is_drawn_from_the_side)
{
    CHECK(!paramActive(kLid, kHarp, 1, 0) && paramActive(kLid, kHarpA, 1, 0) && paramActive(kLid, 0, 1, 0));
    for (uint32_t pos = 1; pos < 5; ++pos) {
        const PeMicGeometry g = geom(2, pos, 0.5, 0.0);
        CHECK(g.body == 2 && g.front_axis == 1 && g.outline_front_count > 0 && !std::isfinite(g.lid_front[0][0]));
        // the side view frames the pair and the outline: both inside
        const Rect r { 0, 0, 300, 300 };
        const ViewTransform t = fitView(g, kFront, r);
        for (int i = 0; i < 2; ++i) {
            const Point2 m = t.toScreen(g.mics[i][1], g.mics[i][2]);
            CHECK(r.contains(m.x, m.y));
        }
        CHECK(hitMic(g, t, kFront, t.toScreen(g.mics[1][1], g.mics[1][2]), 3) == 1);
    }
    // the player behind (−y), the audience in front (+y)
    CHECK(geom(2, 1, 0.5, 0.0).mics[0][1] < 0 && geom(2, 2, 0.5, 0.0).mics[0][1] > 1);
    // the lever harp: the same, drawn smaller (its floor nearer the board's center)
    const PeMicGeometry lever = geom(3, 1, 0.5, 0.0), concert = geom(2, 1, 0.5, 0.0);
    CHECK(lever.body == 3 && lever.front_axis == 1 && lever.outline_front_count > 0 && lever.floor_z > concert.floor_z);
}

TEST(harp_close_pair_drags_in_the_side_view)
{
    const PeMicGeometry g = geom(2, 3, 0.5, 0.0); // in front of the soundboard: off it along its normal
    MicDrag d;
    CHECK(d.begin(g, kFront, 0, g.mics[0][1], g.mics[0][2]));
    // a pointer step along the normal (in y, z) moves the distance by its length
    const DragValues v = d.update(g.mics[0][1] + 0.1 * g.axis[1], g.mics[0][2] + 0.1 * g.axis[2]);
    CHECK_NEAR(v.distance, g.distance + 0.1, 1e-6);
    FreeDrag f;
    CHECK(f.begin(geom(2, 5, 0.0, 0.0), kFront, 0, 0, 0));
    double p[3];
    const PeMicGeometry c = geom(2, 5, 0.0, 0.0);
    f.update(0.3, -0.2, p); // the side view moves y and z
    CHECK_NEAR(p[0], c.mics[0][0], 1e-9); CHECK_NEAR(p[1], c.mics[0][1] + 0.3, 1e-9); CHECK_NEAR(p[2], c.mics[0][2] - 0.2, 1e-9);
}

TEST(instrument_menu_groups_every_instrument_once_by_type)
{
    const std::vector<std::string> g = committed().groups();
    CHECK((g == std::vector<std::string> { "Grand pianos", "Upright pianos", "Harpsichords", "Clavichords", "Harps", "Guitars",
        "Percussion", "Thumb pianos", "Zithers" }));
    std::vector<size_t> count(g.size(), 0);
    for (const InstrumentEntry* e : committed().menu())
        for (size_t i = 0; i < g.size(); ++i)
            if (e->group == g[i]) ++count[i];
    CHECK((count == std::vector<size_t> { 9, 4, 6, 1, 2, 2, 10, 4, 2 }));
    CHECK(groupOf(kCla) == 3 && groupOf(kGui) == 5 && groupOf(kStr) == 5 && groupOf(kPsa) == 8 && groupOf(kDan) == 8);
    CHECK(groupOf(kBal) == 6 && groupOf(kChi) == 6 && groupOf(kHch) == 6 && groupOf(kKal) == 7 && groupOf(kVib) == 6
        && groupOf(kMar) == 6 && groupOf(kXyl) == 6 && groupOf(kBel) == 6 && groupOf(kCro) == 6);
    CHECK(groupOf(kUprB) == 1 && groupOf(kHarpC) == 2 && groupOf(kHarpB2) == 4 && groupOf(profileByDir("grand_g")) == 0);
    // every group's names start like its type
    for (const InstrumentEntry* e : committed().menu())
        if (e->group == "Harps") CHECK(e->name.rfind("Harp ", 0) == 0);
}

TEST(submenu_opens_beside_its_row_and_stays_in_view)
{
    const Rect list { 100, 50, 200, 4 * kRowH };
    Rect s = submenuRect(list, 2, 3, 0, 1000, 1000);
    CHECK(s.x == list.x + list.w && s.y == list.y + 2 * kRowH && s.h == 3 * kRowH && s.w >= 200);
    const Rect right { 300, 50, 200, 4 * kRowH };       // no room on the right: to the left
    s = submenuRect(right, 2, 3, 0, 1000, 650);
    CHECK(s.x + s.w <= right.x + 1e-3f && s.x >= 0 && s.y == right.y + 2 * kRowH);
    s = submenuRect(list, 3, 5, 0, 200, 1000);           // would run past the bottom: moved up
    CHECK(s.y + s.h <= 200 + 1e-3f && s.y >= 0);
    s = submenuRect(list, 0, 20, 30, 200, 1000);          // taller than the view: from its top
    CHECK(s.y == 30);
}

TEST(a_list_taller_than_the_view_is_cut_to_it_and_scrolls)
{
    // 33 snapshots in a 600 px window: the submenu reached past the bottom, its last rows unreachable
    const Rect groups { 100, 50, 200, 9 * kRowH };
    const Rect s = submenuRect(groups, 8, 33, 0, 600, 1000);
    CHECK(s.y == 0 && s.h == 600 && s.y + s.h <= 600 + 1e-3f);
    CHECK(listMaxScroll(s, 33) == 33 * kRowH - 600);
    // a list that fits: no scroll, no bar
    const Rect small { 0, 0, 200, 3 * kRowH };
    CHECK(listMaxScroll(small, 3) == 0 && listBar(small, 3, 0).track.w == 0 && listScrollTo(small, 3, 2) == 0);
    // rows under the pointer follow the scroll; outside the rect, none
    CHECK(listRowAt(s, 33, 0, s.y + 1) == 0 && listRowAt(s, 33, 0, s.y + s.h - 1) == 19);
    CHECK(listRowAt(s, 33, listMaxScroll(s, 33), s.y + s.h - 1) == 32);
    CHECK(listRowAt(s, 33, 2.5f * kRowH, s.y + 1) == 2);
    CHECK(listRowAt(s, 33, 0, s.y - 1) == -1 && listRowAt(s, 33, 0, s.y + s.h) == -1);
    CHECK(listRowAt(small, 3, 0, 3 * kRowH - 1) == 2 && listRowAt({ 0, 0, 200, 600 }, 3, 0, 100) == -1);
    // the wheel: towards the top lowers the scroll, both ends hold
    CHECK(listWheel(s, 33, 0, 3 * kRowH) == 0);
    CHECK(listWheel(s, 33, 0, -3 * kRowH) == 3 * kRowH);
    CHECK(listWheel(s, 33, listMaxScroll(s, 33), -kRowH) == listMaxScroll(s, 33));
    // the current entry, centered where it can be: the first at the top, the last at the bottom
    CHECK(listScrollTo(s, 33, -1) == 0 && listScrollTo(s, 33, 0) == 0);
    CHECK(listScrollTo(s, 33, 32) == listMaxScroll(s, 33));
    for (int row : { 0, 10, 20, 32 }) {
        const float sc = listScrollTo(s, 33, row);
        CHECK(listRowAt(s, 33, sc, s.y + row * kRowH - sc + kRowH / 2) == row);
    }
    const float mid = listScrollTo(s, 33, 20);
    CHECK(std::fabs(s.y + 20.5f * kRowH - mid - (s.y + s.h / 2)) < 1e-3f);
    // the bar: along the right edge, its thumb from the top to the bottom as the list scrolls
    const ListBar top = listBar(s, 33, 0), bottom = listBar(s, 33, listMaxScroll(s, 33));
    CHECK(top.track.x + top.track.w == s.x + s.w && top.track.w == kListBarW && top.track.h == s.h);
    CHECK(top.thumb.y == s.y && std::fabs(bottom.thumb.y + bottom.thumb.h - (s.y + s.h)) < 1e-3f);
    CHECK(top.thumb.h >= kListThumbMin && top.thumb.h < s.h);
    CHECK(std::fabs(top.thumb.h - s.h * s.h / (33 * kRowH)) < 1e-3f);
    // dragging the thumb: its top maps back to the scroll, clamped at the ends
    CHECK(listScrollAtThumb(s, 33, s.y - 50) == 0);
    CHECK(listScrollAtThumb(s, 33, s.y + s.h) == listMaxScroll(s, 33));
    const ListBar m = listBar(s, 33, mid);
    CHECK(std::fabs(listScrollAtThumb(s, 33, m.thumb.y) - mid) < 1e-2f);
    // many rows: the thumb keeps its least height
    const Rect tall { 0, 0, 200, 120 };
    CHECK(listBar(tall, 400, 0).thumb.h == kListThumbMin);
}

TEST(a_dropdown_that_fits_nowhere_takes_the_roomier_side_and_scrolls)
{
    // fits below: below at full height, no bar
    const Rect a { 100, 50, 120, 22 };
    Rect l = dropdownListRect(a, 5, 0, 600, 30, 900);
    CHECK(l.y == 72 && l.h == 5 * kRowH && l.w == 200);
    // fits only above: above
    const Rect low { 100, 500, 120, 22 };
    l = dropdownListRect(low, 10, 0, 600, 30, 900);
    CHECK(l.y + l.h == 500 && l.h == 10 * kRowH);
    // fits neither: the roomier side, cut to the view, widened by the bar
    l = dropdownListRect(a, 40, 0, 600, 190, 900);
    CHECK(l.y == 72 && l.y + l.h == 600 && l.w >= 190 + 20 + kListBarW && listMaxScroll(l, 40) > 0);
    l = dropdownListRect(low, 40, 0, 600, 30, 900);
    CHECK(l.y == 0 && l.h == 500);
}

TEST(drag_upright_audience_moves_along_minus_y)
{
    const PeMicGeometry g = geom(1, 2, 0.5, 0.0);
    MicDrag d;
    CHECK(d.begin(g, kTop, 0, g.mics[0][0], g.mics[0][1]));
    CHECK_NEAR(d.update(g.mics[0][0], g.mics[0][1] - 1.0).distance, g.distance + 1.0, 1e-6);
}

TEST(no_hit_at_recording)
{
    const PeMicGeometry g = geom(0, 0, 0.5, 0.0);
    const ViewTransform t = fitView(g, kTop, Rect { 0, 0, 300, 200 });
    CHECK(hitMic(g, t, kTop, Point2 { 150, 100 }, 1000) == -1);
    MicDrag d;
    CHECK(!d.begin(g, kTop, 0, 0, 0));
}

TEST(markers_hit)
{
    const PeMicGeometry g = geom(0, 1, 0.5, 0.0);
    const ViewTransform t = fitView(g, kTop, Rect { 0, 0, 400, 300 });
    for (int i = 2; i < 5; ++i)
        CHECK(hitMarker(g, t, t.toScreen(g.markers[i][0], g.markers[i][1]), 6, 1) == i);
    // the current position's marker is not drawn and not clickable
    CHECK(hitMarker(g, t, t.toScreen(g.markers[1][0], g.markers[1][1]), 6, 1) == 0);
    CHECK(hitMarker(g, t, t.toScreen(g.markers[1][0], g.markers[1][1]), 6, 0) == 1);
}

TEST(ui_state_round_trip_and_bad_input)
{
    UiState s; s.tab = kTabCondition; s.zoom = 250;
    const UiState r = parseUiState(formatUiState(s).c_str());
    CHECK(r.tab == kTabCondition && r.zoom == 250);
    CHECK(formatUiState(s) == "tab=6;zoom=250;diag=0;view=0;lo=27.5;hi=16000;floor=-90"); // Morph is tab 1, Felt 3
    const UiState d = parseUiState("garbage");
    CHECK(d.tab == kTabSound && d.zoom == 100);
    CHECK(parseUiState("tab=9;zoom=999").zoom == 100);         // not a zoom step
    CHECK(parseUiState("tab=10;zoom=999").tab == kTabSound);   // no such tab
    CHECK(parseUiState(nullptr).zoom == 100 && parseUiState(nullptr).tab == kTabSound);
    // a state saved before the tabs: the open sections are dropped (Sound), the rest stays
    const UiState old = parseUiState("open=21;zoom=150;diag=1;view=1;lo=55;hi=8000;floor=-70");
    CHECK(old.tab == kTabSound && old.zoom == 150 && old.diag == 1 && old.view == 1);
    CHECK(old.lo == 55.0f && old.hi == 8000.0f && old.floorDb == -70.0f);
}

TEST(minimum_window_follows_the_host_scale_only)
{
    // a high zoom must not demand a window larger than the screen: the content scrolls
    uint32_t w = 0, h = 0;
    minimumWindowSize(2.0, w, h);
    CHECK(w == 840 && h == 720);
    minimumWindowSize(1.0, w, h);
    CHECK(w == 420 && h == 360);
}

TEST(minimum_window_height_preserves_a_small_controls_viewport)
{
    uint32_t w = 0, h = 0;
    minimumWindowSize(1.0, 1.0, 370, w, h); // full diagnostics panel: 300 + strip 46 + header 24
    CHECK(w == 420 && h == 610); // 370 diagnostics + a 240 px controls viewport, not another 360 px
    minimumWindowSize(1.25, 1.25, 24, w, h);
    CHECK(w == 525 && h == 450);
    minimumWindowSize(1.0, 1.5, 70, w, h);
    CHECK(h == 360); // the normal minimum still wins when the strip is small
    minimumWindowSize(1.0, 1.5, 370, w, h);
    CHECK(h == 795 && h - (uint32_t)std::lround(370 * 1.5) == 240);
}

TEST(parse_rejects_trailing_text_but_takes_the_unit)
{
    float v = 0;
    CHECK(!parseValue(kVolume, "5abc", v));
    CHECK(!parseValue(kVolume, "5 %", v));
    CHECK(parseValue(kVolume, "5dB", v) && v == 5.0f);
    CHECK(parseValue(kConcertPitch, "442 hz", v) && v == 442.0f);
    CHECK(parseValue(kStretch, "as recorded", v) && v == 0.0f);
    CHECK(parseValue(kStretch, "2.5", v) && v == 2.5f);
    CHECK(parseValue(kStretch, "12", v) && v == 10.0f); // clamped to the slider's range
    CHECK(parseValue(kStretch, "-3", v) && v == -1.0f);
    CHECK(parseValue(kMicDistance, "default", v) && v == 0.0f);
    CHECK(parseValue(kMicLX, "-1.5 m", v) && v == -1.5f && parseValue(kMicLY, "0", v) && v == 0.0f);
    CHECK(parseValue(kMicDistance, "Default", v) && v == 0.0f);
}

TEST(mic_distance_shows_what_the_engine_uses)
{
    CHECK(formatMicDistance(0.0f, true, 1.25) == "1.25 m (default)");
    CHECK(formatMicDistance(0.5f, true, 1.0) == "1.00 m (limited)");
    CHECK(formatMicDistance(2.0f, true, 2.0) == "2.00 m");
    CHECK(formatMicDistance(0.0f, false, 0.0) == "Default");
    CHECK(formatMicDistance(2.0f, false, 0.0) == "2.00 m");
    CHECK(formatMicDistance(0.0f, true, std::nan("")) == "Custom"); // the free pair (position 5) has no distance
}

TEST(drag_keeps_its_start_geometry)
{
    // a host change during the drag (the UI fetches a new geometry into the same struct)
    // does not move the drag's result: MicDrag keeps its own copy
    PeMicGeometry g = geom(0, 1, 0.5, 0.0);
    MicDrag d;
    CHECK(d.begin(g, kTop, 1, g.mics[1][0], g.mics[1][1]));
    const DragValues before = d.update(g.mics[1][0], g.mics[1][1] - 0.2);
    const double x = g.mics[1][0], y = g.mics[1][1];
    CHECK(pe_mic_geometry(0, 1, 2.0, 5.0, 1.0, nullptr, &g));
    const DragValues after = d.update(x, y - 0.2);
    CHECK(before.distance == after.distance && before.spacingCm == after.spacingCm);
}

TEST(free_drag_moves_one_microphone_in_the_views_plane)
{
    const PeMicGeometry g = geom(0, 1, 0.5, 0.0); // grand player: mics at (±0.25, −1.25, 0.45)
    FreeDrag d;
    CHECK(d.begin(g, kTop, 1, g.mics[1][0] + 0.05, g.mics[1][1]));   // grabbed 5 cm off the center
    double p[3];
    d.update(g.mics[1][0] + 0.55, g.mics[1][1] - 0.3, p);            // pointer moved (+0.5, −0.3)
    CHECK_NEAR(p[0], g.mics[1][0] + 0.5, 1e-9); CHECK_NEAR(p[1], g.mics[1][1] - 0.3, 1e-9); CHECK_NEAR(p[2], g.mics[1][2], 1e-9);
    d.end();
    CHECK(!d.active());
    CHECK(d.begin(g, kFront, 0, g.mics[0][0], g.mics[0][2]));
    d.update(g.mics[0][0] - 1.0, g.mics[0][2] + 0.4, p);             // the front view moves x and z
    CHECK_NEAR(p[0], g.mics[0][0] - 1.0, 1e-9); CHECK_NEAR(p[1], g.mics[0][1], 1e-9); CHECK_NEAR(p[2], g.mics[0][2] + 0.4, 1e-9);
    CHECK(d.which() == 0);
    // no pair at the recording's position: nothing to drag
    CHECK(!FreeDrag().begin(geom(0, 0, 0.5, 0.0), kTop, 0, 0, 0));
}

TEST(aim_arrow_hit_and_drag)
{
    PeMicGeometry g = geom(0, 5, 0.0, 0.0);               // the default free pair
    g.mics[0][0] = 0; g.mics[0][1] = 0; g.mics[0][2] = 0; // left mic at the origin of the plot
    g.yaw[0] = 90; g.pitch[0] = 0; g.pattern[0] = 0.5;     // aimed towards +x, with a pattern (so it has an arrow)
    ViewTransform t; t.scale = 100; t.ox = 0; t.oy = 0; t.flip = true;
    const Point2 tip = aimTip(g, t, kTop, 0, 40);          // 40 px long arrow
    CHECK_NEAR(tip.x, 40, 1e-3); CHECK_NEAR(tip.y, 0, 1e-3);
    CHECK(hitAim(g, t, kTop, 40, tip, 8) == 0);
    CHECK(hitAim(g, t, kTop, 40, Point2 { 0, 30 }, 8) == -1);
    AimDrag d;
    CHECK(d.begin(g, kTop, 0));
    double yaw = 0, pitch = 0;
    d.update(0.0, 1.0, yaw, pitch);                        // pointer straight at +y in world coordinates
    CHECK_NEAR(yaw, 0, 1e-9);
    d.update(-1.0, 0.0, yaw, pitch);                       // towards −x
    CHECK_NEAR(yaw, -90, 1e-9);
    AimDrag f;
    CHECK(f.begin(g, kFront, 0));
    f.update(1.0, 1.0, yaw, pitch);                        // front view: x across, z up; 45° above the horizon
    CHECK_NEAR(pitch, 45, 1e-9);
}

TEST(reset_covers_every_parameter)
{
    // the instrument is a state, not a parameter: Reset leaves it
    const std::vector<uint32_t> r = resetParams();
    CHECK(r.size() == kParamCount);
    for (uint32_t p : r)
        CHECK(p < kParamCount);
    CHECK(std::set<uint32_t>(r.begin(), r.end()).size() == r.size());
}

TEST(panic_and_the_menu_sit_in_a_row_at_the_right)
{
    for (float w : { kMinWidth, kDefaultWidth, 1600.0f }) {
        const Layout l = computeLayout(w, kDefaultHeight, kTabSound, ctxOf(0, 0, 0));
        CHECK(l.menu.x >= l.panic.x + l.panic.w && l.menu.y == l.panic.y && l.menu.w >= 32 && l.panic.w >= 48);
        CHECK(l.menu.x + l.menu.w <= w + 1e-3f);
        for (const Row& r : l.rows) // the header's controls end before them
            if (r.rect.y == l.panic.y)
                CHECK(r.rect.x + r.rect.w <= l.panic.x + 1e-3f);
    }
}

TEST(the_menu_holds_zoom_copy_and_reset)
{
    const std::vector<std::string> m = menuItems(150, false);
    CHECK(m.size() == (size_t)kMenuCount);
    CHECK(m[kMenuZoom].find("150 %") != std::string::npos && m[kMenuCopy] == "Copy CLI");
    CHECK(m[kMenuReset] == "Reset…" && menuItems(150, true)[kMenuReset].find("again") != std::string::npos);
}

static void checkNoOverlap(const Layout& l, float w, float h);
static bool has(const std::vector<uint32_t>& v, uint32_t x);

TEST(the_header_keeps_volume_and_the_instrument_readable_at_every_width)
{
    // every instrument, from the narrowest window up: Volume keeps 140 px, the instrument menu 120,
    // the toggles their labels' width (Harpsichord B's four stops, Buff and the latch once left
    // Volume nothing at 420 px, and squeezed the toggles under their labels just above 760)
    int bad = 0;
    for (uint32_t profile = 0; profile < kProfileCount; ++profile)
        for (float w = kMinWidth; w <= 1600; w += 10) {
            const Layout l = computeLayout(w, kDefaultHeight, kTabSound, ctxOf(profile, 0, 0));
            for (const Row& r : l.rows)
                if ((r.param == kVolume && r.rect.w < 140) || (r.param == kInstrumentRow && !r.scrolls && r.rect.w < 120)
                    || (has(headerParams(), r.param) && r.kind == Kind::Toggle && r.rect.w < (r.param == kSustainLatch ? 96 : 48))) {
                    if (++bad <= 5)
                        std::fprintf(stderr, "  %s at %.0f px: %s %.0f px\n", kProfiles[profile], w, paramInfo(r.param).symbol, r.rect.w);
                }
            checkNoOverlap(l, w, kDefaultHeight);
        }
    CHECK(bad == 0);
}

TEST(distance_parameters_explain_themselves)
{
    CHECK(paramInfo(kDistanceLevel).description != nullptr && std::strstr(paramInfo(kDistanceLevel).description, "Room to Off"));
    CHECK(paramInfo(kMicDistance).description != nullptr && std::strstr(paramInfo(kMicDistance).description, "default"));
}

TEST(both_views_put_the_treble_right_and_the_front_down)
{
    // seen from above with the front (keys, player, audience) towards the viewer at the
    // bottom: the treble (+x) is on the right as in the view from the front (not mirrored)
    const PeMicGeometry g = geom(1, 2, 0.5, 0.0);
    const Rect r { 0, 0, 300, 200 };
    const ViewTransform top = fitView(g, kTop, r), front = fitView(g, kFront, r);
    CHECK(top.toScreen(1, 0).x > top.toScreen(0, 0).x);
    CHECK(front.toScreen(1, 0).x > front.toScreen(0, 0).x);
    CHECK(top.toScreen(0, -1).y > top.toScreen(0, 1).y); // the front lies below the back
}

TEST(region_names_fit_macos_and_tokens_are_24_bit)
{
    CHECK(SharedRegion::nameForToken(0xABCDEF) == "/pe-abcdef");
    CHECK(SharedRegion::nameForToken(1) == "/pe-000001");
    CHECK(SharedRegion::nameForToken(0xFFFFFF).size() <= 31);
    for (int i = 0; i < 1000; ++i) {
        const uint32_t t = SharedRegion::randomToken();
        CHECK(t >= 1 && t <= 0xFFFFFF);
    }
}

TEST(region_is_shared_between_create_and_open)
{
    const std::string name = SharedRegion::nameForToken(SharedRegion::randomToken());
    auto a = SharedRegion::create(name, 4096);
    CHECK(a != nullptr);
    if (!a) return;
    auto b = SharedRegion::open(name, 4096);
    CHECK(b != nullptr);
    if (!b) return;
    static_cast<char*>(a->data())[100] = 42;
    CHECK(static_cast<char*>(b->data())[100] == 42);
    CHECK(a->size() == 4096);
}

TEST(create_refuses_an_existing_name)
{
    const std::string name = SharedRegion::nameForToken(SharedRegion::randomToken());
    auto a = SharedRegion::create(name, 4096);
    CHECK(a != nullptr);
    CHECK(SharedRegion::create(name, 4096) == nullptr); // a stale or live name: the caller retries with a new token
}

TEST(region_is_gone_after_its_creator)
{
    const std::string name = SharedRegion::nameForToken(SharedRegion::randomToken());
    {
        auto a = SharedRegion::create(name, 4096);
        CHECK(a != nullptr);
    }
    CHECK(SharedRegion::open(name, 4096) == nullptr);
    CHECK(SharedRegion::open("/pe-nonexistent-xyz", 4096) == nullptr);
}

TEST(open_rejects_a_region_smaller_than_asked)
{
    const std::string name = SharedRegion::nameForToken(SharedRegion::randomToken());
    auto a = SharedRegion::create(name, 4096);
    CHECK(a != nullptr);
    // far past the size: macOS and Windows report a region's size rounded up to their pages
    CHECK(SharedRegion::open(name, 1 << 20) == nullptr);
    CHECK(SharedRegion::open(name, 4096) != nullptr);
}

// the sweep of stale regions is Linux's (it reads /dev/shm): its tests too
#ifdef __linux__
namespace {
bool shmExists(const std::string& name) // name as "/pe-xxxxxx"
{
    struct stat st;
    return ::stat(("/dev/shm" + name).c_str(), &st) == 0;
}
// a pid that is dead: a child that has exited and been reaped
uint32_t deadPid()
{
    const pid_t p = fork();
    if (p == 0) _exit(0);
    int st = 0;
    waitpid(p, &st, 0);
    return (uint32_t)p;
}
// a prepared telemetry region under a fresh name, owned by `pid` (0: the owner is not set)
struct Owned {
    std::string name;
    std::unique_ptr<SharedRegion> region;
    explicit Owned(uint32_t pid)
    {
        name = SharedRegion::nameForToken(SharedRegion::randomToken());
        region = SharedRegion::create(name, pe_telemetry_region_size());
        if (region && pe_telemetry_init(region->data(), region->size()) && pid)
            pe_telemetry_set_owner(region->data(), region->size(), pid);
    }
};
// a raw file in /dev/shm with the given bytes (garbage, not a region)
std::string rawShmFile(const std::vector<unsigned char>& bytes)
{
    const std::string name = SharedRegion::nameForToken(SharedRegion::randomToken());
    const int fd = shm_open(name.c_str(), O_CREAT | O_EXCL | O_RDWR, 0600);
    if (fd < 0) return std::string();
    if (!bytes.empty() && write(fd, bytes.data(), bytes.size()) != (ssize_t)bytes.size()) { close(fd); shm_unlink(name.c_str()); return std::string(); }
    close(fd);
    return name;
}
} // namespace

TEST(sweep_removes_dead_owner)
{
    Owned dead(deadPid());
    CHECK(dead.region != nullptr && shmExists(dead.name));
    if (!dead.region) return;
    CHECK(pe_telemetry_owner(dead.region->data(), dead.region->size()) != 0);
    CHECK(SharedRegion::sweepStale() >= 1);
    CHECK(!shmExists(dead.name));
}

TEST(sweep_keeps_live_owner)
{
    Owned me((uint32_t)getpid());          // this process
    Owned parent((uint32_t)getppid());     // another live process of ours
    Owned init(1);                         // a live process we may not signal (kill gives EPERM): still alive
    for (Owned* o : { &me, &parent, &init }) CHECK(o->region != nullptr);
    if (!me.region || !parent.region || !init.region) return;
    SharedRegion::sweepStale();
    CHECK(shmExists(me.name));
    CHECK(shmExists(parent.name));
    CHECK(shmExists(init.name));
    // a region whose owner is live stays openable and intact
    CHECK(SharedRegion::open(me.name, pe_telemetry_region_size()) != nullptr);
}

TEST(sweep_keeps_a_region_still_being_set_up)
{
    Owned fresh(0);                         // initialized, owner not yet recorded (another instance starting right now)
    CHECK(fresh.region != nullptr);
    if (!fresh.region) return;
    SharedRegion::sweepStale();
    CHECK(shmExists(fresh.name));
}

TEST(sweep_ignores_foreign_or_garbage_files)
{
    const size_t full = pe_telemetry_region_size();
    std::vector<unsigned char> noise(full), zeros(full, 0), tiny(10, 0xAB);
    for (size_t i = 0; i < full; ++i) noise[i] = (unsigned char)(std::rand() >> 3);
    noise[0] = 0; noise[1] = 0; // not the magic, whatever else
    // right name and size but no header / random content / too small / empty
    std::vector<std::string> kept = { rawShmFile(noise), rawShmFile(zeros), rawShmFile(tiny), rawShmFile({}) };
    // a valid, dead-owner region under a name that is not ours (not "pe-" + 6 hex digits)
    Owned dead(deadPid());
    CHECK(dead.region != nullptr);
    const std::string foreign = "/pe-" + dead.name.substr(4) + "x";   // seven hex-ish characters
    const std::string foreign2 = "/pe-" + std::string(dead.name.substr(4, 5)) + "G";  // six, but not hex
    for (const std::string& f : { foreign, foreign2 }) {
        const int fd = shm_open(f.c_str(), O_CREAT | O_EXCL | O_RDWR, 0600);
        CHECK(fd >= 0);
        if (fd < 0) continue;
        CHECK(ftruncate(fd, (off_t)full) == 0);
        void* m = mmap(nullptr, full, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
        CHECK(m != MAP_FAILED);
        if (m != MAP_FAILED) { std::memcpy(m, dead.region->data(), full); munmap(m, full); } // header with a dead owner
        close(fd);
        kept.push_back(f);
    }
    for (const std::string& f : kept) CHECK(!f.empty() && shmExists(f));
    SharedRegion::sweepStale();
    for (const std::string& f : kept) {
        CHECK(!f.empty() && shmExists(f));
        if (!f.empty()) shm_unlink(f.c_str());
    }
    CHECK(!shmExists(dead.name)); // the genuine one went
}

TEST(sweep_ignores_an_out_of_range_owner_pid)
{
    // pids above 0x7FFFFFFF would reach kill(-n, 0), a process-group probe; ESRCH there must not unlink a live region
    for (uint32_t pid : { 0x80000001u, 0xFFFFFFFFu }) {
        Owned o(pid);
        CHECK(o.region != nullptr && shmExists(o.name));
        if (!o.region) continue;
        CHECK(pe_telemetry_owner(o.region->data(), o.region->size()) == pid);
        SharedRegion::sweepStale(); // may remove other tests' leftovers, never this one
        CHECK(shmExists(o.name));
    }
}
#endif

TEST(diagnostic_parameters_extend_the_table)
{
    CHECK(kParamCount == 78); // the user-visible table (diagnostics come after it)
    CHECK((uint32_t)kDiagPedalIn == (uint32_t)kParamCount);
    CHECK((uint32_t)kTotalParams == (uint32_t)kParamCount + 24);
    for (uint32_t i = kParamCount; i < kTotalParams; ++i) {
        const ParamInfo& p = diagInfo(i);
        CHECK(p.name != nullptr && p.symbol != nullptr);
        CHECK(p.def >= p.min && p.def <= p.max);
    }
    for (uint32_t i = kDiagKeys0; i <= kDiagKeys7; ++i) {
        CHECK(diagIsOutput(i));
        CHECK(diagInfo(i).max == 16777215.0f); // 24 bits: exact in a float
        CHECK(diagInfo(i).integer);
    }
    CHECK(!diagIsOutput(kDiagPedalIn) && !diagIsOutput(kDiagSostenutoIn) && !diagIsOutput(kDiagSoftIn) && diagIsOutput(kDiagSoftOut));
    CHECK(diagIsOutput(kDiagToken) && diagIsOutput(kDiagVoices) && diagIsOutput(kDiagPedalOut) && diagIsOutput(kDiagSostenutoOut));
    CHECK(diagInfo(kDiagSostenutoIn).max == 1.0f && diagInfo(kDiagSostenutoOut).max == 1.0f);
    // MIDI learn: the UI's inputs, the DSP's outputs; a learned CC and every target fit
    CHECK(!diagIsOutput(kDiagLearnIn) && !diagIsOutput(kDiagMapSeqIn) && diagIsOutput(kDiagLearnedOut) && diagIsOutput(kDiagInstrumentOut));
    CHECK(diagInfo(kDiagLearnIn).max >= (float)kTargetCount && diagInfo(kDiagLearnedOut).max >= (float)packLearned(15, 127, true));
    // the Felt tab's Original toggle: an input, 0 or 1
    CHECK(!diagIsOutput(kDiagFeltOriginalIn) && diagInfo(kDiagFeltOriginalIn).max == 1.0f && diagInfo(kDiagFeltOriginalIn).integer);
}

TEST(reset_leaves_the_diagnostics_alone)
{
    for (uint32_t p : resetParams())
        CHECK(p < kParamCount);
}


TEST(strip_has_88_keys_52_white_36_black)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 44 });
    CHECK(g.keys.size() == 88);
    int black = 0;
    for (const KeyShape& k : g.keys) black += k.black;
    CHECK(black == 36);
    CHECK(!isBlackKey(0) && isBlackKey(1) && !isBlackKey(2)); // A0, A#0, B0
    CHECK(!isBlackKey(87));                                    // C8
    CHECK(g.keys[0].r.x < g.keys[87].r.x);
    CHECK(g.pedal.x >= g.keys[87].r.x + g.keys[87].r.w - 0.01f);
}

TEST(strip_hits_the_key_under_the_pointer_and_black_wins)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 44 });
    int v = 0;
    const KeyShape& a4 = g.keys[48];
    const float cx = a4.r.x + a4.r.w / 2;
    CHECK(hitKey(g, cx, a4.r.y + a4.r.h * 0.9f, v) == 48); // lower part of a white key
    const int vLow = v;
    CHECK(hitKey(g, cx, a4.r.y + a4.r.h * 0.1f, v) == 48);
    // a black key (A#4 = 49) sits over the boundary: its upper part is the black key, below it the white
    const KeyShape& bb = g.keys[49];
    CHECK(bb.black);
    CHECK(hitKey(g, bb.r.x + bb.r.w / 2, bb.r.y + bb.r.h * 0.3f, v) == 49);
    CHECK(hitKey(g, bb.r.x + bb.r.w / 2, bb.r.y + bb.r.h + 2.0f, v) == 50); // falls through to the white key B4
    // harder further down the key
    int vTop = 0, vBot = 0;
    hitKey(g, cx, a4.r.y + a4.r.h * 0.15f, vTop);
    hitKey(g, cx, a4.r.y + a4.r.h * 0.95f, vBot);
    CHECK(vBot > vTop);
    hitKey(g, cx, a4.r.y, vTop);
    hitKey(g, cx, a4.r.y + a4.r.h - 0.001f, vBot);
    CHECK(vTop == 20 && vBot == 127);
    CHECK(vLow >= 1 && vLow <= 127);
    CHECK(hitKey(g, -5, 10, v) == -1);
    CHECK(hitKey(g, 450, 500, v) == -1);
}

TEST(pedal_depth_follows_the_pointer)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 44 });
    CHECK_NEAR(pedalDepthAt(g, g.pedal.y - 10), 0.0, 1e-6);
    CHECK_NEAR(pedalDepthAt(g, g.pedal.y + g.pedal.h + 10), 1.0, 1e-6);
    CHECK_NEAR(pedalDepthAt(g, g.pedal.y + g.pedal.h / 2), 0.5, 1e-3);
}

TEST(key_words_decode_like_the_engine_packs_them)
{
    float w[8] = {};
    w[0] = (float)(0b1011u);                     // key 0 held+ringing, key 1 ringing
    w[1] = (float)(0b01u);                       // key 12 held
    w[7] = (float)(0b10u << (2 * (87 % 12)));    // key 87 ringing
    CHECK(keyLight(w, 0).held && keyLight(w, 0).ringing);
    CHECK(!keyLight(w, 1).held && keyLight(w, 1).ringing);
    CHECK(keyLight(w, 12).held && !keyLight(w, 12).ringing);
    CHECK(!keyLight(w, 87).held && keyLight(w, 87).ringing);
    CHECK(!keyLight(w, 5).held && !keyLight(w, 5).ringing);
    float bad[8] = { -1, NAN, 1e30f, 0, 0, 0, 0, 0 };
    CHECK(!keyLight(bad, 0).held && !keyLight(bad, 12).held); // out-of-range words read as silence
    CHECK(!keyLight(bad, 24).held && !keyLight(bad, 24).ringing); // 1e30f
    float edge[8] = { 16777216.0f, 3.7f, 0, 0, 0, 0, 0, 0 };
    CHECK(!keyLight(edge, 0).held && !keyLight(edge, 0).ringing); // exactly 2^24: silence
    CHECK(keyLight(edge, 12).held && keyLight(edge, 12).ringing && !keyLight(edge, 13).held); // 3.7 -> 3
}

TEST(partials_line_up_above_their_keys)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 44 });
    CHECK_NEAR(freqToStripX(g, 440.0f), keyCenterX(g, 48), 0.01);   // A4
    CHECK_NEAR(freqToStripX(g, 27.5f), keyCenterX(g, 0), 0.01);     // A0
    const float sharp = freqToStripX(g, 440.0f * std::pow(2.0f, 10.0f / 1200.0f)); // 10 cents sharp
    CHECK(sharp > keyCenterX(g, 48) && sharp < keyCenterX(g, 49));
    CHECK_NEAR(freqToStripX(g, 5.0f), keyCenterX(g, 0), 0.01);      // below the range: clamped
    CHECK_NEAR(freqToStripX(g, 30000.0f), keyCenterX(g, 87), 0.01);
}

TEST(strip_player_never_leaves_a_note_on)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 44 });
    std::vector<int> on, off;
    StripPlayer p([&](int k, int) { on.push_back(k); }, [&](int k) { off.push_back(k); });
    int v;
    auto at = [&](int key, float fy) { const KeyShape& k = g.keys[key]; return std::pair<float, float>(k.r.x + k.r.w / 2, k.r.y + k.r.h * fy); };
    auto [x1, y1] = at(40, 0.9f);
    p.press(g, x1, y1);
    CHECK(on.size() == 1 && on[0] == 40 && p.current() == 40);
    auto [x2, y2] = at(41, 0.9f); // glissando: the next key (D4, white)
    p.move(g, x2, y2);
    CHECK(off.size() == 1 && off[0] == 40);
    CHECK(on.size() == 2 && p.current() == on.back());
    p.move(g, x2, y2);                 // no movement to another key: nothing new
    CHECK(on.size() == 2);
    p.move(g, 450, 500);               // the pointer left the keys: the note ends
    CHECK(p.current() == -1 && off.size() == 2);
    p.press(g, x1, y1);
    p.release();                       // also the cancel (focus lost)
    CHECK(p.current() == -1 && off.size() == 3);
    p.release();                       // idempotent
    CHECK(off.size() == 3);
    CHECK(on.size() == off.size());    // every note on has its note off
    (void)v;
}

TEST(adjacent_white_keys_share_an_exact_edge)
{
    for (float width : { 900.f, 901.f, 1000.f, 1234.5f, 777.f, 1920.f, 640.f }) {
        const StripGeometry g = computeStrip({ 0, 0, width, 44 });
        const float y = 40; // below the black keys
        int v;
        for (int k = 0; k + 1 < kKeys; ++k) {
            if (isBlackKey(k) || isBlackKey(k + 1)) continue;
            const float x = g.keys[k + 1].r.x;
            CHECK(hitKey(g, x, y, v) == k + 1);
            CHECK(hitKey(g, std::nextafter(x, -INFINITY), y, v) == k);
        }
        // no gap anywhere across the keys area
        for (float x = g.keysArea.x; x < g.keysArea.x + g.keysArea.w - 0.5f; x += 0.37f)
            CHECK(hitKey(g, x, y, v) >= 0);
    }
}

TEST(strip_player_move_without_press_starts_a_note)
{
    // documented: move over a key sounds a note even without a press (drag re-entry)
    const StripGeometry g = computeStrip({ 0, 0, 900, 44 });
    std::vector<int> on, off;
    StripPlayer p([&](int k, int) { on.push_back(k); }, [&](int k) { off.push_back(k); });
    const KeyShape& k = g.keys[48];
    p.move(g, k.r.x + k.r.w / 2, k.r.y + k.r.h * 0.9f);
    CHECK(on.size() == 1 && on[0] == 48 && p.current() == 48);
    p.release();
    CHECK(off.size() == 1 && p.current() == -1);
}

namespace {
std::vector<float> sine(float hz, float amp, float rate, size_t frames)
{
    std::vector<float> s(2 * frames);
    for (size_t i = 0; i < frames; ++i)
        s[2 * i] = s[2 * i + 1] = amp * std::sin(6.2831853f * hz * (float)i / rate);
    return s;
}
}

TEST(fft_finds_a_sine_and_reads_dbfs)
{
    Spectrogram sp;
    const auto s = sine(1000.0f, 0.5f, 48000.0f, 8192);
    const int cols = sp.push(s.data(), 8192);
    CHECK(cols >= 4);
    const auto& c = sp.column();
    CHECK((int)c.size() == 4096 / 2 + 1);
    int peak = 0;
    for (int i = 1; i < (int)c.size(); ++i) if (c[i] > c[peak]) peak = i;
    CHECK_NEAR(peak * 48000.0 / 4096.0, 1000.0, 12.0);   // within one bin
    CHECK_NEAR(c[peak], -6.0, 1.5);                       // 0.5 full scale ≈ −6 dBFS
    CHECK(c[peak + 40] < c[peak] - 40.0f);                // far bins well below
}

TEST(silence_is_the_floor_not_nan)
{
    Spectrogram sp;
    std::vector<float> z(2 * 8192, 0.0f);
    sp.push(z.data(), 8192);
    for (float v : sp.column()) CHECK(std::isfinite(v) && v <= -100.0f);
}

TEST(rows_map_log_frequency_and_the_ridge_lands_on_its_row)
{
    CHECK_NEAR(freqToRow(20000.0f, 20.0f, 20000.0f, 100), 0.0, 1e-3);   // top = fHi
    CHECK_NEAR(freqToRow(20.0f, 20.0f, 20000.0f, 100), 100.0, 1e-3);    // bottom = fLo
    CHECK_NEAR(freqToRow(632.4555f, 20.0f, 20000.0f, 100), 50.0, 0.1);  // geometric mid
    CHECK_NEAR(rowToFreq(freqToRow(440.0f, 27.5f, 16000.0f, 200), 27.5f, 16000.0f, 200), 440.0, 0.5);
    Spectrogram sp;
    const auto s = sine(1000.0f, 0.5f, 48000.0f, 8192);
    sp.push(s.data(), 8192);
    std::vector<float> rows(200);
    columnToRows(sp.column(), 48000.0f, 200, 27.5f, 16000.0f, -90.0f, rows.data());
    int best = 0;
    for (int i = 0; i < 200; ++i) if (rows[i] > rows[best]) best = i;
    CHECK(std::fabs(best - freqToRow(1000.0f, 27.5f, 16000.0f, 200)) < 3.0);
    CHECK(rows[best] > 0.8f);
    for (float v : rows) CHECK(v >= 0.0f && v <= 1.0f);
}

TEST(rows_above_nyquist_are_blank)
{
    struct Leg { float rate, fHi; };
    for (Leg leg : { Leg { 44100.0f, 48000.0f }, Leg { 48000.0f, 48000.0f }, Leg { 96000.0f, 60000.0f }, Leg { 22050.0f, 16000.0f } }) {
        Spectrogram sp;
        const auto s = sine(1000.0f, 0.5f, leg.rate, 8192);
        sp.push(s.data(), 8192);
        const int n = 150;
        std::vector<float> rows((size_t)n, 1.0f);
        const float nyquist = leg.rate / 2;
        columnToRows(sp.column(), leg.rate, n, 27.5f, leg.fHi, -90.0f, rows.data());
        int blankChecked = 0, shown = 0;
        for (int i = 0; i < n; ++i) {
            CHECK(std::isfinite(rows[(size_t)i]) && rows[(size_t)i] >= 0.0f && rows[(size_t)i] <= 1.0f);
            if (rowToFreq((float)(i + 1), 27.5f, leg.fHi, n) >= nyquist) { CHECK(rows[(size_t)i] == 0.0f); ++blankChecked; }
            if (rows[(size_t)i] > 0.0f) ++shown;
        }
        CHECK(shown > 0);
        if (leg.fHi > nyquist) CHECK(blankChecked > 0); // a leg with a range above Nyquist really checks blank rows

        // loud column: every bin at 0 dB, so an unblanked row above Nyquist would read 1.0 (a floor-level
        // column would read 0 either way and could not tell blanking from clamping to the top bin)
        {
            const int before = check::failures();
            const std::vector<float> loud(2049, 0.0f);
            std::vector<float> lr((size_t)n, 0.5f);
            columnToRows(loud, leg.rate, n, 27.5f, leg.fHi, -90.0f, lr.data());
            int blank = 0, lit = 0, straddle = 0;
            for (int i = 0; i < n; ++i) {
                const float lo = rowToFreq((float)(i + 1), 27.5f, leg.fHi, n), hi = rowToFreq((float)i, 27.5f, leg.fHi, n);
                if (lo >= nyquist) { CHECK(lr[(size_t)i] == 0.0f); ++blank; }
                else { CHECK(lr[(size_t)i] == 1.0f); ++lit; }
                if (lo < nyquist && hi >= nyquist) { CHECK(lr[(size_t)i] == 1.0f); ++straddle; }
            }
            CHECK(lit > 0);
            if (leg.fHi > nyquist) { CHECK(blank > 0); CHECK(straddle == 1); }
            if (check::failures() != before) std::printf("  ^ loud-column leg rate %g fHi %g\n", leg.rate, leg.fHi);
        }
    }
}

TEST(column_to_rows_survives_degenerate_input)
{
    Spectrogram sp;
    const auto s = sine(1000.0f, 0.5f, 48000.0f, 8192);
    sp.push(s.data(), 8192);
    std::vector<float> rows(200, 0.5f);
    auto inRange = [&](int n) { for (int i = 0; i < n; ++i) CHECK(std::isfinite(rows[(size_t)i]) && rows[(size_t)i] >= 0.0f && rows[(size_t)i] <= 1.0f); };
    columnToRows(std::vector<float> { -10.0f }, 48000.0f, 50, 27.5f, 16000.0f, -90.0f, rows.data()); // one bin
    inRange(50);
    columnToRows(std::vector<float> {}, 48000.0f, 50, 27.5f, 16000.0f, -90.0f, rows.data());       // empty: no access, no crash
    columnToRows(sp.column(), 48000.0f, 0, 27.5f, 16000.0f, -90.0f, rows.data());
    columnToRows(sp.column(), 0.0f, 50, 27.5f, 16000.0f, -90.0f, rows.data());                       // rate 0: all blank
    for (int i = 0; i < 50; ++i) CHECK(rows[(size_t)i] == 0.0f);
    columnToRows(sp.column(), std::nanf(""), 50, 27.5f, 16000.0f, -90.0f, rows.data());
    for (int i = 0; i < 50; ++i) CHECK(rows[(size_t)i] == 0.0f);
    columnToRows(sp.column(), 48000.0f, 50, 16000.0f, 27.5f, -90.0f, rows.data());                   // hi < lo
    inRange(50);
    columnToRows(sp.column(), 48000.0f, 200, 27.5f, 16000.0f, -90.0f, rows.data());                  // rows narrower than a bin
    inRange(200);
    for (float hz : { 0.0f, -5.0f, std::nanf("") }) {
        const float r = freqToRow(hz, 27.5f, 16000.0f, 100);
        CHECK(std::isfinite(r) && r >= 0.0f && r <= 100.0f);
    }
    for (auto lh : { std::pair<float, float> { 100.0f, 100.0f }, { 200.0f, 100.0f } }) {
        const float r = freqToRow(440.0f, lh.first, lh.second, 100);
        CHECK(std::isfinite(r) && r >= 0.0f && r <= 100.0f);
    }
}

TEST(spectrogram_clear_and_non_finite_input)
{
    Spectrogram sp;
    const auto s = sine(1000.0f, 0.5f, 48000.0f, 8192);
    sp.push(s.data(), 8192);
    sp.clear();
    std::vector<float> z(2 * 8192, 0.0f);
    CHECK(sp.push(z.data(), 8192) >= 4);
    for (float v : sp.column()) CHECK(v == -120.0f);
    std::vector<float> bad(2 * 8192, 0.1f);
    bad[2 * 5000] = INFINITY;
    bad[2 * 6000 + 1] = std::nanf("");
    Spectrogram sp2;
    sp2.push(bad.data(), 8192);
    for (float v : sp2.column()) CHECK(std::isfinite(v));
    sp2.push(bad.data(), 8192);
    for (float v : sp2.column()) CHECK(std::isfinite(v));
}

TEST(amplitudes_in_db_are_always_finite)
{
    CHECK_NEAR(amplitudeDb(1.0f, 1.0f), 0.0, 1e-6);
    CHECK_NEAR(amplitudeDb(0.1f, 1.0f), -20.0, 1e-4);
    CHECK(amplitudeDb(0.0f, 1.0f) == -200.0f);
    CHECK(amplitudeDb(1.0f, 0.0f) == -200.0f);
    CHECK(partialHeight01(1.0f, 1.0f, -60.0f) == 1.0f);
    CHECK(partialHeight01(0.001f, 1.0f, -60.0f) == 0.0f);   // exactly at the floor
    CHECK(partialHeight01(0.0f, 1.0f, -60.0f) == 0.0f);
    CHECK_NEAR(keyFrequency(48), 440.0, 1e-3);
    CHECK_NEAR(keyFrequency(0), 27.5, 1e-4);
}

namespace {
std::vector<float> noiseStereo(size_t frames)
{
    std::vector<float> s(2 * frames);
    unsigned x = 12345u;
    for (size_t i = 0; i < frames; ++i) {
        x = x * 1664525u + 1013904223u;
        const float l = ((x >> 8) & 0xffff) / 32768.0f - 1.0f;
        x = x * 1664525u + 1013904223u;
        const float r = ((x >> 8) & 0xffff) / 32768.0f - 1.0f;
        s[2 * i] = 0.3f * l + 0.3f * std::sin(0.01f * (float)i);
        s[2 * i + 1] = 0.3f * r;
    }
    return s;
}
}

TEST(spectrogram_is_independent_of_the_chunking)
{
    const size_t total = 30000;
    const auto s = noiseStereo(total);
    // reference: one frame at a time, every completed column recorded with the frame count at completion
    Spectrogram ref;
    std::vector<std::pair<size_t, std::vector<float>>> refCols;
    for (size_t i = 0; i < total; ++i)
        if (ref.push(s.data() + 2 * i, 1) > 0) refCols.push_back({ i + 1, ref.column() });
    CHECK(refCols.size() == (total - 4096) / 1024 + 1);

    Spectrogram sp;
    const size_t sizes[] = { 1, 7, 1000, 4097 };
    size_t pos = 0, k = 0, completed = 0, compared = 0;
    while (pos < total) {
        const size_t n = std::min(sizes[k++ % 4], total - pos);
        const int cols = sp.push(s.data() + 2 * pos, n);
        pos += n;
        completed += (size_t)cols;
        if (cols > 0) { // the newest column equals the reference column of the last hop boundary <= pos
            const std::vector<float>* want = nullptr;
            for (const auto& rc : refCols) if (rc.first <= pos) want = &rc.second;
            CHECK(want != nullptr && sp.column() == *want);
            ++compared;
        }
    }
    CHECK(completed == refCols.size());
    CHECK(compared > 10);
}

TEST(spectrogram_stays_aligned_over_a_long_run)
{
    const size_t total = 1000000, N = 4096, hop = 1024;
    const auto s = noiseStereo(total);
    Spectrogram sp;
    size_t completed = 0;
    for (size_t pos = 0; pos < total; pos += 3000)
        completed += (size_t)sp.push(s.data() + 2 * pos, std::min<size_t>(3000, total - pos));
    CHECK(completed == (total - N) / hop + 1);
    const size_t e = N + hop * ((total - N) / hop); // frame count at the last column
    Spectrogram fresh;
    CHECK(fresh.push(s.data() + 2 * (e - N), N) == 1);
    CHECK(sp.column() == fresh.column());
}

static_assert(sizeof(PeTelemetryFrame) == 44224, "the frame layout is part of the C ABI (engine/src/telemetry.rs)");
static_assert(offsetof(PeTelemetryFrame, key) == 18880 && sizeof(PeTelemetryKey) == 288, "the keys' layout too");
static_assert(sizeof(PeDiag) == 48, "PeDiag layout");
static_assert(sizeof(PeMicSetup) == 104, "PeMicSetup layout");
static_assert(sizeof(PeTuning) == 272, "PeTuning layout");
static_assert(sizeof(PeMicGeometry) == 1360, "PeMicGeometry layout");

TEST(client_opens_by_token_and_controls_the_flag)
{
    TelemetryClient none;
    CHECK(!none.open(0));
    CHECK(!none.open(0xABCDE1)); // no such region
    CHECK(!none.isOpen());
    PeTelemetryFrame f;
    float buf[64];
    CHECK(!none.pollFrame(f));
    CHECK(none.pollAudio(buf, 32) == 0);

    const uint32_t token = SharedRegion::randomToken();
    auto dsp = SharedRegion::create(SharedRegion::nameForToken(token), pe_telemetry_region_size());
    CHECK(dsp != nullptr);
    if (!dsp) return;
    CHECK(pe_telemetry_init(dsp->data(), dsp->size()));
    TelemetryClient ui;
    CHECK(ui.open(token) && ui.isOpen());
    CHECK(!ui.pollFrame(f));             // nothing written yet
    CHECK(ui.pollAudio(buf, 32) == 0);
    ui.setEnabled(true);
    TelemetryClient other;
    CHECK(other.open(token));
    other.setEnabled(false);             // another view of the same flag
    ui.close();                          // clears it, closes
    CHECK(!ui.isOpen());
    CHECK(!ui.open(0xFFFFFF == token ? 1 : 0xFFFFFF));
}

namespace {
// process `seconds` of audio in blocks of 64..512 frames; returns the frames processed
void processFor(PeEngine* e, double seconds, double rate)
{
    static const uint32_t sizes[] = { 64, 512, 128, 256, 192, 100 };
    std::vector<float> l(512), r(512);
    size_t done = 0, i = 0;
    const size_t total = (size_t)(seconds * rate);
    while (done < total) {
        const uint32_t n = std::min<size_t>(sizes[i++ % 6], total - done);
        pe_process(e, nullptr, l.data(), r.data(), n);
        done += n;
    }
}
// everything pending in the audio ring; `peak` is the largest |sample| seen. One empty poll is
// not the end: after detecting a lap the engine returns 0 with data pending, so it takes two
// consecutive empty polls
size_t drainAudio(TelemetryClient& c, float& peak)
{
    std::vector<float> buf(2 * 4096);
    size_t total = 0;
    for (int empty = 0; empty < 2;) {
        const size_t n = c.pollAudio(buf.data(), 4096);
        empty = n == 0 ? empty + 1 : 0;
        total += n;
        for (size_t k = 0; k < 2 * n; ++k) peak = std::max(peak, std::fabs(buf[k]));
    }
    return total;
}
// the grand_a instrument directory: DUSSEK_INSTRUMENTS if it has one, else the source
// tree's; empty (and a stderr line, so that a skip is visible) when there is none
std::string salamanderDir(const char* test)
{
    const char* env = std::getenv("DUSSEK_INSTRUMENTS");
    for (const std::string& base : { std::string(env ? env : ""), std::string(PE_INSTRUMENTS_DIR) }) {
        if (base.empty()) continue;
        const std::string dir = base + "/grand_a";
        if (FILE* probe = std::fopen((dir + "/instrument.json").c_str(), "r")) {
            std::fclose(probe);
            return dir;
        }
    }
    std::fprintf(stderr, "SKIPPED %s: no instruments\n", test);
    return std::string();
}
// an engine with the telemetry region attached (the plugin's DSP side)
struct Rig {
    PeEngine* e = nullptr;
    std::unique_ptr<SharedRegion> dsp;
    uint32_t token = 0;
    static constexpr double rate = 48000.0;
    explicit Rig(const std::string& dir)
    {
        PeTuning tuning = {};
        tuning.a4_hz = 440.0;
        tuning.lid = 1.0;
        tuning.unison_width = 1.0;
        tuning.mic_spacing = 0.5;
        char err[512] = {};
        e = pe_create(dir.c_str(), rate, &tuning, nullptr, err, sizeof err);
        if (!e) { std::fprintf(stderr, "pe_create: %s\n", err); return; }
        token = SharedRegion::randomToken();
        dsp = SharedRegion::create(SharedRegion::nameForToken(token), pe_telemetry_region_size());
        if (!dsp || !pe_telemetry_init(dsp->data(), dsp->size()) || !pe_telemetry_attach(e, dsp->data(), dsp->size(), rate))
            dsp.reset();
    }
    bool ok() const { return e && dsp; }
    ~Rig()
    {
        if (e && dsp) pe_telemetry_attach(e, nullptr, 0, rate);
        dsp.reset();
        if (e) pe_destroy(e);
    }
    Rig(const Rig&) = delete;
    Rig& operator=(const Rig&) = delete;
};
} // namespace

TEST(telemetry_client_polls_notes_and_a_reader_leaves_the_flag)
{
    // the Touch tab reads the incoming velocities beside the diagnostics: its client must not
    // switch the diagnostics' telemetry off when it closes
    uint8_t v[16];
    TelemetryClient unopened;
    CHECK(unopened.pollNotes(v, 16) == 0);
    const std::string dir = salamanderDir("telemetry_client_polls_notes_and_a_reader_leaves_the_flag");
    if (dir.empty()) return;
    Rig rig(dir);
    CHECK(rig.ok());
    if (!rig.ok()) return;
    TelemetryClient diag;
    CHECK(diag.open(rig.token));
    diag.setEnabled(true);
    {
        TelemetryClient notes;
        CHECK(notes.open(rig.token, false));
        pe_note_on(rig.e, 0, 60, 33);
        pe_note_on(rig.e, 0, 64, 99);
        CHECK(notes.pollNotes(v, 16) == 2 && v[0] == 33 && v[1] == 99);
        CHECK(notes.pollNotes(v, 16) == 0);
    } // closed: the flag stays
    PeTelemetryFrame f;
    processFor(rig.e, 0.2, rig.rate);
    CHECK(diag.pollFrame(f)); // still enabled: frames keep coming
    // an LV2 host re-sends the token's port unchanged: opening the same region again keeps the
    // notes played since the last poll (a reopen at the head lost them)
    TelemetryClient notes;
    CHECK(notes.open(rig.token, false));
    pe_note_on(rig.e, 0, 67, 77);
    CHECK(notes.open(rig.token, false));
    CHECK(notes.pollNotes(v, 16) == 1 && v[0] == 77);
}

TEST(telemetry_client_reads_the_value_mirror)
{
    // MIDI-set values for hosts that take no change request: the DSP writes them, the UI reads them
    uint32_t seq = 9;
    float v = 0;
    TelemetryClient unopened;
    CHECK(!unopened.readValue(0, seq, v));
    const uint32_t token = SharedRegion::randomToken();
    std::unique_ptr<SharedRegion> r = SharedRegion::create(SharedRegion::nameForToken(token), pe_telemetry_region_size());
    CHECK(r && pe_telemetry_init(r->data(), r->size()));
    if (!r) return;
    TelemetryClient c;
    CHECK(c.open(token, false));
    CHECK(c.readValue(kLid, seq, v) && seq == 0);
    CHECK(pe_telemetry_set_value(r->data(), r->size(), kLid, 42.0f));
    CHECK(c.readValue(kLid, seq, v) && seq == 1 && v == 42.0f);
    CHECK(!c.readValue(PE_TELEMETRY_VALUES, seq, v));
    CHECK(kParamCount <= PE_TELEMETRY_VALUES); // every parameter has a slot
}

TEST(client_receives_frames_and_audio_from_an_engine)
{
    const std::string dir = salamanderDir("client_receives_frames_and_audio_from_an_engine");
    if (dir.empty()) return;
    const double rate = 48000.0;
    PeTuning tuning = {};
    tuning.a4_hz = 440.0;
    tuning.lid = 1.0;
    tuning.unison_width = 1.0;
    tuning.mic_spacing = 0.5;
    char err[512] = {};
    PeEngine* e = pe_create(dir.c_str(), rate, &tuning, nullptr, err, sizeof err);
    CHECK(e != nullptr);
    if (!e) { std::fprintf(stderr, "pe_create: %s\n", err); return; }

    const uint32_t token = SharedRegion::randomToken();
    auto dsp = SharedRegion::create(SharedRegion::nameForToken(token), pe_telemetry_region_size());
    CHECK(dsp != nullptr);
    if (!dsp) { pe_destroy(e); return; }
    CHECK(pe_telemetry_init(dsp->data(), dsp->size()));
    CHECK(pe_telemetry_attach(e, dsp->data(), dsp->size(), rate));
    {
        TelemetryClient ui;
        CHECK(ui.open(token));
        PeTelemetryFrame f;
        float peak = 0;

        // (a) disabled: a note plays, nothing is published
        pe_note_on(e, 0, 60, 100);
        processFor(e, 0.1, rate);
        CHECK(!ui.pollFrame(f));
        CHECK(drainAudio(ui, peak) == 0);

        // (b) enabled: frames with the voice, and audible audio
        ui.setEnabled(true);
        processFor(e, 0.3, rate);
        CHECK(ui.pollFrame(f));
        CHECK(f.voices >= 1);
        bool found = false;
        for (uint32_t v = 0; v < f.voices && v < PE_TELEMETRY_VOICES; ++v)
            if (f.voice[v].key == 60 - 21 && f.voice[v].count > 0) found = true;
        CHECK(found);
        peak = 0;
        const size_t got = drainAudio(ui, peak);
        CHECK(got > 0);
        CHECK(peak > 1e-6f);

        // (c) disabled again: after one drain, nothing new
        ui.setEnabled(false);
        processFor(e, 0.05, rate); // lets the engine notice the flag
        ui.pollFrame(f); // a frame published before the flag was seen may remain
        peak = 0;
        drainAudio(ui, peak);
        processFor(e, 0.2, rate);
        CHECK(!ui.pollFrame(f));
        CHECK(drainAudio(ui, peak) == 0);

        // (d) close() clears the flag: a second client sees no new frames
        ui.setEnabled(true);
        processFor(e, 0.1, rate);
        CHECK(ui.pollFrame(f));
        ui.close();
        TelemetryClient second;
        CHECK(second.open(token));
        peak = 0;
        drainAudio(second, peak);
        second.pollFrame(f); // a frame from before the close may remain
        processFor(e, 0.2, rate);
        CHECK(!second.pollFrame(f));
        CHECK(drainAudio(second, peak) == 0);
    }
    pe_telemetry_attach(e, nullptr, 0, rate);
    dsp.reset();
    pe_destroy(e);
}

TEST(clear_flag_switches_off_a_flag_a_crashed_ui_left_on)
{
    CHECK(!TelemetryClient::clearFlag(0));
    CHECK(!TelemetryClient::clearFlag(0xABCDE1)); // no such region
    const std::string dir = salamanderDir("clear_flag_switches_off_a_flag_a_crashed_ui_left_on");
    if (dir.empty()) return;
    Rig rig(dir);
    CHECK(rig.ok());
    if (!rig.ok()) return;
    // the UI that switched telemetry on is gone without closing (a crash): the flag stays on
    CHECK(pe_telemetry_set_enabled(rig.dsp->data(), rig.dsp->size(), true));
    pe_note_on(rig.e, 0, 60, 100);
    processFor(rig.e, 0.2, Rig::rate);
    TelemetryClient ui; // the next UI: finds frames waiting although it never asked for them
    CHECK(ui.open(rig.token));
    PeTelemetryFrame f;
    float peak = 0;
    CHECK(ui.pollFrame(f));
    drainAudio(ui, peak);
    CHECK(TelemetryClient::clearFlag(rig.token));
    processFor(rig.e, 0.05, Rig::rate); // lets the engine notice the flag
    ui.pollFrame(f);                    // a frame published before it was seen may remain
    drainAudio(ui, peak);
    processFor(rig.e, 0.3, Rig::rate);
    CHECK(!ui.pollFrame(f));
    CHECK(drainAudio(ui, peak) == 0);
    // clearing again is harmless, and the flag can be set again by the client
    CHECK(TelemetryClient::clearFlag(rig.token));
    ui.setEnabled(true);
    processFor(rig.e, 0.1, Rig::rate);
    CHECK(ui.pollFrame(f));
}

TEST(reopening_does_not_return_the_audio_written_before)
{
    const std::string dir = salamanderDir("reopening_does_not_return_the_audio_written_before");
    if (dir.empty()) return;
    Rig rig(dir);
    CHECK(rig.ok());
    if (!rig.ok()) return;
    pe_note_on(rig.e, 0, 60, 100);
    TelemetryClient first;
    CHECK(first.open(rig.token));
    first.setEnabled(true);
    processFor(rig.e, 0.3, Rig::rate); // about 14400 frames of audio: most of the ring
    float peak = 0;
    CHECK(drainAudio(first, peak) > 4000);
    processFor(rig.e, 0.1, Rig::rate); // more that the first client has not taken
    // a new client (the panel reopened): nothing old, only what is written after the open
    TelemetryClient second;
    CHECK(second.open(rig.token));
    second.setEnabled(true);
    std::vector<float> buf(2 * 4096);
    CHECK(second.pollAudio(buf.data(), 4096) == 0);
    processFor(rig.e, 0.05, Rig::rate); // 2400 frames
    peak = 0;
    const size_t got = drainAudio(second, peak);
    CHECK(got > 0 && got <= 2400);
    CHECK(peak > 1e-6f);
    // the old client, which kept its cursor, still has what it had not taken
    CHECK(drainAudio(first, peak) > got);
}

TEST(ui_state_keeps_the_strip_level_and_stays_backward_compatible)
{
    UiState s;
    s.tab = kTabTuning; s.zoom = 150; s.diag = 1;
    const std::string text = formatUiState(s);
    const UiState back = parseUiState(text.c_str());
    CHECK(back.tab == kTabTuning && back.zoom == 150 && back.diag == 1);
    const UiState old = parseUiState("open=3;zoom=125");   // a state saved before the diagnostics view
    CHECK(old.tab == kTabSound && old.zoom == 125 && old.diag == 0);
    CHECK(parseUiState("open=3;zoom=125;diag=9").diag == 0);   // out of range
    CHECK(parseUiState("open=3;zoom=125;diag=3").diag == 3);   // the visualization
    CHECK(parseUiState("garbage").diag == 0);
}


TEST(ui_state_keeps_the_panel_settings_and_rejects_nonsense)
{
    UiState s;
    s.diag = 2; s.view = 1; s.lo = 55.0f; s.hi = 8000.0f; s.floorDb = -70.0f;
    const UiState b = parseUiState(formatUiState(s).c_str());
    CHECK(b.diag == 2 && b.view == 1 && b.lo == 55.0f && b.hi == 8000.0f && b.floorDb == -70.0f);
    const UiState d = parseUiState("open=0;zoom=100;diag=2;view=1;lo=9000;hi=100;floor=5");
    CHECK(d.view == 1 && d.lo == 27.5f && d.hi == 16000.0f && d.floorDb == -90.0f); // inverted range and floor > 0: defaults
    CHECK(parseUiState("open=0;zoom=100;diag=2;view=7").view == 0);
    CHECK(parseUiState("open=0;zoom=100;diag=2;lo=nan;hi=inf;floor=nan").lo == 27.5f);
}

TEST(cents_from_the_nominal_harmonic)
{
    int n = 0;
    CHECK(std::fabs(centsFromHarmonic(220.0f, 55.0f, n)) < 1e-3f && n == 4);
    CHECK(std::fabs(centsFromHarmonic(55.0f * std::pow(2.0f, 10.0f / 1200.0f), 55.0f, n) - 10.0f) < 0.01f && n == 1);
    for (int h = 1; h <= 20; ++h) {
        const float c = centsFromHarmonic(55.0f * (float)h * 1.002f, 55.0f, n); // a little sharp
        CHECK(n == h && c > 0 && c < 5);
    }
    // A1: the fundamental is not listed; a partial near 2 f0 against the key's nominal pitch: small, not +57
    const float c = centsFromHarmonic(109.7f, keyFrequency(12), n);
    CHECK(n == 2 && std::fabs(c) < 10); // 109.7 is 4.7 cents flat of 2 * 55
    CHECK(centsFromHarmonic(110.3f, keyFrequency(12), n) > 0 && n == 2);
    CHECK(centsFromHarmonic(0.0f, 55.0f, n) == 0 && n == 1);
    CHECK(centsFromHarmonic(100.0f, 0.0f, n) == 0 && n == 1);
    CHECK(centsFromHarmonic(NAN, 55.0f, n) == 0 && n == 1);
    CHECK(centsFromHarmonic(100.0f, INFINITY, n) == 0 && n == 1);
    CHECK(centsFromHarmonic(20.0f, 55.0f, n) < 0 && n == 1); // below the fundamental: n stays 1
}

TEST(key_names_at_the_edges)
{
    CHECK(keyName(0) == "A0" && keyName(87) == "C8" && keyName(39) == "C4" && keyName(12) == "A1" && keyName(-5) == "A0" && keyName(200) == "C8");
}

TEST(ui_state_boundaries)
{
    CHECK(parseUiState("open=0;zoom=100;diag=2;lo=19;hi=8000;floor=-90").lo == 27.5f);
    CHECK(parseUiState("open=0;zoom=100;diag=2;lo=100;hi=24001;floor=-90").hi == 16000.0f);
    CHECK(parseUiState("open=0;zoom=100;diag=2;lo=100;hi=300;floor=-90").hi == 16000.0f); // ratio < 4
    CHECK(parseUiState("open=0;zoom=100;diag=2;lo=100;hi=400;floor=-90").hi == 400.0f);   // ratio exactly 4
    CHECK(parseUiState("open=0;zoom=100;diag=2;floor=-121").floorDb == -90.0f);
    CHECK(parseUiState("open=0;zoom=100;diag=2;floor=-19").floorDb == -90.0f);
    CHECK(parseUiState("open=0;zoom=100;diag=2;floor=-120").floorDb == -120.0f);
    const UiState m = parseUiState("open=0;zoom=100;diag=2;lo=100");
    CHECK(m.lo == 27.5f && m.hi == 16000.0f); // lo without hi: both default
}

// ---- the pedal mixer (Task 5 state machine, extracted from PianoPlugin)

namespace {
using Calls = std::vector<std::pair<int, float>>; // (step, depth)

/// the original PianoPlugin code (before the extraction), verbatim in behavior: the reference
struct OldPedal {
    uint8_t pedal = 0;
    float ui = -1.0f, mute = -2.0f, applied = 0;
    Calls* out = nullptr;
    int step = 0;
    void apply()
    {
        const float u = ui == mute ? 0.0f : ui;
        const float depth = std::max(pedal / 127.0f, u);
        if (depth != applied) { applied = depth; out->push_back({ step, depth }); }
    }
    void cc64(uint8_t v) { if (v != pedal) { pedal = v; apply(); } }
    void panic() { pedal = 0; applied = 0; mute = ui; apply(); }
    void activate(float now) { pedal = 0; applied = 0; ui = mute = now; }
    void readUi(float v) { if (v != ui) { mute = ui < 0.0f ? v : -2.0f; ui = v; apply(); } }
};

void note(Calls& c, int step, const PedalMixer::Call& k)
{
    if (k.call) c.push_back({ step, k.depth });
}
}

TEST(pedal_mixer_matches_the_old_code_for_a_cc64_stream_with_the_ui_at_0)
{
    // a recorded CC64 stream: on/off, half pedalling, repeats, a panic in the middle; the UI pedal is read as 0 every block
    const int stream[] = { 0, 127, 127, 64, 64, 90, 0, 0, 127, 1, -1 /*panic*/, 127, 127, 0, 33, 33, 127, 0, -1, -1, 127, 0 };
    Calls oldCalls, newCalls;
    OldPedal old; old.out = &oldCalls;
    PedalMixer mixer;
    int step = 0;
    old.readUi(0.0f); note(newCalls, step, mixer.setUi(0.0f)); // the first block
    for (int v : stream) {
        ++step; old.step = step;
        old.readUi(0.0f); note(newCalls, step, mixer.setUi(0.0f)); // read every block, never changes
        if (v < 0) { old.panic(); mixer.panic(); }
        else { old.cc64((uint8_t)v); note(newCalls, step, mixer.setCc((uint8_t)v)); }
        CHECK(mixer.applied() == old.applied);
    }
    CHECK(newCalls == oldCalls);
    CHECK(!newCalls.empty());
    // the original calls: 127 -> 1, 64 -> .504.., etc. spot check the first and that repeats make none
    CHECK_NEAR(newCalls[0].second, 1.0, 0);
    CHECK(newCalls.size() == 13); // the changes of the stream (repeats and the panic's own reset are no calls)
}

TEST(pedal_mixer_matches_the_old_code_for_a_random_mixed_stream)
{
    // UI values (repeats, stale values after panic/activate), CC64, panic, activate; a fixed seed, thousands of events
    uint32_t rng = 12345u;
    auto next = [&rng] { rng = rng * 1664525u + 1013904223u; return rng >> 8; };
    const float uiValues[] = { 0.0f, 0.0f, 0.25f, 0.5f, 0.5f, 1.0f };
    const uint8_t ccValues[] = { 0, 0, 1, 33, 64, 64, 90, 127 };
    Calls oldCalls, newCalls;
    OldPedal old; old.out = &oldCalls;
    PedalMixer mixer;
    float ui = 0.0f; // the parameter's current value, read once per block
    int calls = 0;
    for (int step = 1; step <= 6000; ++step) {
        old.step = step;
        const uint32_t r = next() % 100;
        if (r < 30) ui = uiValues[next() % 6];          // the parameter changes (often to the same value)
        if (r >= 30 && r < 60) { const uint8_t v = ccValues[next() % 8]; old.cc64(v); note(newCalls, step, mixer.setCc(v)); }
        else if (r >= 60 && r < 64) { old.panic(); mixer.panic(); }
        else if (r >= 64 && r < 68) { old.activate(ui); mixer.activate(ui); }
        old.readUi(ui); note(newCalls, step, mixer.setUi(ui)); // every block reads the parameter
        CHECK(mixer.applied() == old.applied);
        calls = (int)newCalls.size();
    }
    CHECK(newCalls == oldCalls);
    CHECK(calls > 500); // the stream really exercises the calls
}

TEST(pedal_mixer_walks_the_whole_state_machine)
{
    { // construction: nothing is applied, the very first UI read of 0 is no call, a sentinel never calls
        PedalMixer m;
        CHECK(m.applied() == 0 && !m.replayForNewEngine().call);
        CHECK(!m.setUi(0.0f).call);
        CHECK(!m.setCc(0).call);
    }
    { // UI 0 -> x -> 0, springing back
        PedalMixer m;
        m.setUi(0.0f);
        const PedalMixer::Call down = m.setUi(0.6f);
        CHECK(down.call && down.depth == 0.6f && m.applied() == 0.6f);
        CHECK(!m.setUi(0.6f).call);
        const PedalMixer::Call up = m.setUi(0.0f);
        CHECK(up.call && up.depth == 0.0f && m.applied() == 0.0f);
    }
    { // CC64 and the UI combine as the deeper
        PedalMixer m;
        m.setUi(0.0f);
        CHECK(m.setCc(127).call && m.applied() == 1.0f);
        CHECK(!m.setUi(0.5f).call);                // the UI is shallower: no change
        PedalMixer::Call c = m.setCc(0);
        CHECK(c.call && c.depth == 0.5f);          // the UI still holds
        c = m.setCc(127);
        CHECK(c.call && c.depth == 1.0f);
        c = m.setUi(0.0f);
        CHECK(!c.call && m.applied() == 1.0f);     // CC64 holds
        c = m.setCc(0);
        CHECK(c.call && c.depth == 0.0f);
        c = m.setCc(64); // half pedalling keeps its depth
        CHECK(c.call && std::fabs(c.depth - 64.0f / 127.0f) < 1e-7f);
    }
    { // construction with a non-zero UI value: stale, muted until it changes; a change is live
        PedalMixer m;
        CHECK(!m.setUi(0.7f).call && m.applied() == 0.0f);
        CHECK(!m.setUi(0.7f).call);
        CHECK(m.setCc(127).call);                   // CC works meanwhile
        CHECK(m.setCc(0).call && m.applied() == 0.0f); // the muted 0.7 does not hold it
        const PedalMixer::Call live = m.setUi(0.3f);
        CHECK(live.call && live.depth == 0.3f);     // a changed value is live again
        CHECK(m.setUi(0.7f).call && m.applied() == 0.7f); // and so is going back to the old stale value
    }
    { // panic while the UI is held: reset, muted, no call; a change is live
        PedalMixer m;
        m.setUi(0.0f);
        m.setUi(0.8f);
        m.setCc(127);
        m.panic();
        CHECK(m.applied() == 0.0f);
        CHECK(!m.setUi(0.8f).call);                 // unchanged: stays muted
        CHECK(!m.setCc(0).call);                    // the CC pedal was reset to 0 too
        PedalMixer::Call half = m.setCc(64);        // the held 0.8 is muted: only the CC counts
        CHECK(half.call && std::fabs(half.depth - 64.0f / 127.0f) < 1e-7f);
        CHECK(m.setCc(127).call && m.applied() == 1.0f);
        const PedalMixer::Call c = m.setUi(0.9f);   // CC holds 1.0: no call, but 0.9 is live now
        CHECK(!c.call);
        CHECK(m.setCc(0).call && m.applied() == 0.9f);
    }
    { // activate while held: as panic, from the value read now
        PedalMixer m;
        m.setUi(0.0f);
        m.setUi(0.5f);
        m.setCc(100);
        m.activate(0.5f);
        CHECK(m.applied() == 0.0f);
        CHECK(!m.setUi(0.5f).call);                 // stale: muted
        const PedalMixer::Call quarter = m.setCc(32);
        CHECK(quarter.call && std::fabs(quarter.depth - 32.0f / 127.0f) < 1e-7f); // only the CC counts
        m.setCc(0);
        CHECK(m.setUi(0.4f).call && m.applied() == 0.4f);
    }
    { // activate at 0: the next drag is live at once
        PedalMixer m;
        m.activate(0.0f);
        const PedalMixer::Call c = m.setUi(0.25f);
        CHECK(c.call && c.depth == 0.25f);
    }
}

TEST(pedal_mixer_replays_the_applied_depth_for_an_engine_that_did_not_adopt)
{
    PedalMixer m;
    CHECK(!m.replayForNewEngine().call);            // nothing applied: nothing to tell
    m.setUi(0.0f);
    m.setCc(127);
    PedalMixer::Call r = m.replayForNewEngine();
    CHECK(r.call && r.depth == 1.0f);
    CHECK(m.applied() == 1.0f);                     // the replay changes no state
    m.setCc(0);
    CHECK(!m.replayForNewEngine().call);
}

TEST(pedal_mixer_a_drag_at_exactly_the_stale_value_stays_muted_until_it_moves)
{
    // documented limitation (spec, Known limitations): the host restored 0.5, the UI is opened and the
    // user presses the lever at depth 0.5: the parameter does not change, so the pedal stays muted
    PedalMixer m;
    m.setUi(0.0f);
    m.activate(0.5f);
    CHECK(!m.setUi(0.5f).call && m.applied() == 0.0f); // the press at exactly 0.5
    CHECK(!m.setUi(0.5f).call);
    const PedalMixer::Call c = m.setUi(0.55f);           // the pointer moves: live
    CHECK(c.call && c.depth == 0.55f);
    // and the spring back to 0 is always live
    CHECK(m.setUi(0.0f).call && m.applied() == 0.0f);
}

// ---- the strip's pointer state machine (StripInteraction) with a fake host

namespace {
struct FakeStrip {
    std::vector<std::string> log; // "on 12", "off 12", "pedal 0.5 begin", "levelToggle" ...
    int brackets = 0;             // open pedal gestures (begin - end)
    int maxBrackets = 0;
    std::set<int> sounding;
    int stuckOrDouble = 0;        // a noteOn of a sounding key, or a noteOff of a silent one
    int levelToggles = 0, analysisToggles = 0, vizToggles = 0;
    float lastPedal = -1;
    int sostenutoBrackets = 0, sostenutoState = -1; // open sostenuto gestures; the last value sent
    int softBrackets = 0;
    float lastSoft = -1;
    StripHost host()
    {
        StripHost h;
        h.noteOn = [this](int k, int v) { log.push_back("on " + std::to_string(k)); (void)v; if (!sounding.insert(k).second) ++stuckOrDouble; };
        h.noteOff = [this](int k) { log.push_back("off " + std::to_string(k)); if (!sounding.erase(k)) ++stuckOrDouble; };
        h.pedal = [this](float d, bool begin, bool end) {
            log.push_back("pedal " + std::to_string(d) + (begin ? " begin" : "") + (end ? " end" : ""));
            if (begin) ++brackets;
            if (end) --brackets;
            maxBrackets = std::max(maxBrackets, brackets);
            lastPedal = d;
        };
        h.sostenuto = [this](bool down, bool begin, bool end) {
            log.push_back(std::string("sostenuto ") + (down ? "down" : "up") + (begin ? " begin" : "") + (end ? " end" : ""));
            sostenutoBrackets += (int)begin - (int)end;
            sostenutoState = down;
        };
        h.soft = [this](float d, bool begin, bool end) {
            log.push_back("soft " + std::to_string(d) + (begin ? " begin" : "") + (end ? " end" : ""));
            softBrackets += (int)begin - (int)end;
            lastSoft = d;
        };
        h.levelToggle = [this] { ++levelToggles; log.push_back("levelToggle"); };
        h.analysisToggle = [this] { ++analysisToggles; log.push_back("analysisToggle"); };
        h.vizToggle = [this] { ++vizToggles; log.push_back("vizToggle"); };
        return h;
    }
};

/// the view at `top` (window-logical): levels 2 and 3 have the 300 px panel above the header (as DiagView::layout)
StripLayout stripLayout(float top, int level, float width = 900)
{
    StripLayout l;
    l.top = top;
    l.panelH = level >= 2 ? 300.0f : 0.0f;
    const float header = top + l.panelH;
    l.keyboardTab = { 8, header + 3, 96, 18 };
    l.analysisTab = { 112, header + 3, 84, 18 };
    l.vizTab = { 204, header + 3, 108, 18 };
    l.strip = computeStrip({ 0, header + 24, width, 46 });
    return l;
}

struct StripRig {
    FakeStrip fake;
    StripInteraction in;
    StripLayout l;
    StripRig(int level = 1, float top = 100) : in(fake.host()), l(stripLayout(top, level)) { in.setLayout(l); in.setLevel(level); }
    Point2 key(int k, float fy = 0.5f) const { return { keyCenterX(l.strip, k), l.strip.keys[(size_t)k].r.y + l.strip.keys[(size_t)k].r.h * fy }; }
    Point2 soft(float depth) const { return { l.strip.soft.x + l.strip.soft.w / 2, l.strip.soft.y + l.strip.soft.h * depth }; }
    Point2 sostenuto() const { return { l.strip.sostenuto.x + l.strip.sostenuto.w / 2, l.strip.sostenuto.y + l.strip.sostenuto.h / 2 }; }
    Point2 pedal(float depth) const { return { l.strip.pedal.x + l.strip.pedal.w / 2, l.strip.pedal.y + l.strip.pedal.h * depth }; }
};
}

TEST(strip_interaction_key_drag_plays_and_releases_every_note)
{
    StripRig r;
    const Point2 a = r.key(39, 0.9f), b = r.key(40, 0.9f);
    CHECK(r.in.press(a.x, a.y) == StripInteraction::Press::Ours);
    CHECK(r.in.dragging() && r.fake.sounding.count(39));
    CHECK(r.in.motion(b.x, b.y)); // glissando: the next key replaces it
    CHECK(r.fake.sounding == std::set<int>({ 40 }));
    CHECK(r.in.motion(5.0f, 5.0f)); // off the keys (still dragging): silent, and still ours
    CHECK(r.fake.sounding.empty());
    CHECK(r.in.motion(a.x, a.y)); // back on the keys: plays again
    CHECK(r.fake.sounding == std::set<int>({ 39 }));
    CHECK(r.in.release());
    CHECK(r.fake.sounding.empty() && r.fake.stuckOrDouble == 0 && !r.in.dragging());
    CHECK(!r.in.motion(a.x, a.y)); // hovering plays nothing
    CHECK(r.fake.sounding.empty());
}

TEST(strip_interaction_pedal_drag_brackets_and_springs_back_to_0)
{
    StripRig r;
    const Point2 p = r.pedal(0.5f);
    CHECK(r.in.press(p.x, p.y) == StripInteraction::Press::Ours);
    CHECK(r.fake.brackets == 1 && r.in.pedalDragging());
    CHECK_NEAR(r.in.pedalSent(), 0.5, 0.03);
    const Point2 q = r.pedal(0.9f);
    CHECK(r.in.motion(q.x, q.y));
    CHECK(r.fake.brackets == 1 && r.fake.lastPedal > 0.8f && r.in.pedalSent() == r.fake.lastPedal);
    CHECK(r.in.release());
    CHECK(r.fake.brackets == 0 && r.fake.maxBrackets == 1);
    CHECK(r.fake.lastPedal == 0.0f && r.in.pedalSent() == 0.0f); // the pedal sends 0 on release
    CHECK(r.fake.log.back().find("end") != std::string::npos);
    CHECK(!r.in.release()); // a second release: nothing active, not ours, no second end
    CHECK(r.fake.brackets == 0 && r.fake.log.size() == 3);
    CHECK(r.fake.sounding.empty());
}

TEST(strip_has_two_separate_levers_inside_the_area)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 44 });
    CHECK(g.sostenuto.x + g.sostenuto.w <= g.pedal.x); // the sostenuto lies left of the sustain
    CHECK(g.keys[87].r.x + g.keys[87].r.w <= g.sostenuto.x + 0.01f);
    CHECK(g.sostenuto.w > 8 && g.sostenuto.h > 8);
    CHECK(g.pedal.x + g.pedal.w <= 900.0f && g.sostenuto.x >= g.keysArea.x + g.keysArea.w - 0.01f);
}

TEST(strip_has_three_levers_in_a_row_left_to_right_soft_sostenuto_sustain)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 44 });
    CHECK(g.soft.x + g.soft.w <= g.sostenuto.x && g.sostenuto.x + g.sostenuto.w <= g.pedal.x);
    CHECK(g.keys[87].r.x + g.keys[87].r.w <= g.soft.x + 0.01f && g.soft.x >= g.keysArea.x + g.keysArea.w - 0.01f);
    CHECK_NEAR(softDepthAt(g, g.soft.y - 10), 0.0, 1e-6);
    CHECK_NEAR(softDepthAt(g, g.soft.y + g.soft.h + 10), 1.0, 1e-6);
    CHECK_NEAR(softDepthAt(g, g.soft.y + g.soft.h / 2), 0.5, 1e-3);
}

TEST(strip_interaction_soft_drag_brackets_follows_the_pointer_and_springs_back)
{
    StripRig r;
    const Point2 p = r.soft(0.4f);
    CHECK(r.in.press(p.x, p.y) == StripInteraction::Press::Ours);
    CHECK(r.in.softDragging() && r.fake.softBrackets == 1 && !r.in.pedalDragging() && !r.in.sostenutoDragging());
    CHECK_NEAR(r.in.softSent(), 0.4, 0.03);
    const Point2 q = r.soft(0.9f);
    CHECK(r.in.motion(q.x, q.y));
    CHECK(r.fake.softBrackets == 1 && r.fake.lastSoft > 0.8f && r.in.softSent() == r.fake.lastSoft);
    CHECK(r.in.release());
    CHECK(r.fake.softBrackets == 0 && r.fake.lastSoft == 0.0f && r.in.softSent() == 0.0f);
    CHECK(!r.in.release());
    CHECK(r.fake.sounding.empty() && r.fake.brackets == 0 && r.fake.sostenutoBrackets == 0);
}

TEST(strip_interaction_a_dead_soft_lever_is_ours_but_does_nothing)
{
    StripRig r;
    r.in.setSoftEnabled(false);
    const Point2 p = r.soft(0.5f);
    CHECK(r.in.press(p.x, p.y) == StripInteraction::Press::Ours); // still no click-through to the controls
    CHECK(!r.in.softDragging() && !r.in.dragging() && r.fake.log.empty());
    r.in.setSoftEnabled(true);
    r.in.press(p.x, p.y);
    CHECK(r.in.softDragging());
    r.in.cancel(); // lost focus: closed exactly once
    r.in.cancel();
    CHECK(r.fake.softBrackets == 0 && r.fake.lastSoft == 0.0f && !r.in.dragging());
}

TEST(soft_pedal_is_inactive_on_the_harpsichords_only)
{
    // there is no soft pedal parameter: the strip's lever follows the plucked profiles
    CHECK(!profilePlucked(0) && !profilePlucked(kUprA) && !profilePlucked(kUprB));
    CHECK(profilePlucked(kHarpA) && profilePlucked(kHarpB));
}

TEST(soft_pedal_mixer_is_continuous_like_the_sustain)
{
    PedalMixer m;
    PedalMixer::Call c = m.setCc(64); // CC67 half way
    CHECK(c.call && std::fabs(c.depth - 64.0f / 127.0f) < 1e-6f);
    CHECK(!m.setCc(64).call);
    c = m.setUi(0.8f); // the first UI read is stale (muted)
    CHECK(!c.call);
    c = m.setUi(0.9f); // a live lever drag: the deeper of the two
    CHECK(c.call && c.depth == 0.9f);
    c = m.setUi(0.0f);
    CHECK(c.call && std::fabs(c.depth - 64.0f / 127.0f) < 1e-6f);
}

TEST(strip_interaction_sostenuto_is_down_while_pressed_and_springs_back)
{
    StripRig r;
    const Point2 s = r.sostenuto();
    CHECK(r.in.press(s.x, s.y) == StripInteraction::Press::Ours);
    CHECK(r.in.sostenutoDragging() && r.in.dragging() && r.fake.sostenutoBrackets == 1 && r.fake.sostenutoState == 1);
    CHECK(!r.in.pedalDragging() && r.fake.brackets == 0 && r.fake.sounding.empty()); // only this lever
    CHECK(r.in.motion(s.x, s.y - 3)); // moving while held: still ours, no second event
    CHECK(r.fake.log.size() == 1);
    CHECK(r.in.release());
    CHECK(r.fake.sostenutoBrackets == 0 && r.fake.sostenutoState == 0 && !r.in.sostenutoDragging());
    CHECK(r.fake.log.back() == "sostenuto up end");
    CHECK(!r.in.release() && r.fake.log.size() == 2); // nothing left to release
}

TEST(strip_interaction_a_lost_release_closes_the_sostenuto_before_the_next_press)
{
    StripRig r;
    const Point2 s = r.sostenuto(), p = r.pedal(0.5f);
    r.in.press(s.x, s.y);
    CHECK(r.in.press(p.x, p.y) == StripInteraction::Press::Ours); // the release never came
    CHECK(r.fake.sostenutoBrackets == 0 && r.fake.sostenutoState == 0 && !r.in.sostenutoDragging());
    CHECK(r.in.pedalDragging());
    r.in.press(s.x, s.y);
    r.in.cancel(); // focus lost: both closed, exactly once
    r.in.cancel();
    CHECK(r.fake.sostenutoBrackets == 0 && r.fake.brackets == 0 && !r.in.dragging());
}

TEST(strip_interaction_the_sostenuto_lever_exists_only_on_the_open_strip)
{
    StripRig r(0);
    const Point2 s = r.sostenuto();
    r.in.press(s.x, s.y);
    CHECK(!r.in.sostenutoDragging() && r.fake.log.empty());
}

TEST(sostenuto_mixer_cc66_and_the_lever_combine_as_on_off)
{
    PedalMixer m;
    PedalMixer::Call c = m.setCc(127); // CC66 >= 64 reaches the mixer as 127
    CHECK(c.call && c.depth == 1.0f);
    CHECK(!m.setCc(127).call);
    CHECK(!m.setUi(1.0f).call); // the first UI read is stale (muted)
    c = m.setUi(0.0f);
    CHECK(!c.call); // the CC still holds it down
    c = m.setCc(0);
    CHECK(c.call && c.depth == 0.0f);
    c = m.setUi(1.0f); // a live lever press
    CHECK(c.call && c.depth == 1.0f);
    c = m.setCc(127);
    CHECK(!c.call); // already down
    c = m.setUi(0.0f);
    CHECK(!c.call); // CC66 still holds it
}

TEST(strip_interaction_cancel_ends_what_is_active_exactly_once)
{
    StripRig r;
    const Point2 k = r.key(30);
    r.in.press(k.x, k.y);
    r.in.cancel();
    r.in.cancel();
    CHECK(r.fake.sounding.empty() && r.fake.stuckOrDouble == 0 && !r.in.dragging());
    const Point2 p = r.pedal(0.7f);
    r.in.press(p.x, p.y);
    r.in.cancel();
    r.in.cancel();
    CHECK(r.fake.brackets == 0 && r.fake.maxBrackets == 1 && r.fake.lastPedal == 0.0f);
    int ends = 0;
    for (const std::string& s : r.fake.log) ends += s.find(" end") != std::string::npos;
    CHECK(ends == 1);
}

TEST(strip_interaction_a_press_cancels_a_stale_drag_wherever_it_lands)
{
    // the lost release: the strip drag is still set when the next press arrives, ABOVE the strip (a control)
    for (int what = 0; what < 2; ++what) {
        StripRig r;
        const Point2 s = what == 0 ? r.key(20) : r.pedal(0.4f);
        r.in.press(s.x, s.y);
        CHECK(r.in.dragging());
        CHECK(r.in.press(300.0f, r.l.top - 10.0f) == StripInteraction::Press::Outside); // a control above
        CHECK(!r.in.dragging());                                                      // cancelled first
        CHECK(r.fake.sounding.empty() && r.fake.brackets == 0 && r.fake.stuckOrDouble == 0);
        CHECK(r.fake.lastPedal == 0.0f || what == 0);
        // the control's own release must not be claimed (PianoUI ends its edit bracket on a release we do not take)
        CHECK(!r.in.release());
    }
}

TEST(strip_interaction_a_stale_drag_is_cancelled_by_a_press_in_every_zone)
{
    const float x = 400;
    for (int zone = 0; zone < 4; ++zone) {
        StripRig r(2);
        const Point2 k = r.key(30);
        r.in.press(k.x, k.y);
        const float y = zone == 0 ? r.l.top + 50 : zone == 1 ? r.l.keyboardTab.y + 5 : zone == 2 ? r.l.strip.keysArea.y + 20 : r.l.top - 5;
        r.in.press(x, y);
        r.in.release();
        CHECK(r.fake.stuckOrDouble == 0);
        CHECK(!r.fake.sounding.count(30) || zone == 2); // zone 2 pressed another key (then released)
        CHECK(r.fake.sounding.empty());
        CHECK(r.fake.brackets == 0);
    }
}

TEST(strip_interaction_release_after_a_lost_release_does_not_claim_the_event)
{
    StripRig r;
    const Point2 k = r.key(50);
    r.in.press(k.x, k.y);
    // no release arrives (lost). The next press is on a control above: it cancels, and its release is not ours
    CHECK(r.in.press(200.0f, 20.0f) == StripInteraction::Press::Outside);
    CHECK(!r.in.release());
    // a control drag over the strip never starts a strip drag, and its release is not ours either
    CHECK(!r.in.motion(k.x, k.y));
    CHECK(!r.in.release());
    CHECK(r.fake.sounding.empty() && r.fake.brackets == 0);
    // the old code returned `true` here (the stale flag), leaving the control's edit bracket open: mutant check in the report
}

TEST(strip_interaction_routes_the_press_by_zone)
{
    { // level 0: only the Keyboard tab; the rest of the header is ours but plays nothing
        StripRig r(0);
        CHECK(r.in.press(r.l.keyboardTab.x + 3, r.l.keyboardTab.y + 3) == StripInteraction::Press::Ours && r.fake.levelToggles == 1);
        CHECK(r.in.press(r.l.analysisTab.x + 3, r.l.analysisTab.y + 3) == StripInteraction::Press::Ours && r.fake.analysisToggles == 0);
        CHECK(r.in.press(r.l.vizTab.x + 3, r.l.vizTab.y + 3) == StripInteraction::Press::Ours && r.fake.vizToggles == 0);
        const Point2 k = r.key(30), p = r.pedal(0.5f); // where a strip would be (stale geometry): level 0 has none
        CHECK(r.in.press(k.x, k.y) == StripInteraction::Press::Ours && !r.in.dragging());
        CHECK(r.in.press(p.x, p.y) == StripInteraction::Press::Ours && !r.in.dragging());
        CHECK(r.fake.sounding.empty() && r.fake.brackets == 0);
        CHECK(!r.in.release());
        CHECK(r.in.press(10, r.l.top - 1) == StripInteraction::Press::Outside);
        CHECK(r.fake.log.size() == 1); // only the one toggle
    }
    { // level 1: the three tabs, the pedal and the keys
        StripRig r(1);
        CHECK(r.in.press(r.l.analysisTab.x + 3, r.l.analysisTab.y + 3) == StripInteraction::Press::Ours && r.fake.analysisToggles == 1);
        CHECK(r.in.press(r.l.vizTab.x + 3, r.l.vizTab.y + 3) == StripInteraction::Press::Ours && r.fake.vizToggles == 1 && r.fake.analysisToggles == 1);
        CHECK(r.in.press(r.l.keyboardTab.x + 3, r.l.keyboardTab.y + 3) == StripInteraction::Press::Ours && r.fake.levelToggles == 1);
        CHECK(!r.in.dragging());
        CHECK(r.in.press(r.l.vizTab.x + r.l.vizTab.w + 100, r.l.keyboardTab.y + 3) == StripInteraction::Press::Ours); // header gap
        CHECK(r.fake.levelToggles == 1 && r.fake.analysisToggles == 1 && r.fake.vizToggles == 1);
        CHECK(!r.in.dragging());
        const Point2 p = r.pedal(0.2f);
        CHECK(r.in.press(p.x, p.y) == StripInteraction::Press::Ours && r.in.pedalDragging());
        r.in.release();
    }
    { // level 2: the panel is routed out, the header and strip as at level 1
        StripRig r(2);
        CHECK(r.in.press(300, r.l.top + 10) == StripInteraction::Press::Panel);
        CHECK(r.in.press(300, r.l.top + r.l.panelH - 0.5f) == StripInteraction::Press::Panel);
        CHECK(r.in.press(r.l.keyboardTab.x + 3, r.l.keyboardTab.y + 3) == StripInteraction::Press::Ours && r.fake.levelToggles == 1);
        CHECK(!r.in.dragging());
        const Point2 k = r.key(60);
        CHECK(r.in.press(k.x, k.y) == StripInteraction::Press::Ours && r.fake.sounding.count(60));
        r.in.release();
        CHECK(r.fake.sounding.empty() && r.fake.stuckOrDouble == 0);
    }
    { // level 3: the visualization's panel the same, its tab toggles it
        StripRig r(3);
        CHECK(r.in.press(300, r.l.top + 10) == StripInteraction::Press::Panel);
        CHECK(r.in.press(r.l.vizTab.x + 3, r.l.vizTab.y + 3) == StripInteraction::Press::Ours && r.fake.vizToggles == 1);
        const Point2 k = r.key(40);
        CHECK(r.in.press(k.x, k.y) == StripInteraction::Press::Ours && r.fake.sounding.count(40));
        r.in.release();
        CHECK(r.fake.sounding.empty());
    }
}

TEST(viz_hammer_strikes_rebounds_is_caught_and_falls)
{
    const float rate = 48000;
    const auto at = [&](float s) { return (uint64_t)(1000 + s * rate); };
    HammerTrack h { 1000, 2.0f, 0 };
    CHECK(hammerHeight(HammerTrack {}, at(1), rate) == 0.0f);           // never struck: at rest
    CHECK(hammerHeight(h, 1000, rate) == 1.0f && hammerHeight(h, 500, rate) == 1.0f); // at the strike (and a clock behind it)
    CHECK(hammerHeight(h, at(kVizContactS * 0.9f), rate) == 1.0f);      // held at the string for a frame or two
    const float back = hammerHeight(h, at(kVizContactS + 0.01f), rate);
    CHECK(back < 1.0f && back > kVizCheck);                              // falling back
    CHECK(hammerHeight(h, at(2), rate) == kVizCheck);                    // held: the backcheck catches it
    // released after a second: from the check down to rest, never below it
    HammerTrack r = h;
    r.upAt = at(1);
    CHECK(hammerHeight(r, at(1), rate) == kVizCheck);
    const float falling = hammerHeight(r, at(1.03f), rate);
    CHECK(falling < kVizCheck && falling > 0);
    CHECK(hammerHeight(r, at(2), rate) == 0.0f);
    // staccato: released during the rebound, it goes on falling (no jump back up to the check)
    HammerTrack s = h;
    s.upAt = at(kVizContactS + 0.005f);
    float last = 1.0f;
    for (int i = 0; i <= 400; ++i) {
        const float y = hammerHeight(s, at(0.001f * (float)i), rate);
        CHECK(y >= 0.0f && y <= 1.0f && y <= last + 1e-6f);
        last = y;
    }
    CHECK(last == 0.0f);
    // a release before the strike (the key's last one) does not count: the hammer is held
    HammerTrack o = h;
    o.upAt = 900;
    CHECK(hammerHeight(o, at(2), rate) == kVizCheck);
    // a soft strike falls back slower than a hard one, and never stays at the string
    HammerTrack soft = h, hard = h;
    soft.speed = 0.05f; hard.speed = 5.0f;
    CHECK(hammerHeight(soft, at(0.05f), rate) > hammerHeight(hard, at(0.05f), rate));
    CHECK(hammerHeight(soft, at(1), rate) == kVizCheck);
}

TEST(viz_strings_and_shapes)
{
    CHECK(stringPx(2.0f, 2.0f, 200) == 200.0f);
    CHECK(stringPx(0.05f, 2.0f, 200) > 0.05f / 2.0f * 200 * 4); // a treble string stays visible
    CHECK(stringPx(0.0f, 2.0f, 200) == 0.0f && stringPx(1.0f, 0.0f, 200) == 0.0f);
    CHECK(stringPx(0.5f, 2.0f, 200) < stringPx(1.0f, 2.0f, 200));
    // the fundamental alone: the envelope is its amplitude in the middle, zero at the ends
    float amp[4] = { 1e-3f, 0, 0, 0 }, phase[4] = {};
    CHECK(std::fabs(shapeEnvelope(amp, 4, 0.5f) - 1e-3f) < 1e-8f);
    CHECK(shapeEnvelope(amp, 4, 0.0f) < 1e-9f && shapeEnvelope(amp, 4, 1.0f) < 1e-9f);
    CHECK(std::fabs(shapeSnapshot(amp, phase, 4, 0.5f) - 1e-3f) < 1e-8f);
    // the second partial has a node in the middle
    float second[4] = { 0, 1e-3f, 0, 0 };
    CHECK(shapeEnvelope(second, 4, 0.5f) < 1e-9f && shapeEnvelope(second, 4, 0.25f) > 0.9e-3f);
    CHECK(displacementPitches(0) == 0.0f && displacementPitches(1.0f) == kVizWidest);
    CHECK(displacementPitches(1.2e-3f) < 0.5f); // a millimeter: no wider than the strings' spacing
    CHECK(displacementPitches(-1e-4f) == displacementPitches(1e-4f));
    float lo[3], hi[3];
    keyHue(0, lo); keyHue(87, hi);
    CHECK(lo[0] > lo[2] && hi[2] > hi[1]); // warm bass, cool treble
}

TEST(strip_interaction_never_leaves_a_note_on_across_random_sequences)
{
    // a deterministic pseudo-random event stream, including lost releases and presses anywhere
    uint32_t s = 12345;
    auto rnd = [&s] { s = s * 1664525u + 1013904223u; return (s >> 8) / (float)(1 << 24); };
    for (int level = 0; level <= 2; ++level) {
        StripRig r(level);
        const float bottom = r.l.top + 24 + (level >= 2 ? 300.0f : 0.0f) + (level >= 1 ? 46.0f : 0.0f);
        for (int i = 0; i < 4000; ++i) {
            const float x = rnd() * 900, y = r.l.top - 30 + rnd() * (bottom - r.l.top + 60);
            const float a = rnd();
            if (a < 0.3f) r.in.press(x, y);
            else if (a < 0.5f) r.in.release();
            else if (a < 0.95f) r.in.motion(x, y);
            else r.in.cancel();
            CHECK(r.fake.brackets >= 0 && r.fake.brackets <= 1);
            CHECK(r.fake.stuckOrDouble == 0);
            CHECK(r.in.dragging() || (r.fake.sounding.empty() && r.fake.brackets == 0));
            if (r.fake.stuckOrDouble || r.fake.brackets < 0 || r.fake.brackets > 1) break;
        }
        r.in.release();
        CHECK(r.fake.sounding.empty() && r.fake.brackets == 0);
    }
}

TEST(quick_zoom_requests_build_on_the_last_requested_size)
{
    WindowResizeTracker tracker;
    const uint32_t first = tracker.zoomed(624, 1.0, 1.25, 450, 1000);
    CHECK(first == 780);
    const uint32_t second = tracker.zoomed(624, 1.25, 1.5, 490, 1010);
    CHECK(second == 936); // use 780 pending from the host, not stale getHeight() == 624
    tracker.hostResized(first, 1020);
    CHECK(tracker.pending(1020)); // the newer zoom request is still outstanding
    tracker.hostResized(second, 1030);
    CHECK(!tracker.pending(1030));
    CHECK(tracker.zoomed(second, 1.5, 1.75, 490, 1040) == 1092);
}

TEST(clamped_or_expired_zoom_requests_rebase_from_the_host_size)
{
    WindowResizeTracker clamped;
    CHECK(clamped.zoomed(624, 1.0, 1.25, 450, 1000) == 780);
    clamped.hostResized(750, 1010); // the host's clamped size becomes the base
    CHECK(!clamped.pending(1010));
    CHECK(clamped.zoomed(750, 1.25, 1.5, 450, 1020) == 900);

    WindowResizeTracker expired;
    CHECK(expired.zoomed(624, 1.0, 1.25, 450, 1000) == 780);
    CHECK(expired.pending(1500));
    CHECK(!expired.pending(1000 + WindowResizeTracker::kExpiryMs));
    CHECK(expired.zoomed(700, 1.25, 1.5, 450, 2000) == 840); // expired: use current host size
}

TEST(ui_state_does_not_depend_on_the_numeric_locale)
{
    UiState s;
    s.diag = 2; s.view = 1; s.lo = 27.5f; s.hi = 12345.5f; s.floorDb = -87.5f;
    const std::string c = formatUiState(s);
    CHECK(c == "tab=0;zoom=100;diag=2;view=1;lo=27.5;hi=12345.5;floor=-87.5"); // the text format
    const char* old = std::setlocale(LC_NUMERIC, nullptr);
    const std::string saved = old ? old : "C";
    const char* got = std::setlocale(LC_NUMERIC, "de_DE.UTF-8");
    if (got == nullptr) got = std::setlocale(LC_NUMERIC, "de_DE.utf8");
    if (got == nullptr) {
        std::fprintf(stderr, "SKIPPED the locale part of ui_state_does_not_depend_on_the_numeric_locale: no de_DE locale\n");
    } else {
        char probe[16];
        std::snprintf(probe, sizeof probe, "%.1f", 1.5);
        CHECK(std::strcmp(probe, "1,5") == 0); // the locale really is comma-decimal
        CHECK(formatUiState(s) == c);          // written with a point anyway
        const UiState back = parseUiState(c.c_str());
        CHECK(back.lo == 27.5f && back.hi == 12345.5f && back.floorDb == -87.5f);
        CHECK(parseUiState("open=0;zoom=100;diag=2;lo=27.5;hi=16000;floor=-90").lo == 27.5f); // text saved by an older build
    }
    std::setlocale(LC_NUMERIC, saved.c_str());
    // the same text under the C locale
    const UiState back = parseUiState(c.c_str());
    CHECK(back.lo == 27.5f && back.hi == 12345.5f && back.floorDb == -87.5f && back.view == 1 && back.diag == 2);
    // a value without a fraction, exponent notation and a leading plus (all of which the old %f parser read)
    CHECK(parseUiState("open=0;zoom=100;lo=+40;hi=8e3;floor=-9e1").lo == 40.0f);
    CHECK(parseUiState("open=0;zoom=100;lo=40;hi=8e3;floor=-9e1").hi == 8000.0f);
    CHECK(parseUiState("open=0;zoom=100;lo=40;hi=8e3;floor=-9e1").floorDb == -90.0f);
}

TEST(freq_to_strip_x_is_linear_between_key_centers)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 46 });
    for (int i : { 0, 1, 2, 3, 12, 39, 40, 86 }) { // white and black keys, both orders around a black key
        const float half = 27.5f * std::pow(2.0f, (i + 0.5f) / 12.0f); // exactly +50 cents above key i
        CHECK_NEAR(freqToStripX(g, half), (keyCenterX(g, i) + keyCenterX(g, i + 1)) / 2, 0.02);
        for (float t : { 0.0f, 0.25f, 0.75f }) {
            const float hz = 27.5f * std::pow(2.0f, (i + t) / 12.0f);
            CHECK_NEAR(freqToStripX(g, hz), keyCenterX(g, i) * (1 - t) + keyCenterX(g, i + 1) * t, 0.02);
        }
    }
    // across the black key A#0 (key 1): the key centers are not evenly spaced, and the mapping is monotonic and continuous over it
    float prev = freqToStripX(g, 27.5f);
    for (int step = 1; step <= 300; ++step) {
        const float x = freqToStripX(g, 27.5f * std::pow(2.0f, (step * 0.01f) / 12.0f));
        CHECK(x >= prev);
        CHECK(x - prev < 3.0f); // no jump
        prev = x;
    }
    CHECK_NEAR(freqToStripX(g, 27.5f * std::pow(2.0f, 1.0f / 12.0f)), keyCenterX(g, 1), 0.02); // exactly on the black key
    // out of range: clamped to the first and the last key
    CHECK_NEAR(freqToStripX(g, 1.0f), keyCenterX(g, 0), 0.001);
    CHECK_NEAR(freqToStripX(g, 100000.0f), keyCenterX(g, kKeys - 1), 0.001);
}

TEST(a_black_key_is_centered_over_the_edge_of_its_white_neighbors)
{
    const StripGeometry g = computeStrip({ 10, 0, 900, 46 });
    int blacks = 0;
    for (int k = 0; k < kKeys; ++k) {
        if (!isBlackKey(k)) continue;
        ++blacks;
        CHECK_NEAR(keyCenterX(g, k), g.keys[(size_t)k + 1].r.x, 1e-4);          // the right white neighbor's left edge
        CHECK_NEAR(keyCenterX(g, k), g.keys[(size_t)k - 1].r.x + g.keys[(size_t)k - 1].r.w, 1e-3); // = the left one's right edge
    }
    CHECK(blacks == 36);
}

TEST(glissando_through_a_black_keys_footprint_ends_every_note)
{
    const StripGeometry g = computeStrip({ 0, 0, 900, 46 });
    // A0 (white), A#0 (black, key 1), B0 (white)
    std::vector<std::string> log;
    std::set<int> on;
    int bad = 0;
    StripPlayer p([&](int k, int) { log.push_back("on" + std::to_string(k)); if (!on.insert(k).second) ++bad; },
                  [&](int k) { log.push_back("off" + std::to_string(k)); if (!on.erase(k)) ++bad; });
    const float yUpper = g.area.y + g.area.h * 0.2f, yLower = g.area.y + g.area.h * 0.9f;
    const float x0 = keyCenterX(g, 0), x2 = keyCenterX(g, 2);
    auto sweep = [&](float from, float to, float y) {
        for (int i = 0; i <= 400; ++i) p.move(g, from + (to - from) * (float)i / 400.0f, y);
    };
    p.press(g, x0, yUpper);
    sweep(x0, x2, yUpper);   // up the black key's footprint: A0, A#0, B0
    sweep(x2, x0, yUpper);   // and back: B0, A#0, A0
    CHECK(on.size() == 1 && on.count(0));
    p.release();
    CHECK(on.empty() && bad == 0);
    const std::vector<std::string> expected = { "on0", "off0", "on1", "off1", "on2", "off2", "on1", "off1", "on0", "off0" };
    CHECK(log == expected);
    // below the black key (its lower end): the white keys only, A0 then B0 and back
    log.clear();
    p.press(g, x0, yLower);
    sweep(x0, x2, yLower);
    sweep(x2, x0, yLower);
    p.release();
    CHECK(on.empty() && bad == 0);
    const std::vector<std::string> white = { "on0", "off0", "on2", "off2", "on0", "off0" };
    CHECK(log == white);
    // down then up along the black key's own edge x (where it meets the right white key), at the key's bottom row
    log.clear();
    const float edge = g.keys[1].r.x + g.keys[1].r.w; // the right edge of A#0
    const float yEdge = g.keys[1].r.y + g.keys[1].r.h - 0.01f, yBelow = g.keys[1].r.y + g.keys[1].r.h + 0.5f;
    p.press(g, edge - 0.01f, yEdge); // still on the black key
    p.move(g, edge + 0.01f, yEdge);  // just beyond: B0
    p.move(g, edge - 0.01f, yBelow); // down below the black key at the same x: the white key under it, A#0's left neighbor or B0
    p.move(g, edge - 0.01f, yEdge);  // up again: the black key
    p.release();
    CHECK(on.empty() && bad == 0);
    CHECK(log.front() == "on1" && log.back() == "off1");
}

TEST(spectrogram_constructor_guards_its_size)
{
    CHECK(Spectrogram(512, 128).size() == 512 && Spectrogram(4096, 1024).size() == 4096 && Spectrogram(2, 1).size() == 2);
#ifdef NDEBUG
    // in a debug build the guard asserts instead; here the fall back to 4096 (documented in Spectro.hpp)
    for (int bad : { 0, -1, -4096, 1000, 4097, 3 }) {
        Spectrogram sp(bad, 256);
        CHECK(sp.size() == 4096 && sp.column().size() == 2049);
        const auto s = sine(1000.0f, 0.5f, 48000.0f, 8192); // and it works
        CHECK(sp.push(s.data(), 8192) >= 1);
        for (float v : sp.column()) CHECK(std::isfinite(v));
    }
    Spectrogram nohop(1024, 0); // a hop below 1 would never complete a column
    const auto s = sine(1000.0f, 0.5f, 48000.0f, 8192);
    CHECK(nohop.push(s.data(), 8192) >= 1);
#endif
}

TEST(spectrogram_column_is_finite_for_infinite_input_and_unbounded_above_for_finite_input)
{
    // an infinite sample turns the transform to NaN, which reads as the floor: the column holds no NaN/inf.
    // (The 1e6 clamp for a non-finite magnitude is not reachable through float input, so no +120 dBFS is pinned.)
    std::vector<float> bad(2 * 8192, 0.1f);
    bad[2 * 5000] = INFINITY;
    bad[2 * 6000 + 1] = -INFINITY;
    Spectrogram sp;
    sp.push(bad.data(), 8192);
    for (float v : sp.column()) CHECK(std::isfinite(v) && v >= -120.0f && v <= 120.0f);
    // finite input is not limited above: 1e30 reads about +600 dBFS (documented in Spectro.hpp)
    std::vector<float> huge(2 * 8192, 1e30f);
    Spectrogram sp2;
    sp2.push(huge.data(), 8192);
    float top = -1000.0f;
    for (float v : sp2.column()) { CHECK(std::isfinite(v)); top = std::max(top, v); }
    CHECK(top > 500.0f);
}

TEST(column_to_rows_takes_a_non_negative_floor_as_the_default)
{
    Spectrogram sp;
    const auto s = sine(1000.0f, 0.5f, 48000.0f, 8192);
    sp.push(s.data(), 8192);
    std::vector<float> ref(80), got(80);
    columnToRows(sp.column(), 48000.0f, 80, 27.5f, 16000.0f, -90.0f, ref.data());
    for (float fl : { 0.0f, 5.0f, 1e9f, INFINITY, std::nanf("") }) {
        std::fill(got.begin(), got.end(), -1.0f);
        columnToRows(sp.column(), 48000.0f, 80, 27.5f, 16000.0f, fl, got.data());
        CHECK(got == ref);
    }
    // a floor of -60 is not the default
    columnToRows(sp.column(), 48000.0f, 80, 27.5f, 16000.0f, -60.0f, got.data());
    CHECK(got != ref);
}

// ---- the waterfall ring and the panel's allocation-free text

TEST(column_ring_wraps_and_keeps_the_logical_order)
{
    ColumnRing r(5);
    CHECK(r.push() == 0 && r.push() == 1 && r.push() == 2 && r.push() == 3 && r.push() == 4);
    CHECK(r.head == 0);
    CHECK(r.push() == 0); // wraps: overwrites the oldest
    CHECK(r.head == 1);
    // logical 0 = the oldest (physical 1), width-1 = the newest (physical 0)
    CHECK(r.physicalIndex(0) == 1 && r.physicalIndex(3) == 4 && r.physicalIndex(4) == 0);
    for (int h = 0; h < 5; ++h) {
        ColumnRing q(5);
        for (int i = 0; i < h; ++i) q.push();
        // every logical x maps to a distinct physical column; consecutive logical x are consecutive physically (mod width)
        std::set<int> seen;
        for (int x = 0; x < 5; ++x) {
            seen.insert(q.physicalIndex(x));
            CHECK(q.physicalIndex(x) == (h + x) % 5);
        }
        CHECK(seen.size() == 5);
        // the two rectangles: older part [head, width), newer part [0, head), in that screen order
        const ColumnRing::Split sp = q.split();
        CHECK(sp.first == h && sp.firstCount == 5 - h && sp.secondCount == h);
        for (int x = 0; x < 5; ++x)
            CHECK(q.physicalIndex(x) == (x < sp.firstCount ? sp.first + x : x - sp.firstCount));
    }
    CHECK(ColumnRing(0).width == 1); // a width below 1 is 1
    ColumnRing e(1);
    CHECK(e.push() == 0 && e.push() == 0 && e.physicalIndex(0) == 0);
}

namespace {
/// the picture of the code before the ring: a memmove-shifted RGBA image, newest column at the right
struct MemmovePicture {
    int w, h;
    std::vector<uint8_t> img;
    MemmovePicture(int w_, int h_) : w(w_), h(h_), img((size_t)w_ * h_ * 4)
    {
        for (size_t i = 0; i < img.size(); i += 4) put(0.0f, &img[i]);
    }
    static void put(float v, uint8_t* px)
    {
        px[0] = (uint8_t)(255 * std::max(0.0f, std::min(1.0f, v * 1.5f - 0.5f)));
        px[1] = (uint8_t)(255 * std::min(1.0f, v * 1.6f));
        px[2] = (uint8_t)(255 * std::min(1.0f, 0.2f + v * 0.9f));
        px[3] = 255;
    }
    void add(const std::vector<float>& bins, float rate, float lo, float hi, float fl)
    {
        std::vector<float> rows((size_t)h);
        columnToRows(bins, rate, h, lo, hi, fl, rows.data());
        for (int y = 0; y < h; ++y) {
            uint8_t* row = &img[(size_t)y * w * 4];
            std::memmove(row, row + 4, (size_t)(w - 1) * 4);
            put(rows[(size_t)y], row + (w - 1) * 4);
        }
    }
};
/// the waterfall's picture in logical order (oldest column first), what the two rectangles show
std::vector<uint8_t> logicalPicture(const Waterfall& wf)
{
    std::vector<uint8_t> out((size_t)wf.columns() * wf.rows() * 4);
    for (int x = 0; x < wf.columns(); ++x)
        for (int y = 0; y < wf.rows(); ++y)
            std::memcpy(&out[((size_t)y * wf.columns() + x) * 4], wf.rgba() + ((size_t)y * wf.columns() + wf.ring().physicalIndex(x)) * 4, 4);
    return out;
}
std::vector<float> randomBins(uint32_t& rng, size_t n)
{
    std::vector<float> b(n);
    for (float& v : b) { rng = rng * 1664525u + 1013904223u; v = -120.0f + (float)(rng >> 8) / (float)(1u << 24) * 130.0f; }
    return b;
}
}

TEST(waterfall_ring_shows_the_same_picture_as_the_memmoved_image)
{
    const int W = 24, H = 16;
    Waterfall wf(W, H);
    MemmovePicture ref(W, H);
    CHECK(logicalPicture(wf) == ref.img); // the empty picture
    uint32_t rng = 7;
    for (int i = 1; i <= 3 * W + 5; ++i) { // before the first wrap, at it and after several wraps
        const std::vector<float> bins = randomBins(rng, 129);
        const float rate = i % 7 == 0 ? 44100.0f : 48000.0f;
        wf.add(bins, rate, 27.5f, 16000.0f, -90.0f);
        ref.add(bins, rate, 27.5f, 16000.0f, -90.0f);
        CHECK(logicalPicture(wf) == ref.img);
    }
    CHECK(wf.retained() == W);
}

TEST(waterfall_remap_rerenders_the_retained_columns_instead_of_blanking)
{
    const int W = 24, H = 16;
    Waterfall wf(W, H);
    uint32_t rng = 99;
    std::vector<std::vector<float>> cols;
    for (int i = 0; i < 40; ++i) { cols.push_back(randomBins(rng, 129)); wf.add(cols.back(), 48000.0f, 27.5f, 16000.0f, -90.0f); }
    const std::vector<uint8_t> before = logicalPicture(wf);
    wf.remap(55.0f, 2000.0f, -60.0f); // the user picks another range and floor (frozen or not)
    // the reference: the last W columns mapped with the new settings
    MemmovePicture ref(W, H);
    for (size_t i = cols.size() - W; i < cols.size(); ++i) ref.add(cols[i], 48000.0f, 55.0f, 2000.0f, -60.0f);
    CHECK(logicalPicture(wf) == ref.img);
    CHECK(logicalPicture(wf) != before);
    // not blank: it still holds bright pixels
    MemmovePicture blank(W, H);
    CHECK(logicalPicture(wf) != blank.img);
    // fewer columns than the width: the newest ones are re-rendered, the rest stay background
    Waterfall few(W, H);
    MemmovePicture ref2(W, H);
    for (int i = 0; i < 5; ++i) { few.add(cols[(size_t)i], 48000.0f, 27.5f, 16000.0f, -90.0f); ref2.add(cols[(size_t)i], 48000.0f, 200.0f, 8000.0f, -70.0f); }
    few.remap(200.0f, 8000.0f, -70.0f);
    CHECK(few.retained() == 5);
    CHECK(logicalPicture(few) == ref2.img);
    // a remap before any column is the empty picture; clear() frees the retained data
    Waterfall none(W, H);
    none.remap(55.0f, 2000.0f, -60.0f);
    CHECK(logicalPicture(none) == MemmovePicture(W, H).img);
    CHECK(wf.retainedBytes() >= (size_t)W * 129 * sizeof(float));
    wf.clear();
    CHECK(wf.retainedBytes() == 0 && wf.retained() == 0);
    CHECK(logicalPicture(wf) == MemmovePicture(W, H).img); // no stale picture after a clear (reopen)
    CHECK(wf.ring().head == 0);
}

TEST(panel_text_helpers_need_no_string_and_match_the_old_text)
{
    char b[48];
    for (int k = -3; k < 92; ++k) { keyNameTo(k, b, sizeof b); CHECK(keyName(k) == b); }
    keyNameTo(39, b, sizeof b); CHECK(std::strcmp(b, "C4") == 0);
    keyNameTo(39, b, 3); CHECK(std::strcmp(b, "C4") == 0);
    keyNameTo(39, b, 2); CHECK(std::strcmp(b, "C") == 0); // truncated, still terminated
    formatHz(27.5f, b, sizeof b); CHECK(std::strcmp(b, "27.5") == 0);
    formatHz(16000.0f, b, sizeof b); CHECK(std::strcmp(b, "16k") == 0);
    formatHz(1500.0f, b, sizeof b); CHECK(std::strcmp(b, "1.5k") == 0);
    formatHz(200.0f, b, sizeof b); CHECK(std::strcmp(b, "200") == 0);
    formatStatus(b, sizeof b, false, 5, true, 3, false); CHECK(std::strcmp(b, "") == 0);
    formatStatus(b, sizeof b, true, 7, true, 3, false); CHECK(std::strcmp(b, "stalled") == 0); // stalled beats a drop notice
    formatStatus(b, sizeof b, true, 0, true, 3, false); CHECK(std::strcmp(b, "stalled") == 0);
    formatStatus(b, sizeof b, true, 0, false, 1, false); CHECK(std::strcmp(b, "1 voice") == 0);
    formatStatus(b, sizeof b, true, 0, false, 0, false); CHECK(std::strcmp(b, "0 voices") == 0);
    formatStatus(b, sizeof b, true, 0, false, 24, true); CHECK(std::strcmp(b, "24 voices (frozen)") == 0);
    formatStatus(b, sizeof b, true, 4000000000u, false, 0, false); CHECK(std::strcmp(b, "0 voices, 4000000000 dropped") == 0);
}

TEST(status_shows_drops_only_while_recent_and_keeps_the_voices)
{
    char b[48];
    formatStatus(b, sizeof b, true, 113, false, 12, false); CHECK(std::strcmp(b, "12 voices, 113 dropped") == 0);
    formatStatus(b, sizeof b, true, 5, false, 1, true); CHECK(std::strcmp(b, "1 voice (frozen), 5 dropped") == 0);
    formatStatus(b, sizeof b, true, 113, true, 12, false); CHECK(std::strcmp(b, "stalled") == 0);
    formatStatus(b, sizeof b, false, 113, true, 12, false); CHECK(std::strcmp(b, "") == 0);
    formatStatus(b, sizeof b, false, 113, false, 12, false); CHECK(std::strcmp(b, "") == 0);
    formatStatus(b, 8, true, 113, false, 12, true); CHECK(std::strlen(b) == 7); // truncated, still terminated
    // recovery: the counter stops growing, the notice goes after the window
    DropWatch w;
    w = dropWatchUpdate(w, 0, 1000);
    CHECK(!dropsRecent(w, 1000));
    w = dropWatchUpdate(w, 3, 2000);
    CHECK(dropsRecent(w, 2000) && w.total == 3);
    w = dropWatchUpdate(w, 3, 3500);
    CHECK(dropsRecent(w, 3500));
    w = dropWatchUpdate(w, 3, 4000);
    CHECK(dropsRecent(w, 4000));  // exactly the window
    w = dropWatchUpdate(w, 3, 4001);
    CHECK(!dropsRecent(w, 4001)); // gone, though the counter is still 3
    formatStatus(b, sizeof b, true, dropsRecent(w, 4001) ? w.total : 0, false, 12, false);
    CHECK(std::strcmp(b, "12 voices") == 0);
    w = dropWatchUpdate(w, 4, 9000);
    CHECK(dropsRecent(w, 9000) && w.total == 4); // growth again: the notice returns
}

TEST(drop_watch_rebases_on_a_lower_counter_and_survives_wraparound)
{
    DropWatch w;
    w = dropWatchUpdate(w, 50, 1000);       // first sight of a counter at 50: that is growth from 0
    CHECK(dropsRecent(w, 1000));
    w = dropWatchUpdate(w, 7, 9000);        // went down: re-initialized region
    CHECK(!dropsRecent(w, 9000) && w.last == 7 && w.total == 50);
    w = dropWatchUpdate(w, 9, 9100);        // growth counts from the new base
    CHECK(dropsRecent(w, 9100) && w.total == 52);
    DropWatch x;
    x.last = 0xFFFFFFFEu;
    x = dropWatchUpdate(x, 2, 500);         // wrapped past 2^32: +4
    CHECK(dropsRecent(x, 500) && x.total == 4 && x.last == 2);
    DropWatch y;                            // a reopened panel bases on the current counter: no notice
    y.last = 1234;
    y = dropWatchUpdate(y, 1234, 100);
    CHECK(!dropsRecent(y, 100));
    CHECK(!dropsRecent(y, 100000));
}

TEST(hover_cents_wiring_uses_the_voices_key_and_the_concert_pitch)
{
    int n = 0;
    // A4 (key 48) at 440: its own fundamental is 0 cents from harmonic 1
    CHECK_NEAR(hoverCents(440.0f, 48, 440.0f, n), 0.0, 0.01);
    CHECK(n == 1);
    // at 415 the nominal A4 is 415 Hz: a 440-Hz partial is 1200·log2(440/415) cents above harmonic 1
    CHECK_NEAR(hoverCents(440.0f, 48, 415.0f, n), 1200.0 * std::log2(440.0 / 415.0), 0.01);
    CHECK(n == 1);
    CHECK_NEAR(hoverCents(415.0f, 48, 415.0f, n), 0.0, 0.01);
    // key 12 (A1 = 55 Hz): a partial a little above 2·f0 is harmonic 2, +cents
    CHECK_NEAR(hoverCents(110.3f, 12, 440.0f, n), 1200.0 * std::log2(110.3 / 110.0), 0.01);
    CHECK(n == 2);
    CHECK(hoverCents(165.0f, 12, 440.0f, n) < 1.0f && n == 3);
    // the same partial against another pitch: the wiring multiplies f0 by pitch/440 and nothing else
    CHECK_NEAR(hoverCents(110.3f, 12, 415.0f, n), centsFromHarmonic(110.3f, keyFrequency(12) * 415.0f / 440.0f, n), 1e-5);
    // harpsichord 4' (a voice that sounds an octave up): its fundamental is harmonic 2 of the key's nominal
    CHECK_NEAR(hoverCents(2.0f * keyFrequency(30), 30, 440.0f, n), 0.0, 0.01);
    CHECK(n == 2);
    // guards: no frequency, NaN, a nonsense pitch -> 0 cents and n = 1; an out-of-range key clamps to the keyboard
    n = 7; CHECK(hoverCents(0.0f, 30, 440.0f, n) == 0.0f && n == 1);
    n = 7; CHECK(hoverCents(std::nanf(""), 30, 440.0f, n) == 0.0f && n == 1);
    n = 7; CHECK(hoverCents(440.0f, 30, 0.0f, n) == 0.0f && n == 1);
    n = 7; CHECK(hoverCents(440.0f, 30, std::nanf(""), n) == 0.0f && n == 1);
    int n2 = 0;
    CHECK(hoverCents(4000.0f, 200, 440.0f, n) == hoverCents(4000.0f, 87, 440.0f, n2) && n == n2);
    CHECK(hoverCents(30.0f, -9, 440.0f, n) == hoverCents(30.0f, 0, 440.0f, n2) && n == n2);
}

TEST(felt_for_build_follows_original)
{
    const PeFelt a = feltForBuild("s:21=6", false, 3.0f);
    CHECK(a.curve[0].n == 1 && a.curve[0].key[0] == 21 && a.hardness_db == 3.0f);
    const PeFelt o = feltForBuild("s:21=6", true, 3.0f); // Original: no curves, the knob stays
    CHECK(o.curve[0].n == 0 && o.hardness_db == 3.0f);
    CHECK(feltForBuild("garbage", false, 0.0f).curve[0].n == 0); // unreadable: shipped
    CHECK(liveHardness(2.0f, true) == 1.0f && liveHardness(2.0f, false) == 2.0f);
}

TEST(a_hardness_moved_during_a_build_builds_again)
{
    // the first build after a harpsichord → piano switch: the hardness moved while it ran (no request
    // then, the last engine played its hardness live); a hammer piano takes it through another build
    CHECK(hardnessMovedDuringBuild(true, 0.0f, 3.0f));
    CHECK(!hardnessMovedDuringBuild(true, 3.0f, 3.0f));
    CHECK(!hardnessMovedDuringBuild(false, 0.0f, 3.0f)); // elsewhere it plays live
}

TEST(felt_state_survives_restore_in_either_order)
{
    // a host restores "felt" and "instrument" in its own order: both keep the felt
    FeltState a;
    a.setFelt("s:21=6");
    a.setInstrument("grand_f");
    CHECK(a.text() == "s:21=6");
    FeltState b;
    b.setInstrument("grand_f");
    b.setFelt("s:21=6");
    CHECK(b.text() == "s:21=6");
    // MIDI switching the instrument: the new one's shipped felt
    b.midiSwitch();
    CHECK(b.text().empty());
}

TEST(felt_original_flips_on_each_change)
{
    // the toggle's input flips at each click, as Panic's: a value a host restores is the baseline (off),
    // every change after it turns Original on or off; the output says which plays, for the UI to show
    OriginalToggle t;
    t.activate(1.0f);
    CHECK(!t.set(1.0f));
    CHECK(t.set(0.0f)); // the UI's first click, whatever the host restored
    CHECK(t.set(0.0f)); // no change: still on
    CHECK(!t.set(1.0f));
    OriginalToggle u; // constructed: off
    CHECK(!u.set(0.0f) && u.set(1.0f));
    CHECK(diagIsOutput(kDiagFeltOriginalOut) && !diagIsOutput(kDiagFeltOriginalIn));
    CHECK(std::string(diagInfo(kDiagFeltOriginalOut).symbol) == "diag_felt_original");
}

namespace {
/// every key struck by the same felt (2 N at 0.5 mm, exponent 2.2), no core
PeFeltKeys flatFelt()
{
    PeFeltKeys k {};
    for (int i = 0; i < 88; ++i) {
        k.hammer[i] = 1;
        k.f05[i] = 2.0f;
        k.p[i] = 2.2f;
        k.core_depth_mm[i] = 1.0f;
    }
    return k;
}
const Rect kFeltGraph { 100, 100, 870, 300 }; // 10 px per key
} // namespace

TEST(felt_editor_adds_moves_and_removes_points)
{
    FeltEditor e;
    e.setShipped(flatFelt());
    e.select(1); // the exponent
    float x, y;
    e.toGraph(kFeltGraph, 1, 60.3f, 2.7f, x, y);
    const int i = e.add(kFeltGraph, x, y);
    CHECK(i == 0 && e.felt().curve[1].n == 1 && e.felt().curve[1].key[0] == 60); // a whole key
    CHECK(std::fabs(e.felt().curve[1].bend[0] - 0.5f) < 0.02f); // the bend: shown 2.7 − shipped 2.2
    CHECK(std::fabs(e.shown(1, 60, true) - 2.7f) < 0.02f && e.shown(1, 60, false) == 2.2f);
    CHECK(e.add(kFeltGraph, x, y) == -1); // on a point's key
    e.toGraph(kFeltGraph, 1, 40, 2.2f, x, y);
    CHECK(e.add(kFeltGraph, x, y) == 0); // before it
    e.toGraph(kFeltGraph, 1, 80, 2.2f, x, y);
    CHECK(e.add(kFeltGraph, x, y) == 2);
    // a drag holds between its neighbors' keys
    e.toGraph(kFeltGraph, 1, 100, 3.0f, x, y);
    e.move(1, kFeltGraph, x, y);
    CHECK(e.felt().curve[1].key[1] == 79);
    CHECK(e.pointAt(kFeltGraph, x, y) == -1); // the pointer ran past where the point stopped
    e.toGraph(kFeltGraph, 1, 79, e.shown(1, 79, true), x, y);
    CHECK(e.pointAt(kFeltGraph, x, y) == 1);
    for (int k = 0; k < 5; ++k) {
        e.toGraph(kFeltGraph, 1, 21 + k, 2.2f, x, y);
        e.add(kFeltGraph, x, y);
    }
    CHECK(e.felt().curve[1].n == 8);
    e.toGraph(kFeltGraph, 1, 100, 2.2f, x, y);
    CHECK(e.add(kFeltGraph, x, y) == -1); // a 9th
    CHECK(e.remove(0) && e.felt().curve[1].n == 7 && !e.remove(7));
    // the other quantities untouched
    CHECK(e.felt().curve[0].n == 0 && e.felt().curve[2].n == 0 && e.felt().curve[3].n == 0);
}

TEST(felt_editor_ranges_refit_on_drop_only)
{
    FeltEditor e;
    e.setShipped(flatFelt());
    float lo, hi;
    e.range(2, lo, hi);
    CHECK(lo == 0 && hi == 12); // the core's force: its default
    e.select(2);
    float x, y;
    e.toGraph(kFeltGraph, 2, 50, 5.0f, x, y);
    const int i = e.add(kFeltGraph, x, y);
    e.toGraph(kFeltGraph, 2, 50, 30.0f, x, y); // dragged above the graph
    e.move(i, kFeltGraph, x, y);
    CHECK(std::fabs(e.shown(2, 50, true) - 30.0f) < 0.05f);
    e.range(2, lo, hi);
    CHECK(hi == 12); // not while dragging
    CHECK(e.dropped());
    e.range(2, lo, hi);
    CHECK(std::fabs(hi - 32.0f) < 0.05f);
    CHECK(!e.dropped()); // nothing more to refit
    e.range(0, lo, hi);
    CHECK(lo == -12 && hi == 12);
    e.range(1, lo, hi);
    CHECK(lo == 0 && hi == 4);
    e.range(3, lo, hi);
    CHECK(lo == 0 && hi == 3);
}

TEST(felt_editor_stiffness_is_db_re_shipped)
{
    FeltEditor e;
    PeFeltKeys k = flatFelt();
    k.f05[30] = 7.0f; // the key-to-key detail stays under a bend
    e.setShipped(k);
    for (int key = 21; key <= 108; ++key)
        CHECK(e.shown(0, key, false) == 0 && e.shown(0, key, true) == 0);
    e.select(0);
    float x, y;
    e.toGraph(kFeltGraph, 0, 21, 6.0f, x, y);
    e.add(kFeltGraph, x, y);
    CHECK(std::fabs(e.shown(0, 51, true) - 6.0f) < 0.05f); // held beyond the point
    CHECK(std::fabs(e.shown(0, 108, true) - 6.0f) < 0.05f);
}

TEST(felt_editor_reset_and_text)
{
    FeltEditor e;
    e.setShipped(flatFelt());
    PeFelt f {};
    CHECK(pe_felt_parse("s:21=6,48=0;p:;c:60=3;d:", &f));
    e.set(f);
    CHECK(e.text() == "s:21=6,48=0;p:;c:60=3;d:");
    e.reset();
    CHECK(e.felt().curve[0].n == 0 && e.felt().curve[2].n == 0);
    CHECK(e.shown(2, 60, true) == 0);
}

TEST(felt_editor_a_point_stops_at_the_felt_s_limits)
{
    // dragged past a limit (engine felt::LIMITS: exponent 1 … 5, stiffness −36 … +24 dB), a point
    // stops there: its bend is what plays, so dragging back moves it at once
    FeltEditor e;
    e.setShipped(flatFelt());
    e.select(1);
    float x, y;
    e.toGraph(kFeltGraph, 1, 60, 2.7f, x, y);
    const int i = e.add(kFeltGraph, x, y);
    e.toGraph(kFeltGraph, 1, 60, 9.0f, x, y);
    e.move(i, kFeltGraph, x, y);
    CHECK(std::fabs(e.felt().curve[1].bend[0] - (5.0f - 2.2f)) < 1e-4f);
    e.toGraph(kFeltGraph, 1, 40, -3.0f, x, y);
    const int j = e.add(kFeltGraph, x, y);
    CHECK(j == 0 && std::fabs(e.felt().curve[1].bend[0] - (1.0f - 2.2f)) < 1e-4f);
    e.select(0);
    e.toGraph(kFeltGraph, 0, 70, 40.0f, x, y);
    e.add(kFeltGraph, x, y);
    CHECK(std::fabs(e.felt().curve[0].bend[0] - 24.0f) < 1e-3f);
}

TEST(felt_editor_text_has_room_for_every_point)
{
    // 32 points with long bends write more than 512 bytes: the text is whole, not empty
    PeFelt f {};
    for (int q = 0; q < 4; ++q) {
        f.curve[q].n = 8;
        for (int i = 0; i < 8; ++i) {
            f.curve[q].key[i] = (uint8_t)(21 + 10 * i);
            f.curve[q].bend[i] = -0.000123456789f * (float)(i + 1);
        }
    }
    FeltEditor e;
    e.setShipped(flatFelt());
    e.set(f);
    const std::string t = e.text();
    CHECK(t.size() > 512);
    PeFelt back {};
    CHECK(pe_felt_parse(t.c_str(), &back));
    for (int q = 0; q < 4; ++q)
        CHECK(back.curve[q].n == 8 && back.curve[q].bend[7] == f.curve[q].bend[7]);
}

TEST(felt_editor_applies_only_with_hammers)
{
    FeltEditor e;
    CHECK(!e.applies()); // nothing shipped yet
    e.setShipped(flatFelt());
    CHECK(e.applies());
    e.setShipped(PeFeltKeys {});
    CHECK(!e.applies());
}

TEST(felt_tab_layout)
{
    CHECK(std::string(tabName(kTabFelt)) == "Felt");
    for (auto wh : { std::pair<float, float> { kDefaultWidth, kDefaultHeight }, { kMinWidth, kMinHeight } }) {
        const Layout l = computeLayout(wh.first, wh.second, kTabFelt, ctxOf(0, 1, 0)); // a grand
        CHECK(l.feltNote.w > 0 && l.emptyNote.w == 0); // its line for "measuring…"
        CHECK(l.feltGraph.w > 0 && l.feltGraph.h >= 160);
        std::vector<Rect> all { l.feltGraph, l.feltOriginal, l.feltReset, l.feltNote };
        for (const Rect& c : l.feltChips) {
            CHECK(c.w > 0 && c.y == l.feltChips[0].y && c.y + c.h <= l.feltGraph.y); // one row above the graph
            all.push_back(c);
        }
        CHECK(l.feltOriginal.y + l.feltOriginal.h <= l.feltGraph.y && l.feltReset.y + l.feltReset.h <= l.feltGraph.y);
        for (const Rect& r : l.feltSmall) {
            CHECK(r.w > 0 && r.h > 0 && r.y >= l.feltGraph.y + l.feltGraph.h);
            all.push_back(r);
        }
        for (size_t i = 0; i < all.size(); ++i) {
            const Rect& r = all[i];
            CHECK(r.x >= l.content.x && r.x + r.w <= l.content.x + l.content.w + 0.5f && r.y >= l.content.y);
            for (size_t j = i + 1; j < all.size(); ++j)
                CHECK(!r.overlaps(all[j]));
        }
        CHECK(l.contentHeight >= l.feltSmall[2].y + l.feltSmall[2].h - l.content.y);
    }
    // a wide window: the chips' row also holds Original and Reset (at the default width the panel beside
    // the drawing puts them on the next row)
    const Layout w = computeLayout(1600, kDefaultHeight, kTabFelt, ctxOf(0, 1, 0));
    CHECK(w.feltOriginal.y == w.feltChips[0].y && w.feltReset.y == w.feltChips[0].y);
    // other tabs carry none of it
    CHECK(computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(0, 1, 0)).feltGraph.w == 0);
}

TEST(felt_tab_on_a_harp_shows_the_note_only)
{
    const Layout l = computeLayout(kDefaultWidth, kDefaultHeight, kTabFelt, ctxOf(kHarp, 1, 0));
    CHECK(l.feltGraph.w == 0 && l.feltNote.w > 0 && l.feltOriginal.w == 0 && l.feltReset.w == 0);
}

TEST(ten_tabs_fit_two_rows)
{
    CHECK((int)kTabCount == 10);
    for (float w : { kDefaultWidth, kMinWidth }) {
        const Layout l = computeLayout(w, kDefaultHeight, kTabSound, ctxOf(0, 1, 0));
        float lo = l.tabs[0].rect.y, hi = lo;
        for (const TabButton& t : l.tabs)
            hi = std::max(hi, t.rect.y);
        CHECK(hi - lo <= kTabH + 2 + 0.5f);
    }
}

TEST(shipped_felt_is_fetched_off_the_ui_thread_the_newest_wins)
{
    const std::string grand = std::string(PE_INSTRUMENTS_DIR) + "/grand_f", other = std::string(PE_INSTRUMENTS_DIR) + "/grand_b";
    FeltFetch f;
    PeFeltKeys k {};
    CHECK(!f.poll(k)); // nothing asked yet
    f.request({ other }, { 1.0f });
    f.request({ grand }, { 1.0f }); // before the first is done: it replaces it
    bool got = false;
    for (int i = 0; i < 400 && !got; ++i) {
        got = f.poll(k);
        if (!got)
            std::this_thread::sleep_for(std::chrono::milliseconds(25));
    }
    CHECK(got);
    // the last one asked for: Grand F, its bass core (Grand B has none)
    CHECK(k.hammer[0] == 1 && k.core[0] == 1 && k.core[60 - 21] == 0);
    CHECK(!f.poll(k)); // taken once
}

TEST(a_shipped_felt_that_does_not_load_says_so)
{
    // a failed read ends the wait ("reading the felt…" stays up for good otherwise)
    FeltFetch f;
    PeFeltKeys k {};
    k.hammer[0] = 7;
    f.request({ std::string(PE_INSTRUMENTS_DIR) + "/no-such-instrument" }, { 1.0f });
    bool got = false, loaded = true;
    for (int i = 0; i < 400 && !got; ++i) {
        got = f.poll(k, &loaded);
        if (!got)
            std::this_thread::sleep_for(std::chrono::milliseconds(25));
    }
    CHECK(got && !loaded);
    CHECK(k.hammer[0] == 7); // untouched
}

TEST(builds_leave_a_core_to_the_audio_thread)
{
    // a build on every logical CPU slowed the audio thread's blocks 2–3× at their tail: the
    // physical cores but one (the hyperthreads' siblings share a core's caches)
    CHECK(buildThreadsFor(8) == 7 && buildThreadsFor(2) == 1 && buildThreadsFor(1) == 1 && buildThreadsFor(0) == 1);
    const unsigned cores = physicalCores();
    CHECK(cores >= 1 && cores <= std::max(1u, std::thread::hardware_concurrency()));
}

TEST(the_build_worker_and_its_threads_run_at_a_lower_priority)
{
#ifdef __linux__
    // the engine's build threads (a felt measured fills every core for 3–7 s) inherit the worker's
    // (at least kWorkerNice; a suite already run niced keeps its own)
    const int before = workerNice();
    int worker = 0, child = 0;
    std::thread([&] {
        lowerWorkerPriority();
        worker = workerNice();
        std::thread([&] { child = workerNice(); }).join();
    }).join();
    CHECK(worker == std::max(before, kWorkerNice) && child == worker);
    CHECK(workerNice() == before); // the calling thread's own stays
#endif
}

int main() { return check::runAll(); }

TEST(sustain_latch_holds_the_pedal_down_and_survives_resets)
{
    PedalMixer m;
    m.setUi(0.0f);
    PedalMixer::Call c = m.setLatch(true);
    CHECK(c.call && c.depth == 1.0f);
    CHECK(!m.setCc(0).call && !m.setCc(127).call && !m.setCc(0).call); // CC64 under the latch: no change
    CHECK(!m.setUi(0.4f).call && m.applied() == 1.0f);
    // the engine resets its pedal at a panic and at activation: the latch is told again
    c = m.panic();
    CHECK(c.call && c.depth == 1.0f && m.applied() == 1.0f);
    c = m.activate(0.0f);
    CHECK(c.call && c.depth == 1.0f);
    CHECK(m.replayForNewEngine().call && m.replayForNewEngine().depth == 1.0f);
    // off: back to what CC64 and the UI hold
    m.setCc(64);
    c = m.setLatch(false);
    CHECK(c.call && std::fabs(c.depth - 64.0f / 127.0f) < 1e-7f);
    CHECK(!m.setLatch(false).call);
    // without the latch, panic and activation still call nothing
    PedalMixer n;
    CHECK(!n.panic().call && !n.activate(0.5f).call);
}

TEST(sustain_latch_in_the_header_for_every_instrument)
{
    CHECK(paramInfo(kSustainLatch).boolean && paramInfo(kSustainLatch).automatable && paramInfo(kSustainLatch).def == 0.0f);
    CHECK(paramInfo(kSustainLatch).group == kGroupInstrument);
    for (uint32_t p = 0; p < kProfileCount; ++p) {
        // in the header wherever it acts; hidden on the bars without dampers (no pedal to latch)
        const bool acts = !(profileMarimba(p) || profileXylophone(p) || profileBells(p) || profileCrotales(p) || profileKalimba(p)
            || profileBalafon(p));
        CHECK(paramActive(kSustainLatch, p, 0, 0) == acts);
        for (float w : { 700.0f, kBreakpoint, 1200.0f }) {
            int found = 0;
            for (const Row& r : computeLayout(w, kDefaultHeight, kTabSound, ctxOf(p, 0, 0)).rows)
                if (r.param == kSustainLatch) {
                    ++found;
                    CHECK(r.kind == Kind::Toggle && r.rect.w >= 36.0f && !r.scrolls);
                }
            CHECK(found == (acts ? 1 : 0));
        }
    }
}

TEST(preparations_per_instrument_match_the_engine_test)
{
    // engine/tests/no_effect.rs PREPARATIONS
    using V = std::vector<uint32_t>;
    const auto list = [](uint32_t p) {
        const uint32_t* v = nullptr;
        const uint32_t n = profilePreparations(p, &v);
        return V(v, v + n);
    };
    for (uint32_t p = 0; p < kProfileCount; ++p) {
        const V want = p == kCro || p == kKal || p == kMbA || p == kMbB || p == kNyu ? V { 3, 4, 5 }
            : profileBars(p) && p != kVib && p != kChi && p != kHch ? V { 3, 4 }
            : profilePlucked(p) || p == kVib || p == kCla || p == kChi || p == kHch ? V { 2, 3, 4, 5 }
                                             : V { 1, 2, 3, 4, 5 };
        CHECK(list(p) == want);
        // every instrument has the bolt: PianoPlugin's bolt request never needs the instrument
        CHECK(std::find(want.begin(), want.end(), 4u) != want.end());
        CHECK(effectivePrep(p, 0) == 0);
        for (uint32_t v = 1; v < 6; ++v)
            CHECK(effectivePrep(p, v) == (std::find(want.begin(), want.end(), v) != want.end() ? v : 0u));
        CHECK(effectivePrep(p, 99) == 0);
    }
}

TEST(visibility_follows_the_effective_preparation)
{
    // a preparation the instrument has not: its settings are hidden, the menu stays
    CHECK(!paramVisible(kPluckPosition, ctxOf(kHarpA, 1, 1)) && paramVisible(kPrepMode, ctxOf(kHarpA, 1, 1)));
    CHECK(paramVisible(kPluckPosition, ctxOf(0, 1, 1)) && paramVisible(kPluckBrightness, ctxOf(0, 1, 1)));
    CHECK(!paramVisible(kMuteStrength, ctxOf(kMar, 1, 2)) && paramVisible(kMuteStrength, ctxOf(kVib, 1, 2)));
    CHECK(!paramVisible(kPaperBuzz, ctxOf(kBel, 1, 5)) && paramVisible(kPaperBuzz, ctxOf(kCro, 1, 5)));
    CHECK(paramVisible(kBoltMass, ctxOf(kXyl, 1, 4)) && paramVisible(kHarmonicNumber, ctxOf(kMar, 1, 3)));
    // the temperament key stays at every temperament (no effect at Equal: its tooltip says so)
    for (uint32_t p = 0; p < kProfileCount; ++p)
        CHECK(paramVisible(kTemperamentKey, ctxOf(p, 1, 0)));
    // the rest as paramActive
    for (uint32_t p = 0; p < kProfileCount; ++p)
        for (uint32_t mic = 0; mic < 6; ++mic)
            for (uint32_t i = 0; i < kPrepMode; ++i)
                CHECK(paramVisible(i, ctxOf(p, mic, 0)) == paramActive(i, p, mic, 0));
}

TEST(every_parameter_explains_itself)
{
    for (uint32_t i = 0; i < kParamCount; ++i) {
        const char* d = paramInfo(i).description;
        const bool ok = d != nullptr && std::strlen(d) >= 20;
        CHECK(ok);
        if (!ok)
            std::printf("  no description: %s\n", paramInfo(i).name);
    }
    const char* key = paramInfo(kTemperamentKey).description;
    CHECK(key != nullptr && std::strstr(key, "No effect with Equal temperament") != nullptr);
}

TEST(tooltip_shows_after_resting_and_hides_on_a_press)
{
    TooltipTimer t;
    CHECK(t.visible(0, false) == -1);
    t.hover(3, 1000);
    CHECK(t.visible(1000 + kTooltipDelayMs - 1, false) == -1);
    CHECK(t.visible(1000 + kTooltipDelayMs, false) == 3);
    CHECK(t.visible(1000 + kTooltipDelayMs, true) == -1); // dragging, typing, a list open
    t.hover(3, 5000);                                      // the same row: the delay keeps running
    CHECK(t.visible(5000, false) == 3);
    t.dismiss();                                           // a press or a scroll
    CHECK(t.visible(9000, false) == -1);
    t.hover(3, 9000);                                      // still that row: stays hidden
    CHECK(t.visible(20000, false) == -1);
    t.hover(4, 20000);                                     // another row: a new delay
    CHECK(t.visible(20000, false) == -1 && t.visible(20000 + kTooltipDelayMs, false) == 4);
    t.hover(-1, 30000);
    CHECK(t.visible(40000, false) == -1);
}

TEST(tooltip_stays_inside_the_view)
{
    const Rect row { 500, 100, 380, 22 };
    Rect r = tooltipRect(row, 600, 280, 90, 900, 600);
    CHECK(r.x == 600 && r.y == row.y + row.h + 4 && r.w == 280 && r.h == 90); // below, from the label's end
    r = tooltipRect({ 500, 560, 380, 22 }, 620, 280, 90, 900, 600);
    CHECK(r.y + r.h <= 560 && r.y >= 0);                                      // no room below: above
    r = tooltipRect(row, 800, 280, 90, 900, 600);
    CHECK(r.x + r.w <= 900 && r.x >= 0);                                      // no room right: moved left
    r = tooltipRect(row, 10, 280, 90, 200, 60);                               // a view smaller than the tip
    CHECK(r.x >= 0 && r.y >= 0 && r.w <= 200 && r.w > 0);
}

TEST(tooltip_texts)
{
    CHECK(tooltipTitle(kHammerHardness, kMar) == "Mallet hardness" && tooltipTitle(kHammerHardness, 0) == "Hammer hardness");
    CHECK(tooltipTitle(kWearHammers, 0) == "Wear: hammers" && tooltipTitle(kRoom, 0) == "Room");
    CHECK(std::string(tooltipGestures(Kind::Slider)).find("Shift") != std::string::npos);
    CHECK(std::string(tooltipGestures(Kind::Slider)).find("double-click") != std::string::npos);
    CHECK(std::string(tooltipGestures(Kind::Dropdown)).find("Shift") == std::string::npos);
    CHECK(std::string(tooltipGestures(Kind::Toggle)).find("Shift") == std::string::npos);
}

static bool has(const std::vector<uint32_t>& v, uint32_t x) { return std::find(v.begin(), v.end(), x) != v.end(); }

TEST(every_parameter_has_one_place)
{
    std::vector<int> n(kParamCount, 0);
    // the Instrument and Preset rows (kInstrumentRow, kMicPresetRow, kPresetRow) are no parameters
    for (uint32_t i : headerParams())
        if (i < kParamCount) ++n[i];
    for (uint32_t i : drawingParams())
        if (i < kParamCount) ++n[i];
    for (uint32_t i : morphParams()) ++n[i];
    for (uint32_t t = 0; t < kTabCount; ++t)
        for (const TabGroup& g : tabGroups((Tab)t))
            for (uint32_t i : g.params)
                if (i < kParamCount) ++n[i];
    for (uint32_t i = 0; i < kParamCount; ++i)
        CHECK(n[i] == 1);
    CHECK(std::string(tabName(kTabSound)) == "Sound" && std::string(tabName(kTabExtras)) == "Extras");
}

TEST(every_visible_control_appears_exactly_once)
{
    for (float w : { kMinWidth, kDefaultWidth, 1600.0f })
        for (uint32_t p = 0; p < kProfileCount; ++p)
            for (uint32_t mic = 0; mic < 6; ++mic)
                for (uint32_t prep = 0; prep < 6; ++prep) {
                    const UiContext c = ctxOf(p, mic, prep);
                    std::vector<int> inTabs(kParamCount, 0);
                    for (uint32_t t = 0; t < kTabCount; ++t) {
                        const Layout l = computeLayout(w, kDefaultHeight, (Tab)t, c);
                        std::vector<int> seen(kParamCount, 0);
                        for (const Row& r : l.rows)
                            if (r.param < kParamCount) ++seen[r.param]; // not the Instrument row
                        for (uint32_t i = 0; i < kParamCount; ++i) {
                            const bool header = has(headerParams(), i), drawing = has(drawingParams(), i);
                            if (header || drawing) {
                                // the header in every tab; the drawing's rows wide in every tab, narrow in Mics only
                                const bool here = header || l.wide || t == kTabMics;
                                CHECK(seen[i] == (here && paramVisible(i, c) ? 1 : 0));
                            } else {
                                CHECK(seen[i] <= 1);
                                inTabs[i] += seen[i];
                            }
                        }
                    }
                    for (uint32_t i = 0; i < kParamCount; ++i)
                        if (!has(headerParams(), i) && !has(drawingParams(), i))
                            CHECK(inTabs[i] == (paramVisible(i, c) ? 1 : 0));
                }
}

static void checkNoOverlap(const Layout& l, float w, float h)
{
    std::vector<Rect> fixed { l.panic, l.menu }, scrolled;
    for (const TabButton& t : l.tabs) fixed.push_back(t.rect);
    if (l.topView.w > 0) {
        (l.drawingScrolls ? scrolled : fixed).push_back(l.topView);
        (l.drawingScrolls ? scrolled : fixed).push_back(l.frontView);
    }
    for (const Row& r : l.rows) {
        (r.scrolls ? scrolled : fixed).push_back(r.rect);
        if (r.compact) // its parts lie inside it
            for (const Rect& part : { r.label, r.control, r.value })
                CHECK(part.x >= r.rect.x - 1e-3f && part.x + part.w <= r.rect.x + r.rect.w + 1e-3f && part.w > 0);
    }
    for (const Subheading& s : l.subheadings) scrolled.push_back(s.rect);
    if (l.emptyNote.w > 0) scrolled.push_back(l.emptyNote);
    for (const Rect& r : fixed) {
        CHECK(inside(r, w, h));
        CHECK(!r.overlaps(l.content));
    }
    const Rect area { l.content.x, l.content.y, l.content.w, std::max(l.content.h, l.contentHeight) };
    for (const Rect& r : scrolled)
        CHECK(r.x >= area.x - 1e-3f && r.y >= area.y - 1e-3f && r.x + r.w <= area.x + area.w + 1e-3f
            && r.y + r.h <= area.y + area.h + 1e-3f);
    for (const std::vector<Rect>* set : { &fixed, &scrolled })
        for (size_t i = 0; i < set->size(); ++i)
            for (size_t j = i + 1; j < set->size(); ++j)
                CHECK(!(*set)[i].overlaps((*set)[j]));
}

TEST(no_control_overlaps_another_at_three_sizes)
{
    const std::pair<float, float> sizes[] = { { kMinWidth, kDefaultHeight }, { kDefaultWidth, kDefaultHeight }, { 1600.0f, 900.0f } };
    for (const auto& [w, h] : sizes)
        for (uint32_t p = 0; p < kProfileCount; ++p)
            for (uint32_t mic = 0; mic < 6; ++mic)
                for (uint32_t prep : { 0u, 1u, 2u, 4u })
                    for (uint32_t t = 0; t < kTabCount; ++t)
                        checkNoOverlap(computeLayout(w, h, (Tab)t, ctxOf(p, mic, prep)), w, h);
}

TEST(default_size_fits_all_but_the_mics_tab)
{
    for (uint32_t p = 0; p < kProfileCount; ++p)
        for (uint32_t mic = 0; mic < 6; ++mic)
            for (uint32_t prep = 0; prep < 6; ++prep)
                for (Tab t : { kTabSound, kTabTuning, kTabCondition, kTabExtras }) {
                    const Layout l = computeLayout(kDefaultWidth, kDefaultHeight, t, ctxOf(p, mic, prep));
                    CHECK(l.wide && l.contentHeight <= l.content.h && maxScroll(l) == 0);
                }
}

TEST(no_tab_is_empty_for_any_instrument)
{
    // every tab has a row for every instrument today (the empty note stays for a future one)
    for (uint32_t p = 0; p < kProfileCount; ++p)
        for (uint32_t mic = 0; mic < 6; ++mic)
            for (uint32_t t = 0; t < kTabCount; ++t)
                CHECK(computeLayout(kDefaultWidth, kDefaultHeight, (Tab)t, ctxOf(p, mic, 0)).emptyNote.w == 0);
}

TEST(narrow_windows_put_the_drawing_in_the_mics_tab)
{
    for (uint32_t t = 0; t < kTabCount; ++t) {
        const Layout n = computeLayout(kMinWidth, kDefaultHeight, (Tab)t, ctxOf(0, 1, 0));
        CHECK(!n.wide && (n.topView.w > 0) == (t == kTabMics) && (n.frontView.w > 0) == (t == kTabMics));
        CHECK(n.drawingScrolls == (t == kTabMics));
        const Layout wd = computeLayout(kDefaultWidth, kDefaultHeight, (Tab)t, ctxOf(0, 1, 0));
        CHECK(wd.wide && wd.topView.w > 0 && !wd.drawingScrolls);
        for (const Row& r : wd.rows)
            if (has(drawingParams(), r.param)) CHECK(!r.scrolls && r.compact);
    }
}

TEST(content_columns_follow_the_panel_width)
{
    const UiContext c = ctxOf(0, 5, 1); // a grand at Custom, plucked: the fullest Mics and Extras
    CHECK(computeLayout(kDefaultWidth, 600, kTabExtras, c).columns == 1);
    CHECK(computeLayout(1700, 600, kTabExtras, c).columns == 2); // panel 748.8
    CHECK(computeLayout(2600, 600, kTabExtras, c).columns == 3);
    CHECK(computeLayout(5000, 600, kTabExtras, c).columns == 3);
    CHECK(computeLayout(2600, 600, kTabSound, c).columns == 1); // one group: one column
    CHECK(computeLayout(2600, 600, kTabMics, c).columns == 2);
    CHECK(computeLayout(kBreakpoint - 1, 600, kTabExtras, c).columns == 1);
    // a group stays whole: its rows below its subheading, in its column
    const Layout l = computeLayout(2600, 600, kTabExtras, c);
    for (const TabGroup& g : tabGroups(kTabExtras))
        for (const Subheading& s : l.subheadings)
            if (std::string(s.title) == g.title)
                for (const Row& r : l.rows)
                    if (has(g.params, r.param)) CHECK(r.rect.x == s.rect.x && r.rect.y > s.rect.y);
}

TEST(labels_line_up_within_a_tab)
{
    const Layout l = computeLayout(kDefaultWidth, kDefaultHeight, kTabExtras, ctxOf(0, 1, 1));
    float lw = -1;
    for (const Row& r : l.rows)
        if (r.scrolls) {
            if (lw < 0) lw = r.label.w;
            CHECK(r.label.w == lw && r.rect.h == kLineH && r.compact);
        }
    CHECK(lw >= textWidth("Sympathetic resonance"));
    CHECK(textWidth("abc") > 0 && textWidth("abcdef") == 2 * textWidth("abc") && textWidth("▾") == textWidth("a"));
}

TEST(the_wheel_scrolls_and_never_changes_a_control)
{
    // the wheel's only output is a scroll; it moves over the content when it overflows
    const UiContext c = ctxOf(0, 5, 1);
    const Layout l = computeLayout(kMinWidth, 400, kTabMics, c); // narrow, short: the Mics tab overflows
    CHECK(maxScroll(l) > 0);
    const float x = l.content.x + 5, y = l.content.y + 5;
    CHECK(wheelScroll(l, x, y, 0, -26) == 26);                 // down
    CHECK(wheelScroll(l, x, y, 26, 100) == 0);                 // up, stops at the top
    CHECK(wheelScroll(l, x, y, 0, -1e6f) == maxScroll(l));     // stops at the bottom
    for (const Row& r : l.rows)
        if (r.param == kVolume) CHECK(wheelScroll(l, r.rect.x + 1, r.rect.y + 1, 10, -26) == 10); // the header: nothing
    const Layout fits = computeLayout(kDefaultWidth, kDefaultHeight, kTabTuning, c);
    CHECK(maxScroll(fits) == 0 && wheelScroll(fits, fits.content.x + 5, fits.content.y + 5, 0, -26) == 0);
    const Layout wide = computeLayout(kDefaultWidth, 300, kTabMics, c);   // wide: over the drawing nothing scrolls
    CHECK(maxScroll(wide) > 0 && wheelScroll(wide, wide.topView.x + 5, wide.topView.y + 5, 0, -26) == 0);
}

TEST(rows_are_found_where_they_are_shown)
{
    const Layout l = computeLayout(kMinWidth, 400, kTabMics, ctxOf(0, 5, 0));
    const float s = 120;
    for (size_t i = 0; i < l.rows.size(); ++i) {
        const Rect r = shown(l.rows[i].rect, l.rows[i].scrolls, s);
        const float x = r.x + r.w / 2, y = r.y + r.h / 2;
        if (!l.rows[i].scrolls || inContent(l, x, y))
            CHECK(rowAt(l, x, y, s) == (int)i);
    }
    // a row scrolled up under the tab bar or the header takes no click there
    for (const TabButton& t : l.tabs) {
        CHECK(tabAt(l, t.rect.x + 1, t.rect.y + 1) == t.tab);
        CHECK(rowAt(l, t.rect.x + 1, t.rect.y + 1, s) == -1);
    }
    CHECK(tabAt(l, -5, -5) == kTabCount && rowAt(l, -5, -5, s) == -1);
    // window → layout: inside the content the scroll is added, outside not
    float x = l.content.x + 3, y = l.content.y + 3;
    toLayout(l, s, inContent(l, x, y), x, y);
    CHECK(y == l.content.y + 3 + s);
    float hx = 1, hy = 1;
    toLayout(l, s, inContent(l, hx, hy), hx, hy);
    CHECK(hy == 1);
    // a drag that began in the content keeps the scroll above it (the drawing does not jump)
    float dx = l.content.x + 3, dy = l.content.y - 20;
    toLayout(l, s, true, dx, dy);
    CHECK(dy == l.content.y - 20 + s);
}

TEST(the_drawings_rows_stay_in_short_wide_windows)
{
    // down to a header and the three rows: the views give way (zoomed, the minimum window's controls
    // viewport can be this short in logical units)
    for (float h : { 150.0f, 200.0f, 240.0f, kMinHeight, 450.0f })
        for (uint32_t mic : { 1u, 3u }) {
            const Layout l = computeLayout(kBreakpoint, h, kTabSound, ctxOf(0, mic, 0));
            int rows = 0;
            for (const Row& r : l.rows)
                if (has(drawingParams(), r.param)) {
                    ++rows;
                    CHECK(!r.scrolls && r.rect.y + r.rect.h <= h);
                }
            CHECK(rows == 3 && l.topView.h >= 1 && l.content.h >= 1);
        }
}

TEST(tiny_logical_sizes_stay_valid)
{
    // 400 % zoom on a small window: the logical size can fall below the minimum
    for (float w : { 100.0f, 200.0f, kMinWidth })
        for (float h : { 80.0f, 200.0f })
            for (uint32_t t = 0; t < kTabCount; ++t) {
                const Layout l = computeLayout(w, h, (Tab)t, ctxOf(0, 5, 1));
                CHECK(l.contentHeight > 0 && l.content.w > 0 && l.content.h > 0);
                for (const Row& r : l.rows) {
                    CHECK(r.rect.w > 0 && r.rect.h > 0 && r.control.w > 0);
                    if (r.compact) CHECK(r.label.w <= r.rect.w * 0.5f + 1e-3f);
                }
                for (size_t i = 0; i < l.tabs.size(); ++i) {
                    CHECK(l.tabs[i].rect.w > 0 && l.tabs[i].rect.x + l.tabs[i].rect.w <= w + 1e-3f);
                    for (size_t j = i + 1; j < l.tabs.size(); ++j) CHECK(!l.tabs[i].rect.overlaps(l.tabs[j].rect));
                }
                if (t == kTabMics) CHECK(l.topView.w > 0 && l.frontView.w > 0);
            }
}

TEST(the_minimum_window_keeps_the_controls_usable)
{
    // with the diagnostics expanded the window may shrink to them and a controls viewport: there the
    // drawing's rows stay in view (wide) and the tab's content keeps room for a few rows (narrow)
    uint32_t w = 0, h = 0;
    minimumWindowSize(1.0, 1.0, 370, w, h);
    const float controls = (float)h - 370;
    for (uint32_t mic : { 0u, 1u, 5u }) {
        const Layout wide = computeLayout(kBreakpoint, controls, kTabSound, ctxOf(0, mic, 0));
        for (const Row& r : wide.rows)
            if (has(drawingParams(), r.param)) CHECK(r.rect.y + r.rect.h <= controls);
        const Layout narrow = computeLayout(kMinWidth, controls, kTabSound, ctxOf(0, mic, 0));
        CHECK(narrow.content.h >= 3 * (kLineH + kLineGap));
    }
}

TEST(a_list_is_as_wide_as_its_longest_item_and_stays_in_the_view)
{
    // the Mic position menu in a narrow window: its long names fit the list, the list the window
    float longest = 0;
    for (uint32_t i = 0; i < paramLabelCount(kMicPosition); ++i)
        longest = std::max(longest, textWidth(paramLabel(kMicPosition, i)));
    const Layout l = computeLayout(kMinWidth, kDefaultHeight, kTabMics, ctxOf(0, 1, 0));
    for (const Row& r : l.rows)
        if (r.param == kMicPosition) {
            const Rect list = dropdownListRect(r.control, 6, 0, kDefaultHeight, longest, kMinWidth);
            CHECK(list.w >= longest + 20 && list.x >= 0 && list.x + list.w <= kMinWidth + 1e-3f);
            CHECK(list.h == 6 * kRowH);
        }
    // short items: the anchor's width, at least 200, at the anchor
    const Rect a { 100, 50, 120, 22 };
    const Rect s = dropdownListRect(a, 3, 0, 600, 30, 900);
    CHECK(s.x == 100 && s.w == 200 && s.y == 72);
}

TEST(a_sliders_value_fits_its_longest_text)
{
    // the mic distance's "20.00 m (default)" was cut in the 76 px value column
    for (float w : { kMinWidth, kDefaultWidth, 1600.0f })
        for (uint32_t mic : { 1u, 2u, 5u })
            for (uint32_t t = 0; t < kTabCount; ++t)
                for (const Row& r : computeLayout(w, kDefaultHeight, (Tab)t, ctxOf(0, mic, 1)).rows) {
                    if (!r.compact || r.kind != Kind::Slider)
                        continue;
                    const ParamInfo& p = paramInfo(r.param);
                    std::vector<std::string> texts { formatValue(r.param, p.min), formatValue(r.param, p.max), formatValue(r.param, p.def) };
                    if (r.param == kMicDistance)
                        texts.insert(texts.end(), { formatMicDistance(0, true, 20.0), formatMicDistance(5, true, 20.0) });
                    for (const std::string& s : texts)
                        CHECK(r.value.w >= textWidth(s.c_str()));
                    CHECK(r.control.w >= 40); // the track keeps a usable length
                }
    // the sliders of one column line up: one value column (the drawing's rows too)
    for (float w : { kMinWidth, kDefaultWidth })
        for (uint32_t t = 0; t < kTabCount; ++t) {
            const Layout l = computeLayout(w, kDefaultHeight, (Tab)t, ctxOf(0, 2, 1));
            for (const Row& a : l.rows)
                for (const Row& b : l.rows)
                    if (a.compact && b.compact && a.kind == Kind::Slider && b.kind == Kind::Slider && a.scrolls == b.scrolls
                        && a.rect.x == b.rect.x && a.rect.w == b.rect.w)
                        CHECK(a.control.x + a.control.w == b.control.x + b.control.w);
        }
}

TEST(the_drawing_takes_points_only_where_it_is_shown)
{
    // narrow Mics, short: the front view runs past the content area's bottom; a point in the margin
    // below it (where the view is not shown) is not the drawing's
    const Layout l = computeLayout(kMinWidth, 440, kTabMics, ctxOf(0, 1, 0)); // (the tab bar wraps to two rows)
    CHECK(l.drawingScrolls && l.frontView.y + l.frontView.h > l.content.y + l.content.h && l.frontView.y + 8 < l.content.y + l.content.h);
    const float bottom = l.content.y + l.content.h;
    CHECK(viewAt(l, l.frontView.x + 20, bottom + 4, 0) == -1);
    CHECK(viewAt(l, l.frontView.x + 20, bottom - 4, 0) == 1);
    CHECK(viewAt(l, l.topView.x + 20, l.topView.y + 20, 0) == 0);
    // scrolled: the top view's upper part is under the tab bar and the header
    CHECK(viewAt(l, l.topView.x + 20, l.content.y - 5, 60) == -1);
    CHECK(viewAt(l, l.topView.x + 20, l.content.y + 5, 60) == 0);
    // wide: the views are fixed, beside the content
    const Layout w = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(0, 1, 0));
    CHECK(viewAt(w, w.topView.x + 5, w.topView.y + 5, 100) == 0 && viewAt(w, w.frontView.x + 5, w.frontView.y + 5, 100) == 1);
    CHECK(viewAt(w, w.content.x + 5, w.content.y + 5, 0) == -1);
    // narrow, another tab: no drawing
    CHECK(viewAt(computeLayout(kMinWidth, 400, kTabSound, ctxOf(0, 1, 0)), 30, 300, 0) == -1);
}

TEST(panic_fires_on_every_change_of_its_input_and_not_at_activation)
{
    // the UI flips the hidden input at each click (it cannot send MIDI); the audio side silences
    // on a change, whatever the value: a session restored with the input at 1 does not fire
    PanicTrigger t;
    t.activate(1.0f);
    CHECK(!t.fire(1.0f) && t.fire(0.0f) && !t.fire(0.0f) && t.fire(1.0f) && !t.fire(1.0f));
    CHECK(std::string(diagInfo(kDiagPanicIn).symbol) == "diag_panic_in" && !diagIsOutput(kDiagPanicIn));
    // the next value the UI sends: the other one
    CHECK(panicNext(0.0f) == 1.0f && panicNext(1.0f) == 0.0f && panicNext(0.4f) == 1.0f);
}

/// the parameters' defaults, on an instrument at a microphone position with a preparation
static std::vector<float> defaults(uint32_t profile, uint32_t mic, uint32_t prep)
{
    std::vector<float> v(kParamCount);
    for (uint32_t i = 0; i < kParamCount; ++i)
        v[i] = paramInfo(i).def;
    (void)profile; // the instrument is a state: the values are the same on every one
    v[kMicPosition] = (float)mic;
    v[kPrepMode] = (float)prep;
    return v;
}

static std::string cliLine(const std::vector<float>& v)
{
    return midi2wavLine(v.data(), "/i/grand_a", 48000);
}

TEST(the_pedal_ring_is_a_control_of_its_own)
{
    // the thumps (Pedal noise) and the strings' ring as the dampers lift (Pedal ring), each its own
    // gain, the noise −20 dB by default, the ring −10 (by ear: louder than fitted relative to the
    // thumps); the ring beside the noise in the Noises group
    const ParamInfo& r = paramInfo(kPedalRing);
    CHECK(std::string(r.symbol) == "pedal_ring" && r.def == -10.0f && r.min == kOffDb && r.max == 12.0f);
    CHECK(paramInfo(kPedalNoise).def == -20.0f);
    std::vector<float> v = defaults(0, 0, 0);
    v[kPedalNoise] = kOffDb;
    v[kPedalRing] = 0.0f;
    const PeParams p = engineParams(v.data());
    CHECK(p.pedal == 0.0f && p.pedal_ring == 1.0f);
    CHECK(paramActive(kPedalRing, 0, 1, 0) && !paramActive(kPedalRing, kVib, 1, 0));
    const Layout l = computeLayout(900, 600, kTabExtras, ctxOf(0, 1, 0));
    int noise = -1, ring = -1;
    for (size_t i = 0; i < l.rows.size(); ++i) {
        if (l.rows[i].param == kPedalNoise) noise = (int)i;
        if (l.rows[i].param == kPedalRing) ring = (int)i;
    }
    CHECK(noise >= 0 && ring == noise + 1);
}

TEST(the_default_settings_copy_as_a_short_command_line)
{
    // the plugin's defaults differ from the engine's (midi2wav's) in five: the Condition's Normal
    // zone (the voicing's wear 0.05; midi2wav plays the export mint), the Pedal noise at −20 dB and
    // the Pedal ring at −10 dB (midi2wav plays them as fitted), the motor on (the vibraphone's), the wear's variant 1
    const std::string tail = " in.mid out.wav --rate 48000 --param pedal=0.1,pedal_ring=0.31622776,voicing=0.05,motor=1 --tuning seed=1";
    const std::string line = cliLine(defaults(0, 0, 0));
    CHECK(line == "dussek render /i/grand_a" + tail);
    // a directory that needs quoting, the latch, a rate that is no whole number
    std::vector<float> v = defaults(0, 0, 0);
    v[kSustainLatch] = 1;
    CHECK(midi2wavLine(v.data(), "/a b/it's", 44100.5)
        == "dussek render '/a b/it'\\''s' in.mid out.wav --rate 44100.5 --param pedal=0.1,pedal_ring=0.31622776,voicing=0.05,motor=1 --tuning seed=1 --latch");
}

TEST(touch_tab_has_a_graph_and_no_empty_note)
{
    for (auto wh : { std::pair<float, float> { 900, 600 }, { 420, 360 } }) {
        const Layout l = computeLayout(wh.first, wh.second, kTabTouch, ctxOf(0, 1, 0));
        CHECK(l.curve.w > 0 && l.curve.h >= 160);
        CHECK(l.history.y >= l.curve.y + l.curve.h && l.history.w > 0 && l.history.h > 0);
        CHECK(l.resetCurve.w > 0 && l.resetCurve.y + l.resetCurve.h <= l.curve.y);
        for (const Rect& r : { l.curve, l.history, l.resetCurve })
            CHECK(r.x >= l.content.x && r.x + r.w <= l.content.x + l.content.w && r.y >= l.content.y);
        CHECK(l.contentHeight >= l.history.y + l.history.h - l.content.y);
        CHECK(l.emptyNote.w == 0);
        int tabRows = 0;
        for (const Row& r : l.rows)
            if (r.scrolls) { // a piano: the Dynamics, the Strike variation and the re-strike, above the curve
                ++tabRows;
                CHECK((r.param == kDynamics || r.param == kVariation || r.param == kRestrike)
                      && r.rect.y + r.rect.h <= l.resetCurve.y);
            }
        CHECK(tabRows == 3);
        // an instrument without it (the harp): the curve heads the tab
        const Layout h = computeLayout(wh.first, wh.second, kTabTouch, ctxOf(kHarp, 1, 0));
        for (const Row& r : h.rows)
            CHECK(!r.scrolls);
        CHECK(h.resetCurve.y == h.content.y && h.curve.h >= 160);
    }
    CHECK(std::string(tabName(kTabTouch)) == "Touch");
    CHECK(computeLayout(900, 600, kTabSound, ctxOf(0, 1, 0)).curve.w == 0);
}

TEST(graph_mapping_round_trips)
{
    const Rect g { 10, 20, 300, 200 };
    for (auto v : { std::pair<float, float> { 1, 1 }, { 64, 64 }, { 127, 127 } }) {
        float x, y, vin, vout;
        CurveEdit::toGraph(g, v.first, v.second, x, y);
        CurveEdit::fromGraph(g, x, y, vin, vout);
        CHECK_NEAR(vin, v.first, 1e-4);
        CHECK_NEAR(vout, v.second, 1e-4);
    }
    float x, y;
    CurveEdit::toGraph(g, 127, 127, x, y);
    CHECK_NEAR(y, 20, 1e-4);
    CHECK_NEAR(x, 310, 1e-4);
}

TEST(curve_edit_adds_moves_and_removes_points)
{
    const Rect g { 0, 0, 252, 252 };
    auto at = [&](float vin, float vout, float& x, float& y) { CurveEdit::toGraph(g, vin, vout, x, y); };
    float x, y;
    CurveEdit e; // straight
    CHECK(e.curve().n == 2);
    at(64, 90, x, y);
    CHECK(e.add(g, x, y) == 1 && e.curve().n == 3 && e.curve().vin[1] == 64 && e.curve().vout[1] == 90);
    CHECK(e.pointAt(g, x + 3, y - 3) == 1 && e.pointAt(g, x + 20, y) == -1);
    at(30, 40, x, y);
    e.move(0, g, x, y); // the bottom end: up and down only
    CHECK(e.curve().vin[0] == 1 && e.curve().vout[0] == 40);
    at(127, 100, x, y);
    e.move(1, g, x, y); // past the top end's in: one short of it
    CHECK(e.curve().vin[1] == 126 && e.curve().vout[1] == 100);
    at(64, 10, x, y);
    e.move(1, g, x, y); // below the bottom end's out: held at it (the curve keeps rising)
    CHECK(e.curve().vin[1] == 64 && e.curve().vout[1] == 40);
    CHECK(!e.remove(0) && !e.remove(2));
    for (int v : { 10, 20, 30, 40, 80 }) {
        at((float)v, 64, x, y);
        CHECK(e.add(g, x, y) >= 0);
    }
    CHECK(e.curve().n == 8);
    at(100, 110, x, y);
    CHECK(e.add(g, x, y) == -1); // the 9th
    PeVelocityCurve back {};
    CHECK(pe_velocity_curve_parse(e.text().c_str(), &back) && back.n == e.curve().n);
    for (uint32_t i = 0; i < back.n; ++i)
        CHECK(back.vin[i] == e.curve().vin[i] && back.vout[i] == e.curve().vout[i]);
    CHECK(e.remove(3) && e.curve().n == 7);
    // a click below the band the curve may take there: the point goes in at the band's edge, far
    // from the pointer, and add() still names it (the UI drags it from there)
    CurveEdit f;
    at(64, 120, x, y);
    CHECK(f.add(g, x, y) == 1);
    at(32, 1, x, y);
    const int k = f.add(g, x, y);
    CHECK(k == 1 && f.curve().vin[1] == 32 && f.curve().vout[1] == 1 && f.pointAt(g, x, y) == 1);
    at(100, 2, x, y); // between (64, 120) and (127, 127): held at 120
    const int m = f.add(g, x, y);
    CHECK(m == 3 && f.curve().vout[3] == 120 && f.pointAt(g, x, y) == -1);
}

TEST(note_history_fades_dots_and_counts_bars)
{
    NoteHistory h;
    h.add(20, 0.0);
    h.add(100, 3.0);
    auto d = h.dots(3.5);
    CHECK(d.size() == 2 && d[0].first == 20 && d[1].first == 100);
    CHECK(d[0].second > 0 && d[0].second < d[1].second);
    d = h.dots(5.0);
    CHECK(d.size() == 1 && d[0].first == 100);
    const auto b = h.bars(10.0);
    int full = 0, partial = 0;
    for (float v : b) {
        if (v == 1.0f) ++full;
        else if (v != 0.0f) ++partial;
    }
    CHECK(full == 2 && partial == 0);
    for (float v : h.bars(40.0))
        CHECK(v == 0.0f);
    // a double-click on the bars clears them, and the dots with them
    h.add(64, 41.0);
    h.clear();
    CHECK(h.dots(41.5).empty() && !h.fading(41.5));
    for (float v : h.bars(41.5))
        CHECK(v == 0.0f);
}

TEST(curve_slot_hands_the_curve_to_every_new_engine)
{
    // the host's thread publishes the curve state, the audio thread takes it for its engine: once
    // per change, and again for every new engine (an instrument switch or a session restore builds
    // one without pe_adopt)
    PeVelocityCurve a {}, b {}, out {};
    CHECK(pe_velocity_curve_parse("1:1,64:100,127:127", &a));
    CHECK(pe_velocity_curve_parse("1:20,127:90", &b));
    auto* e1 = reinterpret_cast<PeEngine*>(0x1000);
    auto* e2 = reinterpret_cast<PeEngine*>(0x2000);
    CurveSlot slot;
    slot.publish(a); // before any engine exists
    CHECK(slot.take(e1, out) && out.n == 3 && out.vout[1] == 100);
    CHECK(!slot.take(e1, out));
    slot.publish(b);
    CHECK(slot.take(e1, out) && out.n == 2 && out.vout[0] == 20);
    CHECK(!slot.take(e1, out));
    CHECK(slot.take(e2, out) && out.n == 2 && out.vout[1] == 90); // a new engine: the current curve
    CHECK(!slot.take(e2, out));
    CurveSlot fresh; // nothing published: the straight line, for the first engine
    CHECK(fresh.take(e1, out) && out.n == 2 && out.vin[1] == 127 && out.vout[1] == 127);
}

TEST(curve_slot_take_never_waits_for_a_publish)
{
    // a publish half done (the host's thread preempted between its two steps): the audio thread's
    // take returns at once with nothing, and gets the curve on a later block
    PeVelocityCurve c {}, out {};
    CHECK(pe_velocity_curve_parse("1:1,64:100,127:127", &c));
    auto* e1 = reinterpret_cast<PeEngine*>(0x1000);
    CurveSlot slot;
    slot.beginPublish();
    CHECK(!slot.take(e1, out));
    slot.endPublish(c);
    CHECK(slot.take(e1, out) && out.n == 3 && out.vout[1] == 100);
}

TEST(curve_slot_forgets_its_engine)
{
    // the engine dropped (an instrument that does not load) and a new one built at the same
    // address: the slot must not take it for the old one (it would keep the straight line)
    PeVelocityCurve c {}, out {};
    CHECK(pe_velocity_curve_parse("1:20,127:90", &c));
    auto* e1 = reinterpret_cast<PeEngine*>(0x1000);
    CurveSlot slot;
    slot.publish(c);
    CHECK(slot.take(e1, out));
    slot.forget();
    CHECK(slot.take(e1, out) && out.vout[0] == 20);
}

TEST(a_bent_velocity_curve_copies_into_the_command_line)
{
    // the curve is a state, not a parameter: the line takes it beside the values (null: straight)
    PeVelocityCurve c {};
    CHECK(pe_velocity_curve_parse("1:1,64:100,127:127", &c));
    const std::vector<float> v = defaults(0, 0, 0);
    CHECK(midi2wavLine(v.data(), "/i/grand_a", 48000, &c)
        == "dussek render /i/grand_a in.mid out.wav --rate 48000 --velocity-curve 1:1,64:100,127:127 "
           "--param pedal=0.1,pedal_ring=0.31622776,voicing=0.05,motor=1 --tuning seed=1");
    CHECK(midi2wavLine(v.data(), "/i/grand_a", 48000, nullptr) == cliLine(v));
}

TEST(every_parameter_that_acts_shows_in_the_command_line)
{
    // where a control acts (paramVisible: what the UI shows), changing it changes the line: the
    // Copy CLI button leaves nothing out that the engine plays. The instrument, the position and
    // the preparation make the context; the temperament's key acts with a temperament, the bolt's
    // position with a bolt. The audio input acts on a sound the MIDI file does not carry: not in the line
    int missing = 0;
    for (uint32_t profile = 0; profile < kProfileCount; ++profile)
        for (uint32_t mic = 0; mic <= 5; ++mic)
            for (uint32_t prep = 0; prep <= 5; ++prep) {
                if (effectivePrep(profile, prep) != prep)
                    continue;
                const UiContext ctx = ctxOf(profile, mic, prep);
                for (uint32_t p = 0; p < kParamCount; ++p) {
                    if (p == kMicPosition || p == kPrepMode || p == kAudioInput || !paramVisible(p, ctx))
                        continue;
                    std::vector<float> v = defaults(profile, mic, prep);
                    if (p == kTemperamentKey)
                        v[kTemperament] = 1;
                    if (p == kBoltPosition)
                        v[kBoltMass] = paramInfo(kBoltMass).max;
                    const std::string base = cliLine(v);
                    const ParamInfo& d = paramInfo(p);
                    v[p] = d.def != d.max ? d.max : d.min;
                    if (p == kTemperament) // its last, "Scala file", plays nothing without a file
                        v[p] = 1;
                    if (p == kMallet && entryMallets(entryOf(profile)) > 1) // another of the instrument's own
                        v[p] = 1;
                    if (cliLine(v) == base) {
                        if (++missing <= 10)
                            std::fprintf(stderr, "  %s on %s, mic %u, prep %u: not in the line\n", d.symbol, kProfiles[profile], mic, prep);
                    }
                }
            }
    CHECK(missing == 0);
}

TEST(the_tuning_ignores_what_does_nothing_at_the_position)
{
    // at the recording's position the lid and the spread keys do nothing, the free pair only at
    // its own: no new engine for them (and nothing in the line)
    std::vector<float> v = defaults(0, 0, 0);
    const PeTuning t = engineTuning(v.data());
    v[kLid] = 10;
    v[kSpreadKeys] = 1;
    v[kMicLX] = 3;
    v[kBoltPosition] = 20; // no bolt
    CHECK(sameTuning(engineTuning(v.data()), t));
    v[kMicPosition] = 5;
    const PeTuning custom = engineTuning(v.data());
    CHECK(!sameTuning(custom, t) && custom.mic_setup.pos[0][0] == 3 && custom.lid == 0.1 && custom.spread_keys == 1);
    v[kPillow] = 50;
    CHECK(!sameTuning(engineTuning(v.data()), custom));
}

TEST(the_strike_variation_is_a_live_touch_control)
{
    // from the next strike (no rebuild): automatable; off by default
    CHECK(paramInfo(kVariation).automatable && paramInfo(kVariation).def == 0);
    std::vector<float> v = defaults(0, 0, 0);
    CHECK(engineParams(v.data()).variation == 0.0f);
    v[kVariation] = 200;
    CHECK(engineParams(v.data()).variation == 2.0f);
    bool onTouch = false;
    for (const TabGroup& g : tabGroups(kTabTouch))
        for (uint32_t p : g.params)
            onTouch |= p == kVariation;
    CHECK(onTouch);
}

TEST(the_sustain_is_compiled_on_the_sound_tab)
{
    // the ring's length rebuilds the instrument (like the unison width): not automatable
    std::vector<float> v = defaults(0, 0, 0);
    const PeTuning t = engineTuning(v.data());
    CHECK(t.sustain == 1.0);
    v[kSustain] = 200;
    const PeTuning longer = engineTuning(v.data());
    CHECK(longer.sustain == 2.0 && !sameTuning(longer, t));
    CHECK(!paramInfo(kSustain).automatable && paramInfo(kSustain).def == 100);
    bool onSound = false;
    for (const TabGroup& g : tabGroups(kTabSound))
        for (uint32_t p : g.params)
            onSound |= p == kSustain;
    CHECK(onSound);
}

// Everything the UI derives per instrument, one fact per line, sorted (the catalog of the
// instrument-discovery plan must reproduce it; plugin/tests/visibility.txt)
static std::vector<std::string> uiTable()
{
    std::vector<std::string> t;
    const Catalog& c = committed();
    uint32_t pos = 0;
    for (const InstrumentEntry* e : c.menu()) {
        char line[256];
        std::snprintf(line, sizeof(line), "menu %02u %s|%s|%s", pos++, e->group.c_str(), e->name.c_str(), e->id.c_str());
        t.push_back(line);
    }
    for (const InstrumentEntry* e : c.menu()) {
        const std::string& id = e->id;
        t.push_back(id + " body " + std::to_string((uint32_t)entryBody(*e)));
        int lo = 0, hi = 0;
        entryRecorded(*e, lo, hi);
        t.push_back(id + " recorded " + std::to_string(lo) + " " + std::to_string(hi));
        t.push_back(id + " soft " + std::string(entrySoftPedal(*e) ? "1" : "0"));
        uint32_t v[5];
        std::string preps = id + " preps";
        for (uint32_t i = 0, n = entryPreparations(*e, v); i < n; ++i) preps += " " + std::to_string(v[i]);
        t.push_back(preps);
        std::string mallets = id + " mallets ";
        for (uint32_t i = 0, n = entryMallets(*e); i < n; ++i) mallets += (i ? "|" : "") + malletLabel(*e, i);
        t.push_back(mallets);
        for (uint32_t i = 0; i < kParamCount; ++i) {
            if (i == kMallet) continue;
            t.push_back(id + " label " + paramInfo(i).symbol + " " + pe::displayName(i, *e) + "|" + pe::displayDescription(i, *e));
        }
        for (uint32_t mic = 0; mic < 6; ++mic)
            for (uint32_t prep = 0; prep < 6; ++prep) {
                std::string line = id + " visible mic=" + std::to_string(mic) + " prep=" + std::to_string(prep);
                for (uint32_t i = 0; i < kParamCount; ++i)
                    if (paramVisible(i, UiContext { e, mic, prep })) line += std::string(" ") + paramInfo(i).symbol;
                t.push_back(line);
            }
    }
    std::sort(t.begin(), t.end());
    return t;
}

TEST(the_ui_matches_the_recorded_table)
{
    const std::string path = std::string(PE_TESTS_DIR) + "/visibility.txt";
    const std::vector<std::string> now = uiTable();
    if (const char* w = std::getenv("PE_WRITE_UI_TABLE"); w != nullptr && *w) {
        FILE* f = std::fopen(path.c_str(), "w");
        CHECK(f != nullptr);
        if (f == nullptr) return;
        for (const std::string& l : now) std::fprintf(f, "%s\n", l.c_str());
        std::fclose(f);
        return;
    }
    std::vector<std::string> want;
    if (FILE* f = std::fopen(path.c_str(), "r")) {
        char buf[4096];
        while (std::fgets(buf, sizeof(buf), f)) {
            std::string l(buf);
            while (!l.empty() && (l.back() == '\n' || l.back() == '\r')) l.pop_back();
            want.push_back(l);
        }
        std::fclose(f);
    }
    CHECK(!want.empty());
    size_t shown = 0;
    for (const std::string& l : want)
        if (!std::binary_search(now.begin(), now.end(), l) && ++shown <= 20) std::fprintf(stderr, "  lost: %s\n", l.c_str());
    for (const std::string& l : now)
        if (!std::binary_search(want.begin(), want.end(), l) && ++shown <= 40) std::fprintf(stderr, "  new:  %s\n", l.c_str());
    CHECK(shown == 0);
}

// a temporary directory tree for the catalog: <root>/<sub>/<id>/instrument.json copied from
// instruments/<from>, its menu block replaced
struct TempTree {
    std::string root;
    TempTree() : root(tempDir("pe-catalog-")) {}
    ~TempTree()
    {
        std::error_code ec;
        std::filesystem::remove_all(root, ec);
    }
    std::string path(const char* sub) const { return root + "/" + sub; }
    /// the copy's "menu" is `menu` (empty: none); an empty data.bin unless dataBin is false
    void add(const char* sub, const char* id, const char* from, const std::string& menu, bool dataBin = true) const
    {
        std::string json;
        if (FILE* f = std::fopen((std::string(PE_INSTRUMENTS_DIR) + "/" + from + "/instrument.json").c_str(), "rb")) {
            char buf[65536];
            size_t n;
            while ((n = std::fread(buf, 1, sizeof(buf), f)) > 0) json.append(buf, n);
            std::fclose(f);
        }
        // drop the export's menu block (an object without nested braces in strings: the exports')
        if (const size_t m = json.find("\"menu\": {"); m != std::string::npos) {
            size_t e = json.find('{', m), depth = 0;
            for (; e < json.size(); ++e) {
                if (json[e] == '{') ++depth;
                else if (json[e] == '}' && --depth == 0) break;
            }
            size_t end = e + 1;
            if (json.compare(end, 2, ", ") == 0) end += 2;
            json.erase(m, end - m);
        }
        if (!menu.empty()) json.insert(1, "\"menu\": " + menu + ", ");
        const std::string dir = path(sub) + "/" + id;
        std::filesystem::create_directories(dir);
        if (FILE* f = std::fopen((dir + "/instrument.json").c_str(), "wb")) {
            std::fwrite(json.data(), 1, json.size(), f);
            std::fclose(f);
        }
        if (dataBin)
            if (FILE* f = std::fopen((dir + "/data.bin").c_str(), "wb")) std::fclose(f);
    }
};

TEST(catalog_shadows_and_skips)
{
    TempTree t;
    t.add("a", "harp", "harp", R"({"name": "Harp A", "group": "Harps"})");
    t.add("b", "harp", "harp", R"({"name": "Shadowed", "group": "Harps"})");
    t.add("b", "lever", "leverharp", R"({"name": "Harp B", "group": "Harps"})");
    t.add("b", "nodata", "harp", R"({"name": "No data", "group": "Harps"})", false);
    const Catalog c = Catalog::scan({ t.path("a"), t.path("missing"), t.path("b"), "" });
    CHECK(c.find("harp") != nullptr && c.find("harp")->name == "Harp A");
    CHECK(c.find("lever") != nullptr && c.find("nodata") == nullptr);
    CHECK(c.messages().size() == 1); // the shadowed one (a missing path is no problem; no data.bin: not an instrument)
}

TEST(aliases_resolve_old_instrument_ids)
{
    // a renamed or removed instrument's saved sessions find its successor through aliases.txt in an
    // instrument folder ("old new" per line); a real folder wins; a missing target or a cycle leaves
    // the session's id missing (the UI names it), never the menu's first
    TempTree t;
    t.add("a", "b", "harp", R"({"name": "B", "group": "Harps"})");
    t.add("a", "c", "harp", R"({"name": "C", "group": "Harps"})");
    {
        std::ofstream f(t.path("a") + "/aliases.txt");
        f << "# old new\na b\nx y\np q\nq p\nc d\nchain a\n";
    }
    const Catalog c = Catalog::scan({ t.path("a") });
    CHECK(c.find("a") != nullptr && c.find("a")->id == "b");
    CHECK(c.find("chain") != nullptr && c.find("chain")->id == "b"); // chain → a → b
    CHECK(c.find("x") == nullptr && c.find("p") == nullptr);          // a missing target; a cycle
    CHECK(c.find("c") != nullptr && c.find("c")->id == "c");          // the real folder wins
    CHECK(c.find("zzz") == nullptr && choose(c, "zzz").entry == nullptr && choose(c, "zzz").id == "zzz");
    CHECK(c.menu().size() == 2); // an alias is no menu entry
    // the engine's side finds the folder the same way (the worker loads instrumentDir's)
    const char* env = std::getenv("DUSSEK_INSTRUMENTS");
    const std::string envWas = env ? env : "";
    setEnv("DUSSEK_INSTRUMENTS", t.path("a").c_str());
    CHECK(instrumentDir(nullptr, "a") == c.find("b")->dir && instrumentDir(nullptr, "p").empty());
    if (env) setEnv("DUSSEK_INSTRUMENTS", envWas.c_str()); else unsetEnv("DUSSEK_INSTRUMENTS");
}

TEST(an_alias_never_hides_a_real_folder)
{
    // the shipped folder (searched first) aliases grand_x to b; the player's own folder grand_x is a
    // real instrument: it is listed, found and loaded as itself, in this path order and the other
    TempTree t;
    t.add("shipped", "b", "harp", R"({"name": "B", "group": "Harps"})");
    {
        std::ofstream f(t.path("shipped") + "/aliases.txt");
        f << "grand_x b\n";
    }
    t.add("user", "grand_x", "harp", R"({"name": "Mine", "group": "Harps"})");
    for (const auto& paths : { std::vector<std::string> { t.path("shipped"), t.path("user") },
             std::vector<std::string> { t.path("user"), t.path("shipped") } }) {
        const Catalog c = Catalog::scan(paths);
        CHECK(c.find("grand_x") != nullptr && c.find("grand_x")->name == "Mine" && c.menu().size() == 2);
        CHECK(c.messages().empty());
    }
    const char* env = std::getenv("DUSSEK_INSTRUMENTS");
    const std::string envWas = env ? env : "";
    setEnv("DUSSEK_INSTRUMENTS", t.path("user").c_str());
    const Catalog c = Catalog::scan({ t.path("user") });
    CHECK(instrumentDir(nullptr, "grand_x") == c.find("grand_x")->dir);
    if (env) setEnv("DUSSEK_INSTRUMENTS", envWas.c_str()); else unsetEnv("DUSSEK_INSTRUMENTS");
}

TEST(catalog_rejects_and_keeps)
{
    TempTree t;
    t.add("a", "bad", "harp", R"({"name": 3, "group": "Harps"})");
    t.add("a", "odd", "harp", R"({"name": "Odd", "group": "Harps", "labels": {"no_such": {"name": "x"}}})");
    const Catalog c = Catalog::scan({ t.path("a") });
    CHECK(c.find("bad") == nullptr && c.messages().size() == 1);
    CHECK(c.find("odd") != nullptr); // an unknown label symbol is ignored
}

TEST(catalog_orders_the_menu)
{
    TempTree t;
    t.add("a", "z1", "harp", R"({"name": "Zeta", "group": "Harps"})");
    t.add("a", "h2", "harp", R"({"name": "Harp B", "group": "Harps"})");
    t.add("a", "h1", "harp", R"({"name": "Harp A", "group": "Harps"})");
    t.add("a", "w", "harp", R"({"name": "Weird", "group": "Wires"})");
    t.add("a", "plain", "harp", "");
    t.add("a", "g", "grand_a", R"({"name": "Grand Q", "group": "Grand pianos"})");
    t.add("a", "hid", "harp", R"({"name": "Hidden", "group": "Harps", "hidden": true})");
    const Catalog c = Catalog::scan({ t.path("a") });
    std::vector<std::string> ids;
    for (const InstrumentEntry* e : c.menu()) ids.push_back(e->id);
    CHECK((ids == std::vector<std::string> { "g", "h1", "h2", "z1", "w", "plain" }));
    CHECK((c.groups() == std::vector<std::string> { "Grand pianos", "Harps", "Wires", "Other" }));
    CHECK(c.find("plain") != nullptr && c.find("plain")->name == "plain" && c.find("hid") != nullptr);
    CHECK(c.first() != nullptr && c.first()->id == "g");
}

TEST(catalog_of_the_committed_instruments)
{
    const Catalog& c = committed();
    CHECK(c.menu().size() == 40 && c.messages().empty());
    CHECK(c.find("registers_fixture") != nullptr && c.find("registers_fixture")->hidden);
    CHECK(c.first() != nullptr && c.first()->name == "Grand A");
    const InstrumentEntry* k = c.find("kalimba");
    CHECK(k != nullptr && k->label("knock") != nullptr && std::string(k->label("knock")->name) == "Thumb click");
    CHECK(k != nullptr && k->label("volume") == nullptr);
}

TEST(choice_by_id)
{
    const Catalog c = Catalog::scan({ PE_INSTRUMENTS_DIR });
    CHECK(choose(c, "").entry == c.first() && choose(c, "").id == c.first()->id);
    CHECK(choose(c, "harp").entry == c.find("harp"));
    const Choice m = choose(c, "harp2");
    CHECK(m.id == "harp2" && m.entry == nullptr);
}

TEST(a_dropped_in_instrument_plays)
{
    TempTree t;
    // the harp, renamed, with its own menu entry; its data.bin copied (pe_create reads it)
    t.add("user", "myharp", "harp", R"({"name": "My harp", "group": "Harps"})");
    std::filesystem::copy_file(std::string(PE_INSTRUMENTS_DIR) + "/harp/data.bin", t.path("user") + "/myharp/data.bin",
                               std::filesystem::copy_options::overwrite_existing);
    const Catalog c = Catalog::scan({ PE_INSTRUMENTS_DIR, t.path("user") });
    const InstrumentEntry* e = c.find("myharp");
    CHECK(e != nullptr && e->name == "My harp" && e->group == "Harps");
    if (e == nullptr) return;
    // its controls and labels are the harp's
    const InstrumentEntry* harp = c.find("harp");
    CHECK(harp != nullptr);
    if (harp == nullptr) return;
    for (uint32_t i = 0; i < kParamCount; ++i) {
        CHECK(std::strcmp(pe::displayName(i, *e), pe::displayName(i, *harp)) == 0);
        for (uint32_t mic = 0; mic < 6; ++mic)
            CHECK(pe::paramActive(i, *e, mic, 0) == pe::paramActive(i, *harp, mic, 0));
    }
    // listed in its group, after the harp's own entries ("My harp" sorts after "Harp A", "Harp B")
    std::vector<std::string> harps;
    for (const InstrumentEntry* m : c.menu())
        if (m->group == "Harps") harps.push_back(m->name);
    CHECK((harps == std::vector<std::string> { "Harp A", "Harp B", "My harp" }));
    char err[256] = {};
    PeEngine* eng = pe_create(e->dir.c_str(), 48000.0, nullptr, nullptr, err, sizeof(err));
    CHECK(eng != nullptr);
    if (eng == nullptr) {
        std::fprintf(stderr, "  %s\n", err);
        return;
    }
    std::vector<float> l(4800), r(4800);
    pe_note_on(eng, 0, 60, 100);
    pe_process(eng, nullptr, l.data(), r.data(), 4800);
    float peak = 0;
    for (float s : l) peak = std::max(peak, std::fabs(s));
    CHECK(peak > 1e-4f);
    pe_destroy(eng);
}

TEST(the_dsp_finds_an_id_where_the_catalog_does)
{
    // the worker's lookup (instrumentDir) takes the catalog's entry: the first accepted directory
    // (an instrument.json and a data.bin the engine accepts), a plain name only
    TempTree t;
    t.add("env", "harp", "harp", R"({"name": "Half done", "group": "Harps"})", false); // no data.bin
    t.add("bundle/instruments", "harp", "harp", R"({"name": "Harp A", "group": "Harps"})");
    t.add("env", "bad", "harp", R"({"name": 3, "group": "Harps"})"); // rejected by the engine's report
    t.add("bundle/instruments", "bad", "harp", R"({"name": "Fine", "group": "Harps"})");
    // the platform's user folder (pe::dataDirs), from the variable it reads, inside the tree
    const char* var = pe::hostPlatform() == pe::Platform::Linux ? "XDG_DATA_HOME" : pe::hostPlatform() == pe::Platform::MacOS ? "HOME" : "APPDATA";
    const char* env = std::getenv("DUSSEK_INSTRUMENTS");
    const char* xdg = std::getenv(var);
    const std::string envWas = env ? env : "", xdgWas = xdg ? xdg : "";
    setEnv("DUSSEK_INSTRUMENTS", t.path("env").c_str());
    setEnv(var, t.path("xdg").c_str());
    const std::string data = pe::dataDirs().data;
    CHECK(data.rfind(t.path("xdg"), 0) == 0);
    // a snapshot (proto/snapshot.py) in the data directory's snapshots folder, its own group
    const std::string snapshots = "xdg" + data.substr(t.path("xdg").size()) + "/snapshots";
    t.add(snapshots.c_str(), "harp@t-1", "harp", R"({"name": "Harp A @ t-1", "group": "Snapshots"})");
    const std::string bundle = t.path("bundle");
    const Catalog c = Catalog::scan(Catalog::searchPaths(bundle.c_str()));
    CHECK(c.find("harp") != nullptr && instrumentDir(bundle.c_str(), "harp") == c.find("harp")->dir);
    CHECK(c.find("bad") != nullptr && instrumentDir(bundle.c_str(), "bad") == c.find("bad")->dir);
    CHECK(instrumentDir(bundle.c_str(), "../instruments/harp").empty() && instrumentDir(bundle.c_str(), "..").empty());
    CHECK(instrumentDir(bundle.c_str(), "").empty() && instrumentDir(bundle.c_str(), "nonesuch").empty());
    CHECK(c.find("harp@t-1") != nullptr && c.find("harp@t-1")->group == "Snapshots");
    CHECK(Catalog::searchPaths(bundle.c_str()).back() == data + "/snapshots");
    if (env) setEnv("DUSSEK_INSTRUMENTS", envWas.c_str()); else unsetEnv("DUSSEK_INSTRUMENTS");
    if (xdg) setEnv(var, xdgWas.c_str()); else unsetEnv(var);
}

// ---- the worker → audio thread engine handover (PianoPlugin), with stand-ins for engines
TEST(a_new_engine_replaces_the_one_playing)
{
    EngineHandoff<int> h;
    int a = 1, b = 2;
    int* current = &a;
    int* next = nullptr;
    CHECK(h.poll(next) == EngineHandoff<int>::Change::None);
    CHECK(h.offer(&b) == nullptr);
    CHECK(h.poll(next) == EngineHandoff<int>::Change::Next && next == &b);
    h.retire(current); // after the new one adopted from it
    current = next;
    CHECK(h.retired() == &a && h.retired() == nullptr && current == &b);
}

TEST(an_instrument_that_does_not_load_silences_the_one_playing)
{
    // the chosen instrument is missing or broken: no engine (silence), not the previous one
    EngineHandoff<int> h;
    int a = 1;
    int* next = nullptr;
    CHECK(h.drop() == nullptr);
    CHECK(h.poll(next) == EngineHandoff<int>::Change::Drop && next == nullptr);
    h.retire(&a);
    CHECK(h.retired() == &a);
    CHECK(h.poll(next) == EngineHandoff<int>::Change::None); // dropped once
}

TEST(the_handover_keeps_the_last_word_of_the_worker)
{
    EngineHandoff<int> h;
    int a = 1, b = 2;
    int* next = nullptr;
    // a drop, then a new engine before the audio thread looked: the new engine plays
    h.drop();
    CHECK(h.offer(&a) == nullptr);
    CHECK(h.poll(next) == EngineHandoff<int>::Change::Next && next == &a);
    h.retire(nullptr);
    CHECK(h.retired() == nullptr);
    // a new engine, then a drop: the engine comes back to be freed, the audio thread drops
    CHECK(h.offer(&b) == nullptr);
    CHECK(h.drop() == &b);
    CHECK(h.poll(next) == EngineHandoff<int>::Change::Drop);
    // the retire slot busy: nothing changes until the worker has freed what it holds
    h.retire(&a);
    CHECK(h.offer(&b) == nullptr);
    CHECK(h.poll(next) == EngineHandoff<int>::Change::None);
    CHECK(h.retired() == &a && h.poll(next) == EngineHandoff<int>::Change::Next && next == &b);
}

TEST(cc_to_value_covers_sliders_switches_and_lists)
{
    const ListChoices none;
    CHECK(ccToValue(kLid, 0, none) == paramInfo(kLid).min && ccToValue(kLid, 127, none) == paramInfo(kLid).max);
    CHECK(std::fabs(ccToValue(kLid, 64, none) - fromNormalized(kLid, 64 / 127.0f)) < 1e-5f); // the slider's travel
    CHECK(ccToValue(kWearVariant, 64, none) == std::round(ccToValue(kWearVariant, 64, none))); // integer sliders round
    CHECK(ccToValue(kStop8, 0, none) == 0.0f && ccToValue(kStop8, 1, none) == 1.0f); // a pad hit softly switches on
    const ListChoices temps = listChoices(kTemperament, entryOf(0));
    CHECK(temps.count == pe_temperament_count() + 1);
    for (uint32_t i = 0; i < temps.count; ++i) // every entry reachable, in order, equal bands
        CHECK(ccToValue(kTemperament, (uint8_t)((i * 128 + temps.count - 1) / temps.count), temps) == (float)i);
    CHECK(ccToValue(kTemperament, 127, temps) == (float)(temps.count - 1));
    const ListChoices levels = listChoices(kDistanceLevel, entryOf(0)); // 2 entries: 0–63, 64–127
    CHECK(ccToValue(kDistanceLevel, 63, levels) == 0.0f && ccToValue(kDistanceLevel, 64, levels) == 1.0f);
    CHECK(listChoices(kLid, entryOf(0)).count == 0);
}

TEST(cc_bands_follow_the_instruments_menu)
{
    // each knob position lands on a value the instrument's own menu lists, on every committed instrument
    bool fewer = false, prepared = false;
    for (uint32_t p = 0; p < kProfileCount; ++p) {
        const InstrumentEntry& e = entryOf(p);
        fewer = fewer || (entryMallets(e) > 0 && entryMallets(e) < kMalletCount);
        uint32_t v[5];
        prepared = prepared || entryPreparations(e, v) > 0;
        for (uint32_t param : { (uint32_t)kMallet, (uint32_t)kPrepMode }) {
            const ListChoices c = listChoices(param, e);
            CHECK(c.count >= 1);
            std::set<float> seen;
            for (int x = 0; x < 128; ++x) {
                const float y = ccToValue(param, (uint8_t)x, c);
                seen.insert(y);
                CHECK(param == kMallet ? y < (float)std::max(1u, entryMallets(e))
                                       : (y == 0.0f || effectivePrep(e, (uint32_t)y) == (uint32_t)y));
            }
            CHECK(seen.size() == c.count); // every entry reachable
        }
    }
    CHECK(fewer && prepared);
}

TEST(midi_map_ties_move_and_round_trip)
{
    MidiMap m;
    m.tie(kLid, 0, 21);
    m.tie(kMicLX, 0, 21); // the CC moves to the new control
    CHECK(m.find(kLid) == nullptr && m.find(kMicLX) != nullptr);
    m.tie(kMicLX, 1, 22); // a control has one CC
    CHECK(m.ties().size() == 1 && m.find(kMicLX)->channel == 1 && m.find(kMicLX)->cc == 22);
    m.tie(kStop8, 0, 64); // the pedals and the panic CCs never tie
    m.tie(kStop8, 0, 120);
    CHECK(m.find(kStop8) == nullptr);
    m.tie(kTargetNext, 0, 40);
    m.programChange = true;
    const MidiMap r = MidiMap::parse(m.text());
    CHECK(r.programChange && r.ties().size() == 2 && r.find(kTargetNext)->cc == 40 && r.find(kMicLX)->cc == 22);
    CHECK(m.text().find("mic_l_x") != std::string::npos); // by symbol, not index
    CHECK(m.forget(kMicLX) && !m.forget(kMicLX));
    CHECK(tieLabel(*r.find(kTargetNext)) == "CC 40" && tieChannel(*r.find(kMicLX)) == "2");
    CHECK(std::string(targetSymbol(kTargetPrevious)) == "previous_instrument" && targetSymbol(kTargetCount) == nullptr);
    CHECK(std::string(targetName(kTargetNext)) == "Next instrument" && std::string(targetName(kLid)) == "Lid");
    CHECK(learnableCc(1) && !learnableCc(64) && !learnableCc(66) && !learnableCc(67) && !learnableCc(123) && !learnableCc(128));
    uint8_t ch, cc;
    bool zero;
    CHECK(unpackLearned((float)packLearned(15, 127), ch, cc, zero) && ch == 15 && cc == 127 && !zero);
    CHECK(unpackLearned((float)packLearned(3, 40, true), ch, cc, zero) && ch == 3 && cc == 40 && zero);
    CHECK(!unpackLearned(0.0f, ch, cc, zero) && !unpackLearned(5000.0f, ch, cc, zero));
}

TEST(midi_map_steps_share_a_cc)
{
    // a controller's + button sends 127 and its − button 0, on one CC: Next on one side, Previous on the other
    MidiMap m;
    m.tie(kTargetNext, 0, 40);
    m.tie(kTargetPrevious, 0, 40, true);
    CHECK(m.ties().size() == 2 && !m.find(kTargetNext)->zero && m.find(kTargetPrevious)->zero);
    CHECK(tieLabel(*m.find(kTargetPrevious)) == "CC 40 at 0" && tieLabel(*m.find(kTargetNext)) == "CC 40");
    const MidiMap r = MidiMap::parse(m.text());
    CHECK(r.ties().size() == 2 && r.find(kTargetPrevious)->zero && !r.find(kTargetNext)->zero);
    m.tie(kTargetNext, 0, 40, true); // the same side: Previous goes
    CHECK(m.ties().size() == 1 && m.find(kTargetNext)->zero);
    m.tie(kTargetPrevious, 0, 40);
    m.tie(kLid, 0, 40, true); // a control takes the whole CC, on both sides
    CHECK(m.ties().size() == 1 && !m.find(kLid)->zero);
    CHECK(MidiMap::parse("cc 1 30 lid zero\ncc 1 31 next_instrument up\n").ties().empty());
}

TEST(midi_map_reads_bad_files_safely)
{
    CHECK(MidiMap::parse("").ties().empty() && !MidiMap::parse("").programChange);
    const MidiMap g = MidiMap::parse("garbage\ncc 0 21 lid\ncc 17 21 lid\ncc 1 128 lid\ncc 1 21 no_such_control\n"
                                     "cc 1 64 lid\ncc 1 30 lid\ncc 1 30 mic_l_x\nprogram_change maybe\ncc 1 31\n");
    CHECK(g.ties().size() == 1 && g.find(kMicLX) != nullptr && g.find(kMicLX)->cc == 30 && g.find(kLid) == nullptr
        && !g.programChange); // the later of two lines on one CC wins
    const std::string dir = tempDir("pe_midimap_");
    CHECK(!dir.empty());
    const std::string path = dir + "/a/b/midi-map.txt"; // missing directories are created
    MidiMap m;
    m.tie(kLid, 0, 21);
    CHECK(m.save(path) && MidiMap::load(path).find(kLid) != nullptr);
    CHECK(MidiMap::load(dir + "/missing.txt").ties().empty());
    if (readOnlyFoldersRefuse()) {
        std::filesystem::permissions(dir, std::filesystem::perms::owner_read | std::filesystem::perms::owner_exec);
        CHECK(!m.save(dir + "/c/midi-map.txt")); // unwritable: false, no crash
        std::filesystem::permissions(dir, std::filesystem::perms::owner_all);
    }
    // the platform's config folder (pe::dataDirs), here from the variable it reads
    const char* var = pe::hostPlatform() == pe::Platform::Linux ? "XDG_CONFIG_HOME" : pe::hostPlatform() == pe::Platform::MacOS ? "HOME" : "APPDATA";
    const char* old = std::getenv(var);
    const std::string saved = old != nullptr ? old : "";
    setEnv(var, dir.c_str());
    CHECK(!pe::dataDirs().config.empty() && midiMapPath() == pe::dataDirs().config + "/midi-map.txt");
    CHECK(midiMapPath().rfind(dir, 0) == 0);
    if (old != nullptr) setEnv(var, saved.c_str()); else unsetEnv(var);
    std::filesystem::remove_all(dir); // no directories left behind in the temporary folder
}

TEST(router_leaves_untied_ccs_and_program_change_alone)
{
    TieTable t;
    InstrumentLists l;
    MidiRouter r;
    MidiMap m;
    m.tie(kLid, 0, 21);
    t.load(m);
    for (uint8_t cc : { 7, 10, 91, 1, 64 })
        CHECK(r.cc(0, cc, 100, 0, t, l).kind == MidiRouter::Action::Builtin);
    CHECK(r.cc(1, 21, 100, 0, t, l).kind == MidiRouter::Action::Builtin); // the tie's channel only
    const MidiRouter::Action a = r.cc(0, 21, 127, 0, t, l);
    CHECK(a.kind == MidiRouter::Action::Set && a.param == kLid && a.value == paramInfo(kLid).max);
    CHECK(r.program(0, t).kind == MidiRouter::Action::None); // the switch is off
    m.programChange = true;
    t.load(m);
    CHECK(r.program(4, t).kind == MidiRouter::Action::Program && r.program(4, t).program == 4);
    m.tie(kLid, 0, 1); // the mod wheel tied: no Bebung
    t.load(m);
    CHECK(r.cc(0, 1, 0, 0, t, l).kind == MidiRouter::Action::Set);
    CHECK(r.cc(0, 21, 0, 0, t, l).kind == MidiRouter::Action::Builtin); // the old CC is free again
}

TEST(relative_knobs_move_from_where_the_control_is)
{
    // two's complement: 1–63 up, 127 … 65 down, faster turns larger; 0 one up (some send it slowly)
    CHECK(relativeSteps(1) == 1 && relativeSteps(10) == 10 && relativeSteps(0) == 1);
    CHECK(relativeSteps(127) == -1 && relativeSteps(117) == -11 && relativeSteps(64) == -64);
    const ListChoices none;
    CHECK(nudgeValue(kStop8, 0.0f, 1, none) == 1.0f && nudgeValue(kStop8, 1.0f, -2, none) == 0.0f);
    CHECK(std::fabs(nudgeValue(kLid, fromNormalized(kLid, 0.5f), 1, none) - fromNormalized(kLid, 0.5f + 1 / 127.0f)) < 1e-5f);
    CHECK(nudgeValue(kLid, paramInfo(kLid).max, 5, none) == paramInfo(kLid).max); // held at the ends
    CHECK(nudgeValue(kLid, 0.3f, 0, none) == 0.3f);
    const ParamInfo& wv = paramInfo(kWearVariant);
    CHECK(wv.integer && nudgeValue(kWearVariant, wv.min, 2, none) == wv.min + 2 && nudgeValue(kWearVariant, wv.min, -1, none) == wv.min);
    const ListChoices temps = listChoices(kTemperament, entryOf(0));
    CHECK(nudgeValue(kTemperament, (float)temps.values[1], 1, temps) == (float)temps.values[2]);
    CHECK(nudgeValue(kTemperament, (float)temps.values[0], -3, temps) == (float)temps.values[0]);

    MidiMap m;
    m.tie(kTargetNext, 0, 40);
    m.tie(kTargetPrevious, 0, 40, true);
    m.tie(kTargetNext, 0, 40, false, true); // a relative knob takes the whole CC
    CHECK(m.ties().size() == 1 && m.find(kTargetNext)->relative && !m.find(kTargetNext)->zero);
    m.tie(kLid, 0, 21, false, true);
    m.tie(kTemperament, 0, 22, false, true);
    const MidiMap r = MidiMap::parse(m.text());
    CHECK(r.ties().size() == 3 && r.find(kLid)->relative && tieLabel(*r.find(kLid)) == "CC 21 ±");
    CHECK(m.text().find("lid relative") != std::string::npos);

    TieTable t;
    InstrumentLists l;
    MidiRouter router;
    t.load(m);
    MidiRouter::Action a = router.cc(0, 21, 126, 0, t, l);
    CHECK(a.kind == MidiRouter::Action::Nudge && a.param == kLid && a.step == -2);
    CHECK(router.cc(0, 21, 0, 0, t, l).step == 1); // 0: the tie holds on both sides
    // a list: one entry per kRelativeStepsPerEntry steps; a turn back starts over
    int moved = 0;
    for (uint8_t v : { 1, 1, 1, 1 })
        if ((a = router.cc(0, 22, v, 0, t, l)).kind == MidiRouter::Action::Nudge) moved += a.step;
    CHECK(moved == 1);
    CHECK(router.cc(0, 22, 127, 0, t, l).kind == MidiRouter::Action::None); // the gathered step dropped
    a = router.cc(0, 22, 121, 0, t, l);                                      // 7 down, 1 down gathered before
    CHECK(a.kind == MidiRouter::Action::Nudge && a.step == -2 * 1 - 0);
    // the instrument: steps
    CHECK(router.cc(0, 40, 2, 0, t, l).kind == MidiRouter::Action::None);
    a = router.cc(0, 40, 1, 0, t, l);
    CHECK(a.kind == MidiRouter::Action::Step && a.step == 1);
    a = router.cc(0, 40, 122, 0, t, l);
    CHECK(a.kind == MidiRouter::Action::Step && a.step == -2);
}

TEST(a_pad_flips_a_switch_and_a_knob_turns_the_instrument)
{
    MidiMap m;
    m.tie(kStop8, 0, 30, false, false, true);
    m.tie(kLid, 0, 31, false, false, true); // a slider does not flip: an ordinary tie
    m.tie(kTargetKnob, 0, 32);              // always relative
    m.tie(kTargetNext, 0, 33);              // the buttons keep theirs
    CHECK(m.find(kStop8)->flip && !m.find(kLid)->flip && m.find(kTargetKnob)->relative && m.find(kTargetNext) != nullptr);
    CHECK(tieLabel(*m.find(kStop8)) == "CC 30 flip" && tieLabel(*m.find(kTargetKnob)) == "CC 32");
    const MidiMap r = MidiMap::parse(m.text());
    CHECK(r.find(kStop8)->flip && r.find(kTargetKnob)->relative && r.ties().size() == 4);
    CHECK(MidiMap::parse("cc 1 30 lid flip\n").ties().empty()); // a flip on a slider: left out
    CHECK(std::string(targetSymbol(kTargetKnob)) == "instrument_knob");
    TieTable t;
    InstrumentLists l;
    MidiRouter router;
    t.load(m);
    MidiRouter::Action a = router.cc(0, 30, 23, 0, t, l); // a soft press
    CHECK(a.kind == MidiRouter::Action::Flip && a.param == kStop8);
    CHECK(router.cc(0, 30, 0, 0, t, l).kind == MidiRouter::Action::None); // the release
    CHECK(router.cc(0, 32, 3, 0, t, l).step == 1 && router.cc(0, 32, 125, 0, t, l).step == -1);
}

TEST(soft_takeover_waits_for_the_knob)
{
    Takeover k;
    const float lo = fromNormalized(kLid, 0.2f), at = fromNormalized(kLid, 0.5f), hi = fromNormalized(kLid, 0.8f);
    CHECK(!k.accept(kLid, lo, at));                               // far from the slider: held
    CHECK(!k.accept(kLid, fromNormalized(kLid, 0.3f), at));
    CHECK(k.accept(kLid, fromNormalized(kLid, 0.6f), at));        // passed it: taken over
    CHECK(k.accept(kLid, hi, fromNormalized(kLid, 0.6f)));        // where MIDI left it: follows
    CHECK(!k.accept(kLid, fromNormalized(kLid, 0.9f), at));       // the slider moved otherwise: held again
    CHECK(k.accept(kLid, fromNormalized(kLid, 0.51f), at));       // near it
    CHECK(k.accept(kStop8, 1.0f, 0.0f) && k.accept(kTemperament, 3.0f, 0.0f)); // switches and lists: always
    MidiMap m;
    CHECK(m.softTakeover && !MidiMap::parse("soft_takeover off\n").softTakeover && MidiMap::parse(m.text()).softTakeover);
}

TEST(router_steps_at_every_press)
{
    TieTable t;
    InstrumentLists l;
    MidiRouter r;
    MidiMap m;
    m.tie(kTargetNext, 0, 40);
    m.tie(kTargetPrevious, 0, 41);
    m.tie(kTargetNext, 0, 42); // moves Next: one CC for both buttons
    m.tie(kTargetPrevious, 0, 42, true);
    m.tie(kTargetNext, 0, 40);
    t.load(m);
    int steps = 0;
    for (uint8_t v : { 30, 0, 12, 0, 127, 127 }) // a pad hit softly, twice (each released); a button that sends no release
        if (r.cc(0, 40, v, 0, t, l).kind == MidiRouter::Action::Step) ++steps;
    CHECK(steps == 4);
    CHECK(r.cc(0, 40, 0, 0, t, l).kind == MidiRouter::Action::None); // a release: no step, not the plugin's either
    m.tie(kTargetPrevious, 0, 42, true);
    m.tie(kTargetNext, 0, 43);
    t.load(m);
    CHECK(r.cc(0, 43, 127, 0, t, l).step == 1 && r.cc(0, 43, 127, 0, t, l).step == 1);
    m.tie(kTargetNext, 0, 42);
    t.load(m);
    CHECK(r.cc(0, 42, 127, 0, t, l).step == 1 && r.cc(0, 42, 0, 0, t, l).step == -1 && r.cc(0, 42, 0, 0, t, l).step == -1);
}

TEST(router_learn_skips_the_pedals)
{
    TieTable t;
    InstrumentLists l;
    MidiRouter r;
    const uint32_t learn = kLid + 1;
    for (int v = 0; v < 128; v += 8) // half-pedalling while waiting
        CHECK(r.cc(0, 64, (uint8_t)v, learn, t, l).kind == MidiRouter::Action::Builtin);
    const MidiRouter::Action a = r.cc(2, 21, 50, learn, t, l);
    CHECK(a.kind == MidiRouter::Action::Learned && a.learned == packLearned(2, 21));
    CHECK(MidiRouter().cc(2, 21, 0, learn, t, l).learned == packLearned(2, 21, true)); // caught on 0: that side
    CHECK(r.cc(2, 22, 50, learn, t, l).kind == MidiRouter::Action::Builtin);       // only the first for this learn
    CHECK(r.cc(2, 23, 50, kMicLX + 1, t, l).kind == MidiRouter::Action::Learned);  // a new learn catches again
    CHECK(r.cc(2, 24, 50, 0, t, l).kind == MidiRouter::Action::Builtin);
    CHECK(r.cc(2, 25, 50, kMicLX + 1, t, l).kind == MidiRouter::Action::Learned);  // the same target learned again
}

TEST(instrument_lists_publish_the_menus)
{
    InstrumentLists l;
    for (uint32_t p = 0; p < kProfileCount; ++p) {
        l.publish(entryOf(p));
        for (uint32_t param : { (uint32_t)kMallet, (uint32_t)kPrepMode, (uint32_t)kTemperament, (uint32_t)kLid }) {
            const ListChoices a = l.choices(param), b = listChoices(param, entryOf(p));
            CHECK(a.count == b.count && std::equal(a.values, a.values + a.count, b.values));
        }
    }
}

TEST(learn_menu_names_the_tie)
{
    MidiMap m;
    CHECK(learnMenuItems(m, kLid) == std::vector<std::string>({ "Learn MIDI", "Learn MIDI (relative knob)" }));
    m.tie(kLid, 0, 21);
    CHECK(learnMenuItems(m, kLid) == std::vector<std::string>({ "Learn again", "Learn again (relative knob)", "Forget CC 21" }));
    CHECK(learnMenuItems(m, kMicLX) == std::vector<std::string>({ "Learn MIDI", "Learn MIDI (relative knob)" }));
    m.tie(kLid, 0, 21, false, true);
    CHECK(learnMenuItems(m, kLid).back() == "Forget CC 21 ±");
    CHECK(learnMenuItems(m, kStop8) == std::vector<std::string>({ "Learn MIDI", "Learn MIDI (pad flips it)" })); // a switch
}

TEST(gestures_mention_midi_learn_where_it_works)
{
    for (Kind k : { Kind::Slider, Kind::Toggle, Kind::Dropdown }) {
        CHECK(std::string(tooltipGestures(k, true)).find("right-click: MIDI learn") != std::string::npos);
        CHECK(std::string(tooltipGestures(k)).find("right-click") == std::string::npos); // the Instrument menu
    }
}

TEST(midi_tab_lists_every_tie_and_the_switch)
{
    CHECK(std::string(tabName(kTabMidi)) == "MIDI" && kTabMidi + 1 == kTabAbout);
    // the default window keeps the tab bar on one row
    const Layout d = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(0, 1, 0));
    CHECK(d.tabs.size() == kTabCount && d.tabs.back().rect.y == d.tabs.front().rect.y);
    UiContext c = ctxOf(0, 1, 0);
    c.midiTies = 3;
    for (float w : { kMinWidth, kDefaultWidth, 1600.0f }) {
        for (float h : { kMinHeight, kDefaultHeight }) {
            const Layout l = computeLayout(w, h, kTabMidi, c);
            CHECK(l.midiRows.size() == 6); // 3 ties, Next, Previous and the knob untied
            CHECK(l.emptyNote.w == 0 && l.programSwitch.w > 0 && l.takeoverSwitch.w > 0 && !l.takeoverSwitch.overlaps(l.programSwitch));
            for (size_t i = 0; i < l.midiRows.size(); ++i) {
                const MidiRow& r = l.midiRows[i];
                CHECK(r.tie == (i < 3 ? (int)i : -1));
                CHECK(r.rect.x >= l.content.x && r.rect.x + r.rect.w <= l.content.x + l.content.w + 0.5f);
                CHECK(r.button.w > 0 && r.button.x >= r.rect.x && r.button.x + r.button.w <= r.rect.x + r.rect.w + 0.5f);
                CHECK(!r.rect.overlaps(l.programSwitch) && !r.rect.overlaps(l.takeoverSwitch));
                for (size_t j = i + 1; j < l.midiRows.size(); ++j)
                    CHECK(!r.rect.overlaps(l.midiRows[j].rect));
            }
            CHECK(l.midiRows[3].target == kTargetNext && l.midiRows[4].target == kTargetPrevious && l.midiRows[5].target == kTargetKnob);
        }
    }
    c.nextTied = true; // Next is one of the 3 ties: no row of its own
    const Layout n = computeLayout(kDefaultWidth, kDefaultHeight, kTabMidi, c);
    CHECK(n.midiRows.size() == 5 && n.midiRows[3].target == kTargetPrevious);
    c.midiTies = 40;
    const Layout many = computeLayout(kDefaultWidth, kDefaultHeight, kTabMidi, c);
    CHECK(many.contentHeight > many.content.h && maxScroll(many) > 0);
    for (Tab t : { kTabSound, kTabTouch, kTabExtras })
        CHECK(computeLayout(kDefaultWidth, kDefaultHeight, t, c).midiRows.empty()
            && computeLayout(kDefaultWidth, kDefaultHeight, t, c).programSwitch.w == 0);
}

TEST(router_catches_again_when_the_same_control_waits_again)
{
    // Learn again on one control, with no CC while nothing waited in between (a pad on toggle
    // sends one message per press): the new wait catches the next CC
    TieTable t;
    InstrumentLists l;
    MidiRouter r;
    const uint32_t learn = kLid + 1;
    r.setLearn(learn);
    CHECK(r.cc(0, 21, 127, learn, t, l).kind == MidiRouter::Action::Learned);
    r.setLearn(0); // the UI tied it; no CC arrives
    r.setLearn(learn);
    CHECK(r.cc(0, 22, 127, learn, t, l).kind == MidiRouter::Action::Learned);
    r.setLearn(learn); // the same wait, block after block: still only the first
    CHECK(r.cc(0, 23, 127, learn, t, l).kind == MidiRouter::Action::Builtin);
}

TEST(the_value_mirror_reaches_a_ui_that_opens_later)
{
    const float stale = std::nanf("");
    // a UI opening: a slot MIDI set that the host never heard of applies; never set (sequence 0),
    // stale (the host sent a newer value) or the UI's own value does not
    CHECK(mirrorApplies(true, 3, 0, 30.0f, 100.0f));
    CHECK(!mirrorApplies(true, 0, 0, 0.0f, 100.0f));
    CHECK(!mirrorApplies(true, 4, 0, stale, 100.0f));
    CHECK(!mirrorApplies(true, 3, 0, 100.0f, 100.0f));
    // then: only a sequence it has not seen
    CHECK(mirrorApplies(false, 5, 4, 30.0f, 100.0f));
    CHECK(!mirrorApplies(false, 5, 5, 30.0f, 100.0f));
    CHECK(!mirrorApplies(false, 6, 5, stale, 100.0f));
}

TEST(midi_map_update_keeps_other_ties_and_fails_cleanly)
{
    const std::string dir = tempDir("pe_midimap_");
    CHECK(!dir.empty());
    const std::string path = dir + "/midi-map.txt";
    MidiMap mine; // loaded before another instance wrote its tie
    MidiMap other;
    other.tie(kLid, 0, 21);
    CHECK(other.save(path));
    CHECK(mine.update(path, [](MidiMap& m) { m.tie(kRoom, 0, 22); }));
    CHECK(mine.find(kLid) != nullptr && mine.find(kRoom) != nullptr); // the other's tie kept
    const MidiMap disk = MidiMap::load(path);
    CHECK(disk.find(kLid) != nullptr && disk.find(kRoom) != nullptr);
    // a file that cannot be written: the map stays as it was (no tie the DSP never hears of)
    if (readOnlyFoldersRefuse()) {
        std::filesystem::permissions(dir, std::filesystem::perms::owner_read | std::filesystem::perms::owner_exec);
        CHECK(!mine.update(dir + "/sub/midi-map.txt", [](MidiMap& m) { m.tie(kStop8, 0, 30); }));
        CHECK(mine.find(kStop8) == nullptr && mine.ties().size() == 2);
        std::filesystem::permissions(dir, std::filesystem::perms::owner_all);
    }
    std::filesystem::remove_all(dir);
}

TEST(an_instrument_folder_anywhere_is_an_entry)
{
    // the "instrument" state of one opened from a folder: its absolute path
    const std::string dir = std::string(PE_INSTRUMENTS_DIR) + "/grand_a";
    const InstrumentEntry* e = Catalog::outside(dir);
    CHECK(e != nullptr && e->id == dir && e->dir == dir && e->group == "Opened" && e->name == "Grand A");
    CHECK(Catalog::outside(dir + "/") == e && Catalog::outside(dir) == e); // kept: the same entry
    CHECK(Catalog::outside(std::string(PE_INSTRUMENTS_DIR)) == nullptr);   // no instrument there
    CHECK(Catalog::outside("grand_a") == nullptr && Catalog::outside("") == nullptr); // a name, not a path
    const Catalog none;
    CHECK(choose(none, dir).entry == e && instrumentDir(nullptr, dir.c_str()) == dir);
#ifdef _WIN32
    // the file browser's form: backslashes
    std::string back = dir;
    std::replace(back.begin(), back.end(), '/', '\\');
    const InstrumentEntry* b = Catalog::outside(back + "\\");
    CHECK(b != nullptr && b->name == "Grand A");
#endif
}

TEST(learn_input_takes_only_targets)
{
    // kDiagLearnIn comes from the host (a saved session) as well as the UI: anything but 0 … target + 1 is no wait
    CHECK(learnInput(0.0f) == 0 && learnInput(-5.0f) == 0 && learnInput(std::nanf("")) == 0 && learnInput(1e9f) == 0);
    CHECK(learnInput((float)(kLid + 1)) == kLid + 1 && learnInput((float)kTargetCount) == kTargetCount);
    CHECK(learnInput((float)(kTargetCount + 1)) == 0);
}

TEST(a_tied_controls_label_leaves_room_for_its_tag)
{
    UiContext c = ctxOf(0, 1, 0);
    auto labelOf = [](const Layout& l, uint32_t p) { for (const Row& r : l.rows) if (r.param == p) return r.label; return Rect {}; };
    const float need = textWidth(displayName(kHammerHardness, entryOf(0))) + kTagW;
    CHECK(labelOf(computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, c), kHammerHardness).w < need); // the case it fixes
    c.tied.set(kHammerHardness);
    CHECK(labelOf(computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, c), kHammerHardness).w >= need);
}

TEST(tuning_watch_sees_every_change_from_any_thread)
{
    // values change on the host's thread and (MIDI) the audio thread; only the worker compares
    std::atomic<float> shared[kParamCount];
    float v[kParamCount];
    for (uint32_t i = 0; i < kParamCount; ++i)
        shared[i] = v[i] = paramInfo(i).def;
    TuningWatch w;
    w.reset(engineTuning(v));
    PeTuning t {};
    CHECK(!w.take(shared, t));      // nothing marked
    w.mark();
    CHECK(!w.take(shared, t));      // marked, the tuning unchanged
    shared[kConcertPitch] = 442.0f;
    w.mark();
    CHECK(w.take(shared, t) && t.a4_hz == 442.0);
    CHECK(!w.take(shared, t));
    // two writers and the worker at once: once they are done, the watch knows the final tuning
    std::atomic<bool> stop { false };
    std::thread worker([&] { PeTuning x; while (!stop) w.take(shared, x); });
    std::thread a([&] { for (int k = 0; k < 20000; ++k) { shared[kConcertPitch] = 430.0f + (float)(k % 20); w.mark(); } });
    std::thread b([&] { for (int k = 0; k < 20000; ++k) { shared[kStretch] = (float)(k % 5); w.mark(); } });
    a.join();
    b.join();
    stop = true;
    worker.join();
    w.take(shared, t);
    for (uint32_t i = 0; i < kParamCount; ++i)
        v[i] = shared[i];
    CHECK(sameTuning(w.last(), engineTuning(v)));
}

TEST(morph_weights_corners_edges_center)
{
    for (uint32_t n = 3; n <= 5; ++n)
        for (uint32_t i = 0; i < n; ++i) {
            const Point2 c = morphCorner(n, i);
            const std::vector<float> w = morphWeights(n, c.x, c.y);
            for (uint32_t j = 0; j < n; ++j)
                CHECK(std::fabs(w[j] - (i == j ? 1.0f : 0.0f)) < 1e-6f);
        }
    const Point2 a = morphCorner(4, 0), b = morphCorner(4, 1);
    const std::vector<float> w = morphWeights(4, (a.x + b.x) / 2, (a.y + b.y) / 2);
    CHECK(std::fabs(w[0] - 0.5f) < 1e-6f && std::fabs(w[1] - 0.5f) < 1e-6f && w[2] == 0.0f && w[3] == 0.0f);
    for (float v : morphWeights(5, 0, 0))
        CHECK(std::fabs(v - 0.2f) < 1e-6f);
    CHECK(std::fabs(morphCorner(3, 0).x) < 1e-6f && std::fabs(morphCorner(3, 0).y - 1.0f) < 1e-6f);
    CHECK(morphCorner(4, 1).x > 0.5f); // clockwise: the second corner to the right
}

TEST(morph_weights_sum_and_clamp)
{
    for (uint32_t n = 2; n <= 5; ++n)
        for (float x = -1.5f; x <= 1.5f; x += 0.1f)
            for (float y = -1.5f; y <= 1.5f; y += 0.1f) {
                const std::vector<float> w = morphWeights(n, x, y);
                float sum = 0;
                for (float v : w) {
                    CHECK(v >= 0.0f && v <= 1.0f);
                    sum += v;
                }
                CHECK(std::fabs(sum - 1.0f) < 0.03f);
                const Point2 c = morphClamp(n, x, y);
                CHECK(morphWeights(n, c.x, c.y) == w);
            }
    const std::vector<float> top = morphWeights(2, 0, 1), mid = morphWeights(2, 0.7f, 0);
    CHECK(top.size() == 2 && top[0] == 1.0f && top[1] == 0.0f);
    CHECK(std::fabs(mid[0] - 0.5f) < 1e-6f && std::fabs(mid[1] - 0.5f) < 1e-6f);
    CHECK(morphWeights(1, 0.3f, -0.2f) == std::vector<float> { 1.0f });
}

TEST(morph_weights_are_continuous)
{
    std::vector<float> last = morphWeights(5, -1.0f, -0.5f); // the line's first point
    for (float x = -1.0f; x <= 1.0f; x += 0.001f) {
        const std::vector<float> w = morphWeights(5, x, -0.2f + 0.3f * x);
        for (size_t i = 0; i < w.size(); ++i)
            CHECK(std::fabs(w[i] - last[i]) <= 0.02f);
        last = w;
    }
}

TEST(morph_partners_parse_and_validate)
{
    CHECK(parsePartners(" grand_d , ,upright_a,grand_d") == (std::vector<std::string> { "grand_d", "upright_a" }));
    CHECK(formatPartners({ "grand_d", "upright_a" }) == "grand_d,upright_a");
    CHECK(parsePartners("").empty());
    const Catalog& c = committed();
    CHECK(validPartners({ "nonesuch", "grand_d" }, c, "grand_a") == (std::vector<std::string> { "grand_d" }));
    CHECK(validPartners({ "flemish", "upright_a" }, c, "grand_a") == (std::vector<std::string> { "upright_a" }));
    CHECK(validPartners({ "grand_a", "grand_b" }, c, "grand_a") == (std::vector<std::string> { "grand_b" }));
    CHECK(validPartners({ "grand_b", "grand_f", "grand_e", "grand_d", "grand_c" }, c, "grand_a").size() == kMorphMax - 1);
    CHECK(validPartners({ "grand_b" }, c, "flemish").empty());
    CHECK(validPartners({ "grand_b" }, c, "nonesuch").empty());
}

TEST(a_superseded_morph_build_is_offered_only_for_the_same_instruments)
{
    const MorphKey a { "grand_a", { "grand_d" }, 48000 }, b = a, c { "grand_a", { "upright_a" }, 48000 },
        d { "grand_d", {}, 48000 }, e { "grand_a", { "grand_d" }, 44100 };
    CHECK(offerSuperseded(a, b));
    CHECK(!offerSuperseded(a, c));
    CHECK(!offerSuperseded(a, d));
    CHECK(!offerSuperseded(a, e)); // built for another sample rate: wrong pitch, never played
}

TEST(a_point_that_is_no_number_is_the_main_instrument)
{
    const float nan = std::nanf(""), inf = INFINITY;
    for (uint32_t n = 2; n <= 5; ++n)
        for (auto [x, y] : { std::pair { nan, 0.0f }, std::pair { 0.0f, nan }, std::pair { inf, -inf } }) {
            const std::vector<float> w = morphWeights(n, x, y);
            CHECK(w[0] == 1.0f);
            for (size_t i = 1; i < w.size(); ++i)
                CHECK(w[i] == 0.0f);
            const Point2 c = morphClamp(n, x, y);
            CHECK(std::isfinite(c.x) && std::isfinite(c.y));
        }
}

TEST(the_morph_tab_lays_out_its_list_and_pad)
{
    CHECK(std::string(tabName(kTabMorph)) == "Morph" && kTabMorph == kTabSound + 1);
    for (float w : { kMinWidth, kDefaultWidth, 1600.0f })
        for (uint32_t partners : { 0u, 1u, 4u }) {
            UiContext c = ctxOf(0, 1, 0); // Grand A
            c.partners = partners;
            const Layout l = computeLayout(w, kDefaultHeight, kTabMorph, c);
            CHECK(l.morphRows.size() == 1 + partners);
            CHECK(l.morphPad.w > 0 && l.morphPad.h > 0 && l.morphPlus.w > 0 && l.morphNote.w == 0);
            CHECK(l.morphFull == (partners + 1 >= kMorphMax));
            CHECK(l.morphRows[0].remove.w == 0 && l.morphRows[0].up.w == 0); // the main row: no buttons
            for (size_t i = 1; i < l.morphRows.size(); ++i)
                CHECK(l.morphRows[i].remove.w > 0 && l.morphRows[i].up.w > 0 && l.morphRows[i].down.w > 0);
            for (const MorphRow& r : l.morphRows)
                CHECK(!r.rect.overlaps(l.morphPad) && r.rect.x + r.rect.w <= l.content.x + l.content.w + 0.5f);
            CHECK(l.morphPad.x + l.morphPad.w <= l.content.x + l.content.w + 0.5f);
        }
    const Layout h = computeLayout(kDefaultWidth, kDefaultHeight, kTabMorph, ctxOf(kHarpB, 1, 0));
    CHECK(h.morphNote.w > 0 && h.morphRows.empty() && h.morphPad.w == 0 && h.morphPlus.w == 0);
}

TEST(the_tab_bar_fits_with_the_morph_tab)
{
    const Layout d = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(0, 1, 0));
    CHECK(d.tabs.size() == kTabCount);
    for (const TabButton& b : d.tabs)
        CHECK(b.rect.y == d.tabs.front().rect.y && b.rect.x + b.rect.w <= kDefaultWidth);
}

TEST(the_instrument_row_says_morph)
{
    CHECK(morphLabel("Grand A", 0) == "Grand A");
    CHECK(morphLabel("Grand A", 2) == "Grand A + 2 (morph)");
}

TEST(copy_cli_carries_the_morph)
{
    std::vector<float> v(kParamCount);
    for (uint32_t i = 0; i < kParamCount; ++i)
        v[i] = paramInfo(i).def;
    const std::string line = midi2wavLine(v.data(), "/i/grand_a", 48000, nullptr, { { "/i/grand_d", 0.5f }, { "/i/upright_a", 0.0f } });
    CHECK(line.find("--morph /i/grand_d=0.5") != std::string::npos);
    CHECK(line.find("upright_a") == std::string::npos);
    CHECK(midi2wavLine(v.data(), "/i/grand_a", 48000).find("--morph") == std::string::npos);
    // the Felt tab's felt: midi2wav builds it as the plugin does (quoted: the shell's ';')
    CHECK(midi2wavLine(v.data(), "/i/grand_a", 48000, nullptr, {}, "s:21=6;p:;c:;d:").find(" --felt 's:21=6;p:;c:;d:'") != std::string::npos);
    CHECK(midi2wavLine(v.data(), "/i/grand_a", 48000, nullptr, {}, "").find("--felt") == std::string::npos);
}

TEST(the_ui_shows_only_the_partners_that_play)
{
    // the state as a session from elsewhere may hold it: an unknown id, an instrument that cannot join
    const Catalog& c = committed();
    CHECK(shownPartners("nonesuch,grand_d,flemish", c, "grand_a") == (std::vector<std::string> { "grand_d" }));
    CHECK(shownPartners("grand_d", c, "flemish").empty());
    CHECK(shownPartners("grand_a,grand_d", c, "grand_a") == (std::vector<std::string> { "grand_d" }));
}

TEST(the_point_after_the_list_changes)
{
    const Point2 here { 0.3f, -0.2f };
    auto is = [](Point2 p, float x, float y) { return p.x == x && p.y == y; };
    CHECK(is(morphPointAfter(0, 1, here), 0, 1));     // the first partner joins silently, wherever the point was
    CHECK(is(morphPointAfter(2, 3, here), 0.3f, -0.2f)); // another joins where the point is
    CHECK(is(morphPointAfter(3, 2, here), 0, 0));     // a removal: the middle
    CHECK(is(morphPointAfter(1, 0, here), 0, 1));     // the last one gone: the main instrument's corner
    CHECK(is(morphPointAfter(2, 2, here), 0.3f, -0.2f)); // reordered: unchanged
}

TEST(the_old_engine_fades_out_across_blocks)
{
    // an engine the new one could not take the notes over from: its output ramps to silence
    FadeOut f;
    CHECK(!f.active());
    f.start(100);
    CHECK(f.active());
    std::vector<float> l(256, 0.0f), r(256, 0.0f);
    const std::vector<float> one(256, 1.0f);
    float last = 2.0f;
    uint32_t done = 0;
    for (uint32_t block : { 30u, 30u, 30u, 30u }) {
        std::fill(l.begin(), l.end(), 0.0f);
        std::fill(r.begin(), r.end(), 0.0f);
        f.addTo(one.data(), one.data(), l.data(), r.data(), block);
        for (uint32_t i = 0; i < block; ++i) {
            CHECK(l[i] == r[i] && l[i] <= last && l[i] >= 0.0f); // falling, both channels alike
            last = l[i];
            if (done + i >= 100)
                CHECK(l[i] == 0.0f);
        }
        done += block;
    }
    CHECK(!f.active() && last == 0.0f);
    std::fill(l.begin(), l.end(), 0.5f);
    f.addTo(one.data(), one.data(), l.data(), r.data(), 10); // over: adds nothing
    CHECK(l[0] == 0.5f);
}

TEST(a_corner_label_is_cut_between_characters)
{
    CHECK(morphCornerLabel("Grand A") == "Grand A");
    CHECK(morphCornerLabel("Fourteen chars") == "Fourteen chars");
    CHECK(morphCornerLabel("Fifteen chars!!") == "Fifteen chars…");
    CHECK(morphCornerLabel("Đàn tranh très longue") == "Đàn tranh trè…"); // 13 characters, not bytes
}

TEST(the_morph_tab_explains_itself)
{
    for (const char* t : { kMorphAddTitle, kMorphPadTitle })
        CHECK(t != nullptr && std::strlen(t) > 0);
    for (const char* t : { kMorphAddDescription, kMorphPadDescription, kMorphPadGestures, kMorphMainDescription,
             kMorphShareDescription, kMorphUpDescription, kMorphDownDescription, kMorphRemoveDescription })
        CHECK(t != nullptr && std::strlen(t) > 20); // a sentence
    CHECK(std::string(kMorphPadGestures).find("Right-click") != std::string::npos); // MIDI learn of X and Y
    CHECK(std::string(kMorphAddDescription).find("0 %") != std::string::npos);      // a partner joins silently
}

// a fake file system for the watches: stamps by path, set by the test
namespace {
struct FakeFiles {
    std::map<std::string, pe::FileStamp> files;
    pe::StampFn fn()
    {
        return [this](const std::string& p) {
            auto it = files.find(p);
            return it == files.end() ? pe::FileStamp {} : it->second;
        };
    }
    void touch(const std::string& p, int64_t t) { files[p] = { t, 100 }; }
};
} // namespace

TEST(reload_waits_for_stable_files)
{
    FakeFiles f;
    f.touch("/i/a/instrument.json", 1);
    f.touch("/i/a/data.bin", 1);
    pe::ReloadWatch w;
    w.loaded({ "/i/a" }, f.fn(), 0.0);
    CHECK(!w.poll(f.fn(), 0.5));
    f.touch("/i/a/data.bin", 2); // the editor writes a candidate
    CHECK(!w.poll(f.fn(), 1.0));
    CHECK(!w.poll(f.fn(), 1.2));
    f.touch("/i/a/instrument.json", 2); // still writing
    CHECK(!w.poll(f.fn(), 1.3));
    CHECK(!w.poll(f.fn(), 1.7)); // 0.4 s stable
    CHECK(w.poll(f.fn(), 1.85)); // 0.55 s stable: reload, once
    CHECK(!w.poll(f.fn(), 2.5));
}

TEST(reload_ignores_unchanged_files)
{
    FakeFiles f;
    f.touch("/i/a/instrument.json", 1);
    f.touch("/i/a/data.bin", 1);
    pe::ReloadWatch w;
    w.loaded({ "/i/a" }, f.fn(), 0.0);
    bool any = false;
    for (int i = 0; i < 20; ++i)
        any = any || w.poll(f.fn(), 0.5 * i);
    CHECK(!any);
}

TEST(reload_sees_a_file_appear)
{
    FakeFiles f;
    f.touch("/i/a/instrument.json", 1);
    pe::ReloadWatch w;
    w.loaded({ "/i/a" }, f.fn(), 0.0); // data.bin missing at load
    CHECK(!w.poll(f.fn(), 1.0));
    f.touch("/i/a/data.bin", 5);
    CHECK(!w.poll(f.fn(), 1.1));
    CHECK(w.poll(f.fn(), 1.7));
}

TEST(reload_ignores_a_missing_instrument)
{
    // an instrument not found loads as an empty folder name: nothing is watched for it (not
    // "/instrument.json" at the root)
    std::vector<std::string> asked;
    const pe::StampFn stamp = [&](const std::string& p) {
        asked.push_back(p);
        return pe::FileStamp {};
    };
    pe::ReloadWatch w;
    w.loaded({ "", "/i/b" }, stamp, 0.0);
    CHECK(!w.poll(stamp, 1.0));
    for (const std::string& p : asked)
        CHECK(p.rfind("/i/b/", 0) == 0);
    CHECK(!asked.empty());
}

TEST(listing_change_rescans)
{
    std::map<std::string, std::vector<std::string>> dirs { { "/p", { "a", "b" } } };
    auto list = [&](const std::string& p) { return dirs[p]; };
    pe::ListingWatch w;
    CHECK(!w.poll({ "/p" }, list)); // the first look
    CHECK(!w.poll({ "/p" }, list));
    dirs["/p"] = { "a", "b", "rolled_preview" };
    CHECK(w.poll({ "/p" }, list));
    CHECK(!w.poll({ "/p" }, list));
    dirs["/p"] = { "b", "rolled_preview" };
    CHECK(w.poll({ "/p" }, list));
}

TEST(a_folder_counts_once_its_instrument_json_is_there)
{
    // the editor makes the preview's folder first and renames instrument.json in last: a rescan
    // before that would leave the folder out of the menu for good
    const std::string root = tempDir("pe-listdirs-");
    const std::string dir = root + "/rolled_preview";
    std::filesystem::create_directory(dir);
    CHECK(pe::listDirs(root).empty());
    if (FILE* f = std::fopen((dir + "/data.bin").c_str(), "w"))
        std::fclose(f);
    CHECK(pe::listDirs(root).empty());
    if (FILE* f = std::fopen((dir + "/instrument.json").c_str(), "w"))
        std::fclose(f);
    CHECK(pe::listDirs(root) == std::vector<std::string> { "rolled_preview" });
    std::filesystem::remove_all(root);
}

TEST(condition_zones_map_the_slider_to_the_wear)
{
    // the voicing (each key's level and hardness): Mint 0, Normal 10 % 0.05, Worn 35 % 0.2, Broken 1
    const auto voicing = [](double s) { return pe::conditionWear(s, kWearVoicing); };
    CHECK(std::fabs(voicing(0) - 0.0) < 1e-12 && std::fabs(voicing(10) - 0.05) < 1e-12);
    CHECK(std::fabs(voicing(35) - 0.2) < 1e-12 && std::fabs(voicing(100) - 1.0) < 1e-12);
    bool rising = true;
    for (double s = 0.5; s <= 100.0; s += 0.5)
        rising = rising && voicing(s) > voicing(s - 0.5);
    CHECK(rising);
    // no jump at a zone's edge (a typed or automated value between zones)
    CHECK(std::fabs(voicing(9.5) - 0.05) < 0.01 && std::fabs(voicing(10.5) - 0.05) < 0.01);
    CHECK(voicing(-5) == 0.0 && voicing(105) == 1.0);
    // every other aspect: none through Normal, then 0.2 at Worn's edge, 1 at Broken's
    for (uint32_t a : { kWearHammers, kWearDampers, kWearAction, kWearUnisons, kWearTuning,
             kWearFalseBeats, kWearStrings, kWearSoundboard }) {
        const auto w = [a](double s) { return pe::conditionWear(s, a); };
        CHECK(w(0) == 0.0 && w(5) == 0.0 && w(10) == 0.0);
        CHECK(std::fabs(w(35) - 0.2) < 1e-12 && std::fabs(w(100) - 1.0) < 1e-12);
        bool up = true;
        for (double s = 10.5; s <= 100.0; s += 0.5)
            up = up && w(s) > w(s - 0.5);
        CHECK(up && w(10.5) < 0.01);
    }
}

TEST(normal_is_the_voicing_alone)
{
    // the default (Normal): each key's level a little uneven, nothing else worn (no added cost)
    std::vector<float> v(kParamCount);
    for (uint32_t i = 0; i < kParamCount; ++i) v[i] = paramInfo(i).def;
    const PeParams p = pe::engineParams(v.data());
    const PeTuning t = pe::engineTuning(v.data());
    CHECK(p.voicing_wear == 0.05f);
    CHECK(p.hammer_wear == 0.0f && p.damper_wear == 0.0f && p.action_wear == 0.0f && p.phantom == 0.0f);
    CHECK(t.unison_wear == 0.0 && t.tuning_wear == 0.0 && t.string_wear == 0.0 && t.board_wear == 0.0
        && t.falsebeat_wear == 0.0);
}

TEST(condition_zones_have_names)
{
    CHECK(std::string(pe::conditionZone(0)) == "Mint" && std::string(pe::conditionZone(0.5f)) == "Mint");
    CHECK(std::string(pe::conditionZone(1)) == "Normal" && std::string(pe::conditionZone(10)) == "Normal");
    CHECK(std::string(pe::conditionZone(10.5f)) == "Worn" && std::string(pe::conditionZone(35)) == "Worn");
    CHECK(std::string(pe::conditionZone(35.5f)) == "Broken" && std::string(pe::conditionZone(100)) == "Broken");
}

TEST(an_aspects_offset_adds_after_the_zones)
{
    std::vector<float> v(kParamCount);
    for (uint32_t i = 0; i < kParamCount; ++i) v[i] = paramInfo(i).def;
    v[kCondition] = 10.0f;
    v[kWearVoicing] = -100.0f;
    CHECK(pe::engineParams(v.data()).voicing_wear == 0.0f);
    v[kCondition] = 100.0f;
    v[kWearVoicing] = 100.0f;
    CHECK(pe::engineParams(v.data()).voicing_wear == 1.0f);
    v[kCondition] = 35.0f;
    v[kWearVoicing] = 10.0f;
    CHECK(std::fabs(pe::engineParams(v.data()).voicing_wear - 0.3f) < 1e-6f);
    // the compiled aspects the same way
    v[kWearTuning] = 0.0f;
    CHECK(std::fabs(pe::engineTuning(v.data()).tuning_wear - 0.2) < 1e-9);
}

TEST(condition_shows_its_zone)
{
    CHECK(paramInfo(kCondition).def == 10.0f);
    const std::string s = formatValue(kCondition, 10.0f);
    CHECK(s.find("10") != std::string::npos && s.find("Normal") != std::string::npos);
    CHECK(formatValue(kCondition, 0.0f).find("Mint") != std::string::npos);
    // the shown text reads back, and a zone's name means its upper edge
    float out = -1.0f;
    CHECK(parseValue(kCondition, s, out) && out == 10.0f);
    CHECK(parseValue(kCondition, "35", out) && out == 35.0f);
    CHECK(parseValue(kCondition, "worn", out) && out == 35.0f);
    CHECK(parseValue(kCondition, "Mint", out) && out == 0.0f);
    std::string desc = paramInfo(kCondition).description;
    CHECK(desc.find("Mint") != std::string::npos && desc.find("Normal") != std::string::npos
        && desc.find("Worn") != std::string::npos && desc.find("Broken") != std::string::npos);
}

TEST(condition_number_zone_and_wear_agree)
{
    // a dragged or automated Condition between whole percents: the shown number, its zone and the
    // wear all follow the rounded value; Mint is wear 0 exactly
    CHECK(formatValue(kCondition, 10.4f) == "10 % · Normal");
    CHECK(formatValue(kCondition, 35.4f) == "35 % · Worn");
    CHECK(formatValue(kCondition, 0.4f) == "0 % · Mint");
    CHECK(paramInfo(kCondition).integer);
    std::vector<float> v(kParamCount);
    for (uint32_t i = 0; i < kParamCount; ++i) v[i] = paramInfo(i).def;
    v[kCondition] = 0.4f;
    CHECK(pe::engineParams(v.data()).voicing_wear == 0.0f);
    v[kCondition] = 10.4f;
    CHECK(pe::engineParams(v.data()).voicing_wear == 0.05f);
}

TEST(the_board_bass_is_a_rebuild_control_on_the_sound_tab)
{
    // dB on the strings' lowest range; 0 the instrument's own board
    CHECK(std::string(paramInfo(kBoardBass).symbol) == "board_bass");
    CHECK(paramInfo(kBoardBass).min == -12.0f && paramInfo(kBoardBass).max == 24.0f && paramInfo(kBoardBass).def == 0.0f
          && std::string(paramInfo(kBoardBass).unit) == "dB" && paramInfo(kBoardBass).automatable);
    CHECK(std::string(paramInfo(kBoardBass).description).find("Rebuilds the instrument.") != std::string::npos);
    // into the tuning: a rebuild, back to the default the same engine
    std::vector<float> v = defaults(0, 0, 0);
    const PeTuning t = engineTuning(v.data());
    CHECK(t.board_bass_db == 0);
    v[kBoardBass] = 6.0f;
    const PeTuning c = engineTuning(v.data());
    CHECK(c.board_bass_db == 6.0 && !sameTuning(c, t));
    v[kBoardBass] = 0.0f;
    CHECK(sameTuning(engineTuning(v.data()), t));
    // on a hammer piano, after Sustain on the Sound tab; not on a harpsichord or the marimba
    CHECK(paramActive(kBoardBass, entryOf(0), 0, 0) && paramActive(kBoardBass, entryOf(kUprA), 1, 0));
    CHECK(!paramActive(kBoardBass, entryOf(kHarpA), 0, 0) && !paramActive(kBoardBass, entryOf(kMar), 0, 0));
    const std::vector<Row> rows = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(0, 0, 0)).rows;
    auto at = [&](uint32_t p) { for (size_t i = 0; i < rows.size(); ++i) if (rows[i].param == p) return (int)i; return -1; };
    CHECK(at(kSustain) >= 0 && at(kBoardBass) == at(kSustain) + 1);
    const std::vector<Row> harp = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(kHarpA, 0, 0)).rows;
    for (const Row& r : harp)
        CHECK(r.param != kBoardBass);
}

// ---- presets (Presets.hpp, spec 2026-10-27-presets-design)

TEST(shipped_full_presets_name_exported_instruments)
{
    // the shipped full presets (Default, then plugin/presets, embedded): an exported instrument each, every value a known
    // parameter's and in range (none clamped), and they are the plugin's programs
    const std::vector<const pe::Preset*> full = pe::shippedFullPresets();
    CHECK(full.size() >= 3);
    // Default first: nothing of its own (every parameter and state at its default, the menu's first instrument)
    CHECK(full[0]->name == "Default" && full[0]->shipped && full[0]->values.empty() && full[0]->states.empty());
    for (const pe::Preset* p : full) {
        if (p == full[0])
            continue;
        CHECK(p->shipped && !p->name.empty());
        std::string inst;
        for (const auto& [k, v] : p->states)
            if (k == "instrument")
                inst = v;
        const std::string json = std::string(PE_INSTRUMENTS_DIR) + "/" + inst + "/instrument.json";
        CHECK(!inst.empty() && access(json.c_str(), R_OK) == 0);
        const std::vector<std::pair<uint32_t, float>> ps = pe::presetParams(*p);
        CHECK(ps.size() == pe::kParamCount);
        size_t set = 0;
        for (const auto& [sym, x] : p->values)
            for (const auto& [i, y] : ps)
                if (sym == pe::paramInfo(i).symbol) {
                    CHECK(y == x);
                    ++set;
                }
        CHECK(set == p->values.size());
    }
}

TEST(a_full_preset_starts_from_the_defaults)
{
    // a full preset sets every parameter and state: what its file leaves out, at the default
    pe::Preset p;
    p.name = "x";
    p.scope = pe::PresetScope::All;
    p.values = { { "dynamics", 25.0f }, { "no_such_symbol", 1.0f } };
    p.states = { { "instrument", "somewhere" } };
    const std::vector<std::pair<uint32_t, float>> ps = pe::presetParams(p);
    CHECK(ps.size() == pe::kParamCount);
    for (const auto& [i, x] : ps)
        CHECK(x == (i == pe::kDynamics ? 25.0f : pe::paramInfo(i).def));
    const std::vector<std::pair<std::string, std::string>> st = pe::presetStates(p);
    CHECK(st.size() == 5 && st[0].first == "instrument" && st[0].second == "somewhere");
    for (size_t k = 1; k < st.size(); ++k)
        CHECK(st[k].second.empty());
    // a microphone preset: its own values, no states
    p.scope = pe::PresetScope::Mics;
    p.values = { { "mic_position", 2.0f }, { "dynamics", 25.0f } };
    CHECK(pe::presetParams(p).size() == 1 && pe::presetStates(p).empty());
}

TEST(shipped_mic_presets_are_valid_setups)
{
    // every shipped microphone preset sets microphones only, in range, and its Custom pair is one the engine
    // keeps as it is
    CHECK(!pe::shippedPresets().empty());
    for (const pe::Preset& p : pe::shippedPresets()) {
        if (p.scope == pe::PresetScope::All)
            continue; // shipped_full_presets_name_exported_instruments
        CHECK(p.scope == pe::PresetScope::Mics && p.shipped && (p.body == 0 || p.body == 1));
        const std::vector<std::pair<uint32_t, float>> ps = pe::presetParams(p);
        CHECK(ps.size() == p.values.size()); // every symbol known and a microphone's
        float v[pe::kParamCount] = {};
        for (uint32_t i = 0; i < pe::kParamCount; ++i)
            v[i] = pe::paramInfo(i).def;
        for (const auto& [i, x] : ps) {
            CHECK(x >= pe::paramInfo(i).min && x <= pe::paramInfo(i).max);
            v[i] = x;
        }
        CHECK(v[pe::kMicPosition] == 5.0f);
        PeMicSetup s {};
        for (int m = 0; m < 2; ++m) {
            const uint32_t b = m == 0 ? pe::kMicLX : pe::kMicRX, a = pe::kMicLPattern + 3 * m;
            for (int k = 0; k < 3; ++k)
                s.pos[m][k] = v[b + k];
            s.pattern[m] = v[a] / 100.0;
            s.yaw[m] = v[a + 1];
            s.pitch[m] = v[a + 2];
        }
        s.wall = v[pe::kMicWall];
        // kept as it is (to float rounding: the parameters are floats; the grand pins its wall, which it ignores)
        PeMicSetup c = s;
        pe_mic_clamp(p.body, 1.0, &c);
        bool same = std::fabs(c.wall - s.wall) < 1e-6;
        for (int m = 0; m < 2; ++m) {
            for (int k = 0; k < 3; ++k)
                same = same && std::fabs(c.pos[m][k] - s.pos[m][k]) < 1e-6;
            same = same && std::fabs(c.pattern[m] - s.pattern[m]) < 1e-6 && std::fabs(c.yaw[m] - s.yaw[m]) < 1e-6
                && std::fabs(c.pitch[m] - s.pitch[m]) < 1e-6;
        }
        if (!same)
            std::fprintf(stderr, "  %s (%s): clamped\n", p.name.c_str(), pe::bodyName(p.body));
        CHECK(same);
    }
}

TEST(a_preset_reads_back_from_its_file)
{
    pe::Preset p;
    p.name = "Late night = soft";
    p.scope = pe::PresetScope::All;
    p.values = { { "volume", -6.5f }, { "mic_l_x", 0.125f } };
    p.states = { { "instrument", "grand_a" }, { "velocity_curve", "0 0\n64 50\\x\n127 127" }, { "ui", "tab=1" } };
    pe::Preset q;
    CHECK(pe::parsePreset(pe::formatPreset(p), q));
    CHECK(q.name == "Late night = soft" && q.scope == pe::PresetScope::All);
    CHECK(q.values == p.values);
    CHECK(q.states.size() == 2 && q.states[1].second == "0 0\n64 50\\x\n127 127"); // as written; the UI's state dropped
    pe::Preset m;
    m.name = "Close";
    m.body = 1;
    m.values = { { "mic_position", 3.0f } };
    CHECK(pe::parsePreset(pe::formatPreset(m), q) && q.scope == pe::PresetScope::Mics && q.body == 1);
    CHECK(!pe::parsePreset("name = x\n", q));                  // no scope
    CHECK(!pe::parsePreset("name = x\nscope = mics\n", q));    // a mic preset without a body
    CHECK(!pe::parsePreset("scope = all\n", q));               // no name
}

TEST(the_plugin_is_named_dussek)
{
    // the name, the maker and the IDs under ccoors.de (fixed from the first release on)
    CHECK(std::string(DISTRHO_PLUGIN_NAME) == "Dussek");
    CHECK(std::string(DISTRHO_PLUGIN_BRAND) == "Christian Friedrich Coors");
    CHECK(std::string(DISTRHO_PLUGIN_URI) == "https://ccoors.de/plugins/dussek");
    CHECK(std::string(DISTRHO_PLUGIN_CLAP_ID) == "de.ccoors.dussek");
}

TEST(user_presets_save_load_and_delete)
{
    const std::string tmpl = tempDir("pe-presets-");
    CHECK(!tmpl.empty());
    const std::string dir = tmpl + "/dussek/presets"; // made with its parents
    float v[pe::kParamCount] = {};
    for (uint32_t i = 0; i < pe::kParamCount; ++i)
        v[i] = pe::paramInfo(i).def;
    v[pe::kMicPosition] = 5;
    v[pe::kMicLX] = -0.5f;
    v[pe::kVolume] = 3;
    const pe::Preset mine = pe::presetFrom("My/close: pair", pe::PresetScope::Mics, 0, v, {});
    CHECK(std::all_of(mine.values.begin(), mine.values.end(), [](const auto& kv) { return kv.first != "volume"; }));
    std::string err;
    CHECK(pe::saveUserPreset(dir, mine, err) && err.empty());
    CHECK(pe::presetFile(dir, "My/close: pair") == dir + "/My_close_ pair.dussek-preset");
    const pe::Preset all = pe::presetFrom("Everything", pe::PresetScope::All, 0, v, { { "instrument", "grand_b" }, { "ui", "tab=2" } });
    CHECK(pe::saveUserPreset(dir, all, err));
    std::vector<pe::Preset> user = pe::loadUserPresets(dir);
    CHECK(user.size() == 2 && user[0].name == "Everything" && user[1].name == "My/close: pair");
    { // a file of the old extension is not a preset
        std::ofstream old(dir + "/Old.pepreset");
        old << "# a preset\nname = Old\nscope = all\n";
    }
    CHECK(pe::loadUserPresets(dir).size() == 2);
    CHECK(user[0].states.size() == 1 && user[0].states[0].second == "grand_b"); // the UI's state not a preset's
    // offered: the grand's shipped first, then the user's; an upright sees none of the grand's
    const std::vector<const pe::Preset*> grand = pe::presetsFor(pe::PresetScope::Mics, 0, user);
    CHECK(grand.size() == 5 && grand[0]->shipped && grand.back()->name == "My/close: pair");
    const std::vector<const pe::Preset*> upright = pe::presetsFor(pe::PresetScope::Mics, 1, user);
    CHECK(upright.size() == 3 && std::all_of(upright.begin(), upright.end(), [](const pe::Preset* p) { return p->shipped; }));
    const std::vector<const pe::Preset*> full = pe::presetsFor(pe::PresetScope::All, 7, user);
    CHECK(full.size() == pe::shippedFullPresets().size() + 1 && full.back()->name == "Everything"); // the shipped first
    // the values still the preset's: it is the one shown; one moved: none
    CHECK(pe::matchingPreset(grand, v) == 4);
    v[pe::kMicLX] = -0.4f;
    CHECK(pe::matchingPreset(grand, v) == -1);
    for (const auto& [i, x] : pe::presetParams(*grand[0]))
        v[i] = x;
    CHECK(pe::matchingPreset(grand, v) == 0);
    CHECK(pe::deleteUserPreset(user[1]) && pe::loadUserPresets(dir).size() == 1);
    CHECK(!pe::deleteUserPreset(*grand[0])); // a shipped one stays
    std::filesystem::remove_all(tmpl);
}

TEST(a_mic_preset_sets_microphones_only)
{
    // a file's other parameters and unknown symbols are skipped, a value out of range is clamped
    pe::Preset p;
    p.name = "Odd";
    p.body = 0;
    p.values = { { "volume", 12.0f }, { "no_such_knob", 1.0f }, { "mic_l_z", 99.0f }, { "mic_position", 2.0f } };
    const std::vector<std::pair<uint32_t, float>> ps = pe::presetParams(p);
    CHECK(ps.size() == 2 && ps[0].first == pe::kMicLZ && ps[0].second == pe::paramInfo(pe::kMicLZ).max);
    CHECK(ps[1].first == pe::kMicPosition && ps[1].second == 2.0f);
}

TEST(a_preset_rows_menu_offers_save_and_deletes_only_the_users)
{
    pe::Preset mine;
    mine.name = "Mine";
    const std::vector<const pe::Preset*> offered { &pe::shippedPresets()[0], &mine };
    pe::PresetMenu m = pe::presetMenu(offered, 0, false);
    CHECK(m.items.size() == 3 && m.items[2] == "Save as…" && m.actions[2] == pe::kPresetSaveAs); // a shipped one: no delete
    m = pe::presetMenu(offered, 1, false);
    CHECK(m.items.size() == 4 && m.actions[3] == pe::kPresetDelete && m.items[3] == "Delete “Mine”…");
    CHECK(pe::presetMenu(offered, 1, true).items[3] == "Delete “Mine”: click again to confirm");
    CHECK(pe::presetMenu(offered, -1, false).items.size() == 3);
    CHECK(pe::presetLabel(offered, 1) == "Mine" && pe::presetLabel(offered, -1) == "None");
}

TEST(listing_text_names_each_instrument_once)
{
    // `dussek list`: one line per instrument in the menu's order (id, group, name, folder), then the
    // catalog's messages; a shadowed copy only as a message
    TempTree t;
    t.add("a", "harp", "harp", R"({"name": "Harp A", "group": "Harps"})");
    t.add("a", "grand", "grand_a", R"({"name": "Grand A", "group": "Grand pianos"})");
    t.add("b", "harp", "harp", R"({"name": "Harp A, old", "group": "Harps"})");
    const Catalog c = Catalog::scan({ t.path("a"), t.path("b") });
    const std::string text = listingText(c);
    std::vector<std::string> lines;
    for (size_t at = 0, end; at < text.size(); at = end + 1) {
        end = std::min(text.find('\n', at), text.size());
        lines.push_back(text.substr(at, end - at));
    }
    CHECK(lines.size() == 3);
    CHECK(lines.size() == 3 && lines[0] == "grand\tGrand pianos\tGrand A\t" + t.path("a") + "/grand");
    CHECK(lines.size() == 3 && lines[1] == "harp\tHarps\tHarp A\t" + t.path("a") + "/harp");
    CHECK(lines.size() == 3 && lines[2].rfind("# ", 0) == 0 && lines[2].find(t.path("b")) != std::string::npos);
    CHECK(listingText(Catalog::scan({ t.path("none") })).empty());
}

TEST(the_standalone_reads_the_instruments_beside_it)
{
    // an archive unpacked anywhere: the standalone (and only it, not a host running the plugin)
    // reads the instruments folder beside it; on macOS beside its app
    CHECK(pe::portableFolder("/x/dussek-0.3.0-linux/dussek") == "/x/dussek-0.3.0-linux");
    CHECK(pe::portableFolder("C:\\x\\dussek-0.3.0-windows\\dussek.exe") == "C:\\x\\dussek-0.3.0-windows");
    CHECK(pe::portableFolder("C:/x/d/Dussek.exe") == "C:/x/d");
    CHECK(pe::portableFolder("/x/m/Dussek.app/Contents/MacOS/dussek") == "/x/m");
    CHECK(pe::portableFolder("/usr/bin/ardour8").empty());
    CHECK(pe::portableFolder("C:\\Program Files\\REAPER\\reaper.exe").empty());
    CHECK(pe::portableFolder("dussek").empty() && pe::portableFolder("").empty());
}

TEST(system_data_dirs_per_platform)
{
    // the folders an installer fills (the instruments, once), from an environment and a platform
    using pe::Platform;
    using V = std::vector<std::string>;
    const auto env = [](std::map<std::string, std::string> m) {
        return [m](const char* k) -> const char* {
            auto it = m.find(k);
            return it == m.end() ? nullptr : it->second.c_str();
        };
    };
    CHECK(pe::systemDataDirs(env({{"PROGRAMDATA", "C:\\ProgramData"}}), Platform::Windows) == V{"C:\\ProgramData/Dussek"});
    CHECK(pe::systemDataDirs(env({}), Platform::Windows).empty());
    CHECK(pe::systemDataDirs(env({{"HOME", "/Users/u"}}), Platform::MacOS) == V{"/Library/Application Support/Dussek"});
    CHECK(pe::systemDataDirs(env({}), Platform::Linux) == (V{"/usr/local/share/dussek", "/usr/share/dussek"}));
    CHECK(pe::systemDataDirs(env({{"XDG_DATA_DIRS", ""}}), Platform::Linux) == (V{"/usr/local/share/dussek", "/usr/share/dussek"}));
    // the XDG spec: relative entries are ignored
    CHECK(pe::systemDataDirs(env({{"XDG_DATA_DIRS", "/opt/a:rel:/usr/share/"}}), Platform::Linux) == (V{"/opt/a/dussek", "/usr/share//dussek"}));
    CHECK(pe::systemDataDirs(env({{"XDG_DATA_DIRS", "rel"}}), Platform::Linux) == (V{"/usr/local/share/dussek", "/usr/share/dussek"}));
}

TEST(search_paths_put_the_installed_before_the_users)
{
    // an installer's instruments come before the user folder's: an old copy there (a test build's)
    // does not hide them; the user's own instruments have names of their own
    TempTree t;
    const bool mac = pe::hostPlatform() == pe::Platform::MacOS;
    const char* sysVar = pe::hostPlatform() == pe::Platform::Windows ? "PROGRAMDATA" : "XDG_DATA_DIRS";
    const char* userVar = pe::hostPlatform() == pe::Platform::Linux ? "XDG_DATA_HOME" : mac ? "HOME" : "APPDATA";
    const char* sysWas = std::getenv(sysVar);
    const char* userWas = std::getenv(userVar);
    const char* envWas = std::getenv("DUSSEK_INSTRUMENTS");
    const std::string sysOld = sysWas ? sysWas : "", userOld = userWas ? userWas : "", envOld = envWas ? envWas : "";
    unsetEnv("DUSSEK_INSTRUMENTS");
    setEnv(sysVar, t.path("sys").c_str());
    setEnv(userVar, t.path("user").c_str());
    const std::string sys = pe::systemDataDirs().front();
    const std::string user = pe::dataDirs().data;
    const std::vector<std::string> p = Catalog::searchPaths(nullptr);
    const auto at = [&](const std::string& dir) { return std::find(p.begin(), p.end(), dir) - p.begin(); };
    CHECK(at(sys + "/instruments") < at(user + "/instruments"));
    CHECK(at(user + "/instruments") < (long)p.size() && p.back() == user + "/snapshots");
    if (!mac) { // the macOS folder is fixed (/Library/…): not one a test can fill
        t.add(("sys" + sys.substr(t.path("sys").size()) + "/instruments").c_str(), "harp", "harp", R"({"name": "Harp A", "group": "Harps"})");
        t.add(("user" + user.substr(t.path("user").size()) + "/instruments").c_str(), "harp", "harp", R"({"name": "Harp A, old", "group": "Harps"})");
        const Catalog c = Catalog::scan(p);
        CHECK(c.find("harp") != nullptr && c.find("harp")->dir.rfind(sys, 0) == 0);
        bool named = false;
        for (const std::string& m : c.messages())
            named = named || (m.find("shadowed") != std::string::npos && m.find(user) != std::string::npos);
        CHECK(named);
    }
    if (sysWas) setEnv(sysVar, sysOld.c_str()); else unsetEnv(sysVar);
    if (userWas) setEnv(userVar, userOld.c_str()); else unsetEnv(userVar);
    if (envWas) setEnv("DUSSEK_INSTRUMENTS", envOld.c_str());
}

TEST(data_dirs_per_platform)
{
    // the folders for your own files (presets, MIDI maps, instruments you add, snapshots), from an
    // environment and a platform; Linux's as before
    using pe::Platform;
    const auto env = [](std::map<std::string, std::string> m) {
        return [m](const char* k) -> const char* {
            auto it = m.find(k);
            return it == m.end() ? nullptr : it->second.c_str();
        };
    };
    auto d = pe::dataDirs(env({{"HOME", "/hm/u"}}), Platform::Linux);
    CHECK(d.data == "/hm/u/.local/share/dussek" && d.config == "/hm/u/.config/dussek");
    d = pe::dataDirs(env({{"HOME", "/hm/u"}, {"XDG_DATA_HOME", "/x/d"}, {"XDG_CONFIG_HOME", "/x/c"}}), Platform::Linux);
    CHECK(d.data == "/x/d/dussek" && d.config == "/x/c/dussek");
    // a relative XDG folder is not one (the XDG spec): the default instead
    d = pe::dataDirs(env({{"HOME", "/hm/u"}, {"XDG_DATA_HOME", "rel"}, {"XDG_CONFIG_HOME", "rel"}}), Platform::Linux);
    CHECK(d.data == "/hm/u/.local/share/dussek" && d.config == "/hm/u/.config/dussek");
    d = pe::dataDirs(env({}), Platform::Linux);
    CHECK(d.data.empty() && d.config.empty());
    d = pe::dataDirs(env({{"HOME", "/Users/u"}, {"XDG_DATA_HOME", "/x/d"}}), Platform::MacOS);
    CHECK(d.data == "/Users/u/Library/Application Support/Dussek" && d.config == d.data);
    d = pe::dataDirs(env({{"APPDATA", "C:\\Users\\u\\AppData\\Roaming"}, {"HOME", "/c/Users/u"}}), Platform::Windows);
    CHECK(d.data == "C:\\Users\\u\\AppData\\Roaming/Dussek" && d.config == d.data);
    d = pe::dataDirs(env({{"HOME", "/c/Users/u"}}), Platform::Windows);
    CHECK(d.data.empty());
    // the process's own: the readers agree with it
    const pe::DataDirs here = pe::dataDirs();
    if (!here.data.empty()) {
        CHECK(Catalog::searchPaths(nullptr).back() == here.data + "/snapshots");
        CHECK(presetDir() == here.data + "/presets");
        CHECK(midiMapPath() == here.config + "/midi-map.txt");
    }
}

TEST(parse_float_as_from_chars_without_it)
{
    // std::from_chars for floating point is missing from older macOS libc++: pe::parseFloat reads the
    // same way (a point whatever the locale, a prefix, no leading '+' or space)
    float x = 0;
    const char a[] = "1.5x";
    CHECK(pe::parseFloat(a, a + 4, x) == a + 3 && x == 1.5f);
    const char b[] = "-2e3";
    CHECK(pe::parseFloat(b, b + 4, x) == b + 4 && x == -2000.0f);
    const char c[] = "+1";
    CHECK(pe::parseFloat(c, c + 2, x) == nullptr);
    const char d[] = " 1";
    CHECK(pe::parseFloat(d, d + 2, x) == nullptr);
    const char e[] = "abc";
    CHECK(pe::parseFloat(e, e + 3, x) == nullptr);
    const char g[] = "1.5abc"; // libc++ took the letters of a hex float into the number
    CHECK(pe::parseFloat(g, g + 6, x) == g + 3 && x == 1.5f);
    const char h[] = "2e";      // an exponent without digits: the number before it
    CHECK(pe::parseFloat(h, h + 2, x) == h + 1 && x == 2.0f);
    const char i[] = "-.";
    CHECK(pe::parseFloat(i, i + 2, x) == nullptr);
    const char f[] = "0.25";
    CHECK(pe::parseFloat(f, f + 2, x) == f + 2 && x == 0.0f); // only the range given
    const char* old = std::setlocale(LC_NUMERIC, nullptr);
    const std::string saved = old != nullptr ? old : "C";
    if (std::setlocale(LC_NUMERIC, "de_DE.UTF-8") != nullptr || std::setlocale(LC_NUMERIC, "de_DE.utf8") != nullptr) {
        CHECK(pe::parseFloat(a, a + 4, x) == a + 3 && x == 1.5f);
        std::setlocale(LC_NUMERIC, saved.c_str());
    }
}

TEST(saving_again_replaces_the_file)
{
    // the presets and the MIDI map are written to a temporary file and renamed over the old one;
    // std::rename does not replace an existing file on Windows
    const std::filesystem::path dir = std::filesystem::temp_directory_path() / ("pe-replace-" + std::to_string(pe::SharedRegion::randomToken()));
    const std::string presets = (dir / "dussek" / "presets").string();
    float v[pe::kParamCount] = {};
    for (uint32_t i = 0; i < pe::kParamCount; ++i)
        v[i] = pe::paramInfo(i).def;
    std::string err;
    CHECK(pe::saveUserPreset(presets, pe::presetFrom("Twice", pe::PresetScope::Mics, 0, v, {}), err) && err.empty());
    v[pe::kMicSpacing] = 70;
    CHECK(pe::saveUserPreset(presets, pe::presetFrom("Twice", pe::PresetScope::Mics, 0, v, {}), err) && err.empty());
    const std::vector<pe::Preset> user = pe::loadUserPresets(presets);
    CHECK(user.size() == 1);
    MidiMap m;
    const std::string map = (dir / "config" / "midi-map.txt").string();
    CHECK(m.save(map) && m.save(map));
    std::filesystem::remove_all(dir);
}

TEST(the_standalones_audio_row_shows_only_on_its_own_audio)
{
    // the standalone without JACK (DPF's native audio: CoreAudio, WASAPI) has a row of its own on the
    // MIDI tab: the buffer size, and a rescan of the MIDI inputs; a host or JACK shows none
    UiContext c = ctxOf(0, 1, 0);
    for (float w : { kMinWidth, kDefaultWidth, 1600.0f }) {
        const Layout off = computeLayout(w, kDefaultHeight, kTabMidi, c);
        CHECK(off.rescanMidi.w == 0 && off.bufferChips[0].w == 0);
        c.nativeAudio = true;
        const Layout on = computeLayout(w, kDefaultHeight, kTabMidi, c);
        c.nativeAudio = false;
        CHECK(on.rescanMidi.w > 0 && on.rescanButton.w > 0 && !on.rescanMidi.overlaps(on.takeoverSwitch));
        CHECK(on.rescanButton.x >= on.rescanMidi.x && on.rescanButton.x + on.rescanButton.w <= on.rescanMidi.x + on.rescanMidi.w + 0.5f);
        for (int i = 0; i < kBufferSizeCount; ++i) {
            const Rect& b = on.bufferChips[i];
            CHECK(b.w > 0 && b.x >= on.content.x && b.x + b.w <= on.content.x + on.content.w + 0.5f);
            CHECK(!b.overlaps(on.takeoverSwitch) && !b.overlaps(on.rescanMidi));
            for (int j = i + 1; j < kBufferSizeCount; ++j)
                CHECK(!b.overlaps(on.bufferChips[j]));
        }
    }
    c.nativeAudio = true;
    for (Tab t : { kTabSound, kTabExtras })
        CHECK(computeLayout(kDefaultWidth, kDefaultHeight, t, c).rescanMidi.w == 0);
    CHECK(kBufferSizes[0] == 128 && kBufferSizes[kBufferSizeCount - 1] == 1024);
}

TEST(settings_line_is_copy_cli_for_named_controls)
{
    // the website's clips name plugin controls by symbol: the line is Copy CLI's for the defaults
    // with those set (what the plugin plays), an unknown symbol an error naming it
    std::vector<float> v(pe::kParamCount);
    for (uint32_t i = 0; i < pe::kParamCount; ++i)
        v[i] = pe::paramInfo(i).def;
    std::string error;
    CHECK(pe::settingsLine("/i/grand_a", {}, error) == midi2wavLine(v.data(), "/i/grand_a", 48000) && error.empty());
    const std::string line = pe::settingsLine("/i/grand_a", { { "audio_input", 0.0f }, { "sustain_latch", 1.0f } }, error);
    CHECK(error.empty() && line.find("input=1") != std::string::npos && line.find("--latch") != std::string::npos);
    const std::string worn = pe::settingsLine("/i/grand_a", { { "condition", 35.0f } }, error);
    CHECK(error.empty() && worn != pe::settingsLine("/i/grand_a", {}, error));
    CHECK(pe::settingsLine("/i/grand_a", { { "nonesuch", 1.0f } }, error).empty() && error.find("nonesuch") != std::string::npos);
}

TEST(copy_cli_finds_the_standalone)
{
    // Copy CLI's program: the standalone's own path in the standalone; in a plugin the standalone next
    // to its bundle (a release's folder, or plugin/build/bin behind `just install`'s links), else on
    // PATH; empty: none found (the line then says plain `dussek`)
    namespace fs = std::filesystem;
    const fs::path root = tempDir("pe-standalone-");
    const auto file = [](const fs::path& p) {
        fs::create_directories(p.parent_path());
        std::FILE* f = std::fopen(p.string().c_str(), "w");
        if (f != nullptr)
            std::fclose(f);
    };
#ifdef _WIN32
    const char* exe = "dussek.exe";
    const char sep = ';';
#else
    const char* exe = "dussek";
    const char sep = ':';
#endif
    const fs::path bin = root / "build" / "bin";
    file(bin / exe);
    file(bin / "dussek.lv2" / "dussek_dsp.so");
    file(bin / "dussek.vst3" / "Contents" / "x86_64-linux" / "dussek.so");
    file(bin / "dussek.clap");
    const auto same = [](const std::string& a, const fs::path& b) { std::error_code ec; return !a.empty() && fs::equivalent(a, b, ec); };
    CHECK(pe::standaloneExecutable((bin / exe).string(), true, nullptr) == (bin / exe).string());
    CHECK(same(pe::standaloneExecutable((bin / "dussek.lv2" / "dussek_dsp.so").string(), false, nullptr), bin / exe));
    CHECK(same(pe::standaloneExecutable((bin / "dussek.vst3" / "Contents" / "x86_64-linux" / "dussek.so").string(), false, nullptr), bin / exe));
    CHECK(same(pe::standaloneExecutable((bin / "dussek.clap").string(), false, nullptr), bin / exe));
#ifndef _WIN32
    // `just install`: ~/.lv2/dussek.lv2 a link to the build's bundle
    fs::create_directories(root / "home" / ".lv2");
    std::error_code ec;
    fs::create_directory_symlink(bin / "dussek.lv2", root / "home" / ".lv2" / "dussek.lv2", ec);
    if (!ec)
        CHECK(same(pe::standaloneExecutable((root / "home" / ".lv2" / "dussek.lv2" / "dussek_dsp.so").string(), false, nullptr), bin / exe));
#endif
#ifndef _WIN32
    // macOS: the VST3's binary is named dussek too; the standalone is the .app's (Windows has no .app)
    const fs::path mac = root / "mac";
    file(mac / "dussek.vst3" / "Contents" / "MacOS" / "dussek");
    file(mac / "dussek.app" / "Contents" / "MacOS" / "dussek");
    CHECK(same(pe::standaloneExecutable((mac / "dussek.vst3" / "Contents" / "MacOS" / "dussek").string(), false, nullptr),
        mac / "dussek.app" / "Contents" / "MacOS" / "dussek"));
#endif
    // a package's layout: the plugin in lib/lv2, the program on PATH
    const fs::path pkg = root / "pkg";
    file(pkg / "lib" / "lv2" / "dussek.lv2" / "dussek_dsp.so");
    file(pkg / "bin" / exe);
    const std::string path = (root / "nowhere").string() + sep + (pkg / "bin").string();
    const std::string plugin = (pkg / "lib" / "lv2" / "dussek.lv2" / "dussek_dsp.so").string();
    CHECK(same(pe::standaloneExecutable(plugin, false, path.c_str()), pkg / "bin" / exe));
    CHECK(pe::standaloneExecutable(plugin, false, nullptr).empty());
    CHECK(pe::standaloneExecutable("", false, nullptr).empty());
    // an installer's layout: the plugin in the system's plugin folder, the program in its own
    // (Windows: Program Files\Dussek; macOS: /Applications, the app named Dussek.app)
    const fs::path apps = root / "apps";
    const fs::path inApp = apps / (pe::hostPlatform() == pe::Platform::Windows ? fs::path(exe) : fs::path("Dussek.app") / "Contents" / "MacOS" / "dussek");
    file(inApp);
    file(root / "sys" / "LV2" / "dussek.lv2" / "dussek_dsp.so");
    const std::string sysPlugin = (root / "sys" / "LV2" / "dussek.lv2" / "dussek_dsp.so").string();
    CHECK(pe::standaloneExecutable(sysPlugin, false, nullptr).empty());
    CHECK(same(pe::standaloneExecutable(sysPlugin, false, nullptr, { apps.string() }), inApp));
    fs::remove_all(root);
    // the line starts with it, quoted as the instrument's path is
    std::vector<float> v(pe::kParamCount);
    for (uint32_t i = 0; i < pe::kParamCount; ++i)
        v[i] = pe::paramInfo(i).def;
    CHECK(midi2wavLine(v.data(), "/i/grand_a", 48000).rfind("dussek render /i/grand_a ", 0) == 0);
    CHECK(midi2wavLine(v.data(), "/i/grand_a", 48000, nullptr, {}, std::string(), std::string(), "/opt/my apps/dussek")
              .rfind("'/opt/my apps/dussek' render /i/grand_a ", 0) == 0);
}

TEST(about_is_the_last_tab)
{
    CHECK(std::string(tabName(kTabAbout)) == "About" && kTabAbout + 1 == kTabCount);
    CHECK(tabGroups(kTabAbout).empty()); // no parameters
    // the default window keeps the ten tabs on one row (the drawing gives up a little width for it)
    const Layout d = computeLayout(kDefaultWidth, kDefaultHeight, kTabSound, ctxOf(0, 1, 0));
    CHECK(d.tabs.size() == kTabCount && d.tabs.back().rect.y == d.tabs.front().rect.y);
    CHECK(d.topView.w + d.frontView.w + kGap >= 0.5f * (kDefaultWidth - 3 * kPad) - 0.5f);
    // a wide window keeps the drawing's share as before
    const Layout big = computeLayout(1600, kDefaultHeight, kTabSound, ctxOf(0, 1, 0));
    CHECK_NEAR(big.topView.w + big.frontView.w + kGap, 0.55f * (1600 - 3 * kPad), 0.5);
}

TEST(the_version_and_the_commit_are_known)
{
    const Version v = pluginVersion();
    CHECK(versionText() == std::to_string(v.major) + "." + std::to_string(v.minor) + "." + std::to_string(v.patch));
    CHECK(v.major + v.minor + v.patch > 0);
    const std::string c = buildCommit();
    CHECK(!c.empty() && c.find('\n') == std::string::npos && c.find('"') == std::string::npos);
}

TEST(markdown_becomes_plain_lines)
{
    const std::string md = "\n# License: Grand A (`grand_a`)\r\n\r\nThis instrument was made by **someone**.\nIt is licensed.\n\n\n"
                           "- **Set A** by X, https://a.example/: CC0.\n- **Set B** by Y\n  continued here.\n\n"
                           "## Credit\nCredit it as: A.\n\n";
    const std::vector<TextLine> want {
        { "License: Grand A (grand_a)", true }, { "", false },
        { "This instrument was made by someone. It is licensed.", false }, { "", false },
        { "• Set A by X, https://a.example/: CC0.", false }, { "• Set B by Y continued here.", false }, { "", false },
        { "Credit", true }, { "Credit it as: A.", false } };
    const std::vector<TextLine> got = markdownLines(md);
    CHECK(got == want);
    CHECK(markdownLines("").empty() && markdownLines("\n\n").empty());
    CHECK((markdownLines("a *b* c") == std::vector<TextLine> { { "a *b* c", false } })); // only ** goes
}

TEST(lines_wrap_to_the_width)
{
    const std::string url = "https://theremin.music.uiowa.edu/MISpiano.html";
    const std::vector<TextLine> in { { "License: Grand A (grand_a)", true }, { "", false },
        { "It is licensed under the Creative Commons Attribution 4.0 International license, " + url + ".", false },
        { "• Salamander Grand Piano V3 by Alexander Holm: public domain.", false } };
    for (float w : { 120.0f, 200.0f, 380.0f, 1000.0f }) {
        const std::vector<TextLine> out = wrapLines(in, w);
        std::string words, back;
        for (const TextLine& l : in) words += l.text + " ";
        bool blank = false;
        for (const TextLine& l : out) {
            CHECK(textWidth(l.text.c_str()) <= w);
            back += l.text + " ";
            blank = blank || l.text.empty();
        }
        CHECK(blank); // the blank line stays
        // the same words in the same order (the breaks and the list's indent aside)
        auto squeeze = [](const std::string& s) { std::string r; for (char c : s) if (c != ' ') r += c; return r; };
        CHECK(squeeze(back) == squeeze(words));
        CHECK(out.front().heading && out.front().text.rfind("License", 0) == 0);
        if (w >= 1000) CHECK(out.size() == in.size()); // nothing to wrap
    }
    // at 200 px: the URL (47 characters, 329 px) is broken, the list item's further lines indented
    const std::vector<TextLine> narrow = wrapLines(in, 200);
    CHECK(narrow.size() > in.size() + 3);
    size_t item = 0;
    for (size_t i = 0; i < narrow.size(); ++i)
        if (narrow[i].text.rfind("• ", 0) == 0) item = i;
    CHECK(item > 0 && item + 1 < narrow.size() && narrow[item + 1].text.rfind("  ", 0) == 0);
    // a heading wraps as a heading; no width still ends, a character a line
    const std::vector<TextLine> h = wrapLines({ { "A long heading that wraps", true } }, 80);
    CHECK(h.size() > 1 && std::all_of(h.begin(), h.end(), [](const TextLine& l) { return l.heading; }));
    const std::vector<TextLine> tiny = wrapLines({ { "abc def", false } }, 1);
    CHECK(tiny.size() == 6);
    CHECK(wrapLines({ { "ä ö ü", false } }, 7).size() == 3); // UTF-8: characters, not bytes
}

TEST(every_export_carries_a_license_the_about_tab_reads)
{
    for (const InstrumentEntry* e : committed().menu()) {
        std::string md;
        CHECK(readTextFile(e->dir + "/LICENSE.md", md));
        const std::vector<TextLine> lines = markdownLines(md);
        CHECK(!lines.empty() && lines.front().heading && lines.front().text.find(e->name) != std::string::npos);
        for (const TextLine& l : wrapLines(lines, 300))
            CHECK(textWidth(l.text.c_str()) <= 300 && l.text.find("**") == std::string::npos && l.text.find('`') == std::string::npos);
    }
    std::string none = "x";
    CHECK(!readTextFile(std::string(PE_TESTS_DIR) + "/no such file.md", none) && none.empty());
}

TEST(the_about_tab_lays_out_its_lines_in_the_content)
{
    std::string md;
    CHECK(readTextFile(entryOf(0).dir + "/LICENSE.md", md));
    const std::vector<TextLine> license = markdownLines(md);
    for (float w : { kMinWidth, kDefaultWidth, 1600.0f })
        for (float h : { kMinHeight, kDefaultHeight }) {
            UiContext c = ctxOf(0, 1, 0);
            c.license = &license;
            const Layout l = computeLayout(w, h, kTabAbout, c);
            CHECK(l.emptyNote.w == 0 && l.aboutLines.size() > license.size());
            CHECK(std::none_of(l.rows.begin(), l.rows.end(), [](const Row& r) { return r.scrolls; }));
            const Rect& k = l.content;
            auto inside = [&](const Rect& r) { return r.w > 0 && r.x >= k.x - 1e-3f && r.x + r.w <= k.x + k.w + 0.5f && r.y >= k.y; };
            float y = k.y;
            for (const AboutLine& a : l.aboutLines) {
                CHECK(inside(a.rect) && a.rect.y >= y - 1e-3f); // top to bottom, none over another
                CHECK(textWidth(a.text.c_str()) <= a.rect.w + 0.5f);
                CHECK(!a.rect.overlaps(l.aboutCopy));
                y = a.rect.y + a.rect.h;
            }
            CHECK(inside(l.aboutCopy) && inside(l.aboutLink) && !l.aboutLink.overlaps(l.aboutCopy));
            CHECK(l.contentHeight >= y - k.y - 0.5f);
            for (const Subheading& s : l.subheadings)
                for (const AboutLine& a : l.aboutLines)
                    CHECK(!s.rect.overlaps(a.rect));
            // the plugin's name and version first, the commit, the license; then the instrument
            CHECK(l.aboutLines[0].text.find(versionText()) != std::string::npos);
            auto has = [&](const std::string& s) {
                return std::any_of(l.aboutLines.begin(), l.aboutLines.end(), [&](const AboutLine& a) { return a.text.find(s) != std::string::npos; });
            };
            CHECK(has(buildCommit()) && has("AGPL-3.0-or-later") && has(entryOf(0).name) && has("Salamander"));
            CHECK(l.aboutLink.w > 0);
        }
    // the license scrolls in a small window
    UiContext c = ctxOf(0, 1, 0);
    c.license = &license;
    CHECK(maxScroll(computeLayout(kMinWidth, kMinHeight, kTabAbout, c)) > 0);
    // no LICENSE.md, no instrument: said plainly
    auto says = [](const Layout& l, const char* s) {
        return std::any_of(l.aboutLines.begin(), l.aboutLines.end(), [&](const AboutLine& a) { return a.text.find(s) != std::string::npos; });
    };
    CHECK(says(computeLayout(kDefaultWidth, kDefaultHeight, kTabAbout, ctxOf(0, 1, 0)), "No LICENSE.md"));
    UiContext none {};
    CHECK(says(computeLayout(kDefaultWidth, kDefaultHeight, kTabAbout, none), "missing"));
    // a tiny window: every line still inside
    for (float w : { 100.0f, 200.0f }) {
        const Layout t = computeLayout(w, 80, kTabAbout, c);
        for (const AboutLine& a : t.aboutLines)
            CHECK(a.rect.w > 0 && a.rect.x + a.rect.w <= w + 1e-3f);
    }
    // the other tabs carry none of it
    for (Tab t : { kTabSound, kTabMidi, kTabFelt })
        CHECK(computeLayout(kDefaultWidth, kDefaultHeight, t, c).aboutLines.empty() && computeLayout(kDefaultWidth, kDefaultHeight, t, c).aboutCopy.w == 0);
}
