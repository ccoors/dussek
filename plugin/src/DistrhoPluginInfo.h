#ifndef DISTRHO_PLUGIN_INFO_H_INCLUDED
#define DISTRHO_PLUGIN_INFO_H_INCLUDED

// working name; no product names or trademarks
#define DISTRHO_PLUGIN_BRAND   "PianoEngine"
#define DISTRHO_PLUGIN_NAME    "PianoEngine"
#define DISTRHO_PLUGIN_URI     "urn:pianoengine:piano"
#define DISTRHO_PLUGIN_CLAP_ID "local.pianoengine.piano"

#define DISTRHO_PLUGIN_BRAND_ID  PnEn
#define DISTRHO_PLUGIN_UNIQUE_ID PnoA

#define DISTRHO_PLUGIN_HAS_UI           1
#define DISTRHO_UI_USE_NANOVG           1
#define DISTRHO_UI_USER_RESIZABLE       1
#define DISTRHO_UI_FILE_BROWSER         1 // the Temperament menu's "Scala file"
#define DISTRHO_UI_DEFAULT_WIDTH        900
#define DISTRHO_UI_DEFAULT_HEIGHT       624 // the controls' 600 + the diagnostics' header row (24)
#define DISTRHO_PLUGIN_WANT_PROGRAMS    1 // the shipped full presets (PianoPlugin.cpp)
#define DISTRHO_PLUGIN_WANT_STATE       1
#define DISTRHO_PLUGIN_WANT_FULL_STATE  1
#define DISTRHO_PLUGIN_IS_RT_SAFE       1
#define DISTRHO_PLUGIN_IS_SYNTH         1
#define DISTRHO_PLUGIN_NUM_INPUTS       2
#define DISTRHO_PLUGIN_NUM_OUTPUTS      2
#define DISTRHO_PLUGIN_WANT_MIDI_INPUT  1
#define DISTRHO_PLUGIN_WANT_LATENCY     1
// MIDI learn sets parameters from the DSP side and tells the host (where it takes such requests)
#define DISTRHO_PLUGIN_WANT_PARAMETER_VALUE_CHANGE_REQUEST 1

#define DISTRHO_PLUGIN_LV2_CATEGORY    "lv2:InstrumentPlugin"
#define DISTRHO_PLUGIN_VST3_CATEGORIES "Instrument|Piano"
#define DISTRHO_PLUGIN_CLAP_FEATURES   "instrument", "synthesizer", "stereo"

#endif
