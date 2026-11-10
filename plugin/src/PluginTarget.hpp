#pragma once
// A target's own sources (PianoPlugin.cpp, PianoUI.cpp, StandaloneCli.cpp), never pe_common's: the kind
// the target is built as, from its DistrhoPluginInfo.h (src/plugins/<name>/, on its include path only)
#include "DistrhoPluginInfo.h"
#include "PluginKind.hpp"

namespace pe {
constexpr PluginKind kKind = PluginKind(PE_PLUGIN_KIND);
static_assert(sameText(DISTRHO_PLUGIN_NAME, identity(kKind).name), "DistrhoPluginInfo.h: the name");
static_assert(sameText(DISTRHO_PLUGIN_URI, identity(kKind).uri), "DistrhoPluginInfo.h: the URI");
static_assert(sameText(DISTRHO_PLUGIN_CLAP_ID, identity(kKind).clapId), "DistrhoPluginInfo.h: the CLAP id");
static_assert(sameText(DISTRHO_PLUGIN_BRAND, kPluginBrand), "DistrhoPluginInfo.h: the maker");
static_assert(DISTRHO_PLUGIN_NUM_INPUTS == identity(kKind).audioInputs, "DistrhoPluginInfo.h: the audio inputs");
}
