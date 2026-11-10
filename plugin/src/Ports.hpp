#pragma once
// The ports: which of the internal parameters (Params.hpp, one table for both plugins) a host sees,
// in which order, and at which default. Dussek's are the table's first 78 rows, as v0.3.2 released them;
// Weidinger's the brass's own and the shared ones it plays (volume, dynamics, mics, room, tuning). The
// diagnostics follow a plugin's ports in their order. A parameter outside a plugin's ports holds its
// default there and never reaches the host. Locked per plugin: plugin/tests/params.lock (Dussek),
// params-weidinger.lock.
#include "Params.hpp"
#include "PluginKind.hpp"

#include <cstdint>
#include <vector>

namespace pe {

constexpr uint32_t kNoPort = 0xFFFFFFFFu;

/// the internal indices a host sees, ascending (user parameters only)
const std::vector<uint32_t>& ports(PluginKind k);
/// the host's parameter count: the ports and the diagnostics
uint32_t hostCount(PluginKind k);
/// an internal index's host index (diagnostics included), kNoPort where the plugin has none
uint32_t toHost(PluginKind k, uint32_t internal);
/// a host index's internal one, kNoPort out of range
uint32_t toInternal(PluginKind k, uint32_t host);
/// the plugin's default for an internal parameter (the table's, but Weidinger starts at the Audience)
float defaultOf(PluginKind k, uint32_t internal);
/// the plugin's name and description for an internal parameter (the table's, but Weidinger's speak of a bell and its
/// player where the table's speak of a piano)
const char* nameOf(PluginKind k, uint32_t internal);
const char* descriptionOf(PluginKind k, uint32_t internal);
/// a value's name (paramLabel), the plugin's own where it differs
const char* labelOf(PluginKind k, uint32_t internal, uint32_t value);
/// whether a host sees it
bool hasPort(PluginKind k, uint32_t internal);
/// the parameter groups (Group, Params.hpp) the plugin's ports use, ascending: a host's group id is its place here (DPF
/// takes an id at or past their count for one of its predefined groups)
const std::vector<uint32_t>& portGroups(PluginKind k);
/// a Group's host id, kNoPort where the plugin has no port in it
uint32_t hostGroup(PluginKind k, uint32_t group);

} // namespace pe
