/*
 * DPF wrapper around the Rust engine (dussek.h): MIDI, sustain pedal, parameters,
 * instrument loading off the audio thread.
 *
 * Loading: a worker thread creates engines (instrument × sample rate × tuning) and hands
 * them to the audio thread (EngineHandoff); the audio thread swaps them in at the start
 * of run(), the new engine taking over the sounding notes of the old one (pe_adopt), and
 * hands the old one back for the worker to free. An instrument that does not load drops
 * the engine playing (silence). Each slot has one writer and one reader, so the audio
 * thread never blocks, allocates or frees. The
 * worker keeps a cache (loaded instrument, radiation) so that tuning changes rebuild in
 * milliseconds.
 *
 * MIDI learn: the worker reads the ties from the settings file (MidiMap.hpp) into a lock-free
 * table; run() routes each CC through it (MidiRoute.hpp) and sets a tied control as the user's
 * edit would, telling the host where it takes change requests and writing the value mirror for
 * the UI to pass on where it does not. Instrument steps and program changes go to the worker.
 */

#include "DistrhoPlugin.hpp"
#include "DistrhoPluginUtils.hpp"
#include "About.hpp"
#include "Catalog.hpp"
#include "FileWatch.hpp"
#include "Morph.hpp"
#include "EngineHandoff.hpp"
#include "EngineValues.hpp"
#include "FeltWatch.hpp"
#include "WorkerPriority.hpp"
#include "Params.hpp"
#include "Presets.hpp"
#include "Instruments.hpp"
#include "MidiMap.hpp"
#include "MidiRoute.hpp"
#include "PedalMixer.hpp"
#include "CurveSlot.hpp"
#include "SharedRegion.hpp"
#include "StateVersion.hpp"
#include "TuningWatch.hpp"
#include "UiState.hpp"
#include "dussek.h"

#include <atomic>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <unistd.h>
#include <vector>
#include <iterator>

static_assert(pe::kParamCount <= PE_TELEMETRY_VALUES, "every parameter has a slot in the value mirror");

START_NAMESPACE_DISTRHO

using namespace pe;

class PianoPlugin : public Plugin
{
public:
    PianoPlugin()
        : Plugin(kTotalParams, (uint32_t)shippedFullPresets().size(), 7)
    {
        for (uint32_t i = 0; i < kParamCount; ++i)
            fValues[i] = paramInfo(i).def;
        for (uint32_t i = kParamCount; i < kTotalParams; ++i)
            fValues[i] = diagInfo(i).def;
        for (uint32_t i = 0; i < kParamCount; ++i)
            fShared[i] = fValues[i];
        fTuningWatch.reset(engineTuning(fValues));
        fInput.resize(getBufferSize());
        fFadeL.resize(getBufferSize());
        fFadeR.resize(getBufferSize());
        setLatency(64);
        if (isDummyInstance())
            return;
        try {
            SharedRegion::sweepStale(); // regions of dead processes (crash, SIGKILL, JACK standalone): not RT, once
            for (int attempt = 0; attempt < 16 && !fRegion; ++attempt) {
                const uint32_t token = SharedRegion::randomToken();
                fRegion = SharedRegion::create(SharedRegion::nameForToken(token), pe_telemetry_region_size());
                if (fRegion && pe_telemetry_init(fRegion->data(), fRegion->size())) {
                    pe_telemetry_set_owner(fRegion->data(), fRegion->size(), (uint32_t)getpid()); // the sweep's liveness proof
                    fValues[kDiagToken] = (float)token;
                }
                else
                    fRegion.reset();
            }
        } catch (...) {
            // anything in the setup (the sweep, the region) can throw: go on without diagnostics (the token stays 0)
            fRegion.reset();
            fValues[kDiagToken] = 0.0f;
            d_stderr2("Dussek: diagnostics region unavailable (the strip and the sound are unaffected)");
        }
        fRequestRate = getSampleRate();
        fRequest.fetch_add(1);
        fWorker = std::thread([this] { worker(); });
    }

    ~PianoPlugin() override
    {
        if (fWorker.joinable()) {
            {
                std::lock_guard<std::mutex> lock(fMutex);
                fQuit = true;
            }
            fWake.notify_one();
            fWorker.join();
        }
        pe_destroy(fEngine);
        pe_destroy(fFading);
        pe_destroy(fHandoff.drop());
        pe_destroy(fHandoff.retired());
    }

protected:
    const char* getLabel() const override { return "Dussek"; }
    const char* getDescription() const override
    {
        return "Physically modeled pianos, harpsichords, a clavichord, harps, guitars, zithers and "
               "percussion, played in real time: no samples, each instrument measured from recordings "
               "of a real one. Strings with hammer, plectrum or mallet, soundboard, room, dampers, "
               "pedals and sympathetic resonance.";
    }
    const char* getMaker() const override { return "Christian Friedrich Coors"; }
    const char* getHomePage() const override { return kProjectUrl; }
    const char* getLicense() const override { return "AGPL-3.0-or-later"; }
    uint32_t getVersion() const override
    {
        const pe::Version v = pe::pluginVersion(); // plugin/CMakeLists.txt's project version
        return d_version(v.major, v.minor, v.patch);
    }

    void initParameter(uint32_t index, Parameter& p) override
    {
        if (index >= kParamCount) {
            const ParamInfo& d = diagInfo(index);
            // never automatable (ui_tests no_diagnostic_is_automatable): they move when the table grows
            p.hints = (diagIsOutput(index) ? kParameterIsOutput : 0) | kParameterIsHidden | (d.integer ? kParameterIsInteger : 0)
                | (d.automatable ? kParameterIsAutomatable : 0);
            p.name = d.name;
            p.symbol = d.symbol;
            p.ranges = ParameterRanges(d.def, d.min, d.max);
            return;
        }
        const ParamInfo& d = paramInfo(index);
        p.hints = (d.automatable ? kParameterIsAutomatable : 0) | (d.integer ? kParameterIsInteger : 0)
            | (d.boolean ? kParameterIsBoolean : 0);
        p.groupId = d.group;
        p.name = d.name;
        p.symbol = d.symbol;
        if (d.unit != nullptr)
            p.unit = d.unit;
        if (d.description != nullptr)
            p.description = d.description;
        p.ranges = ParameterRanges(d.def, d.min, d.max);
        if (const uint32_t n = paramLabelCount(index)) {
            p.enumValues.count = n;
            p.enumValues.restrictedMode = true;
            p.enumValues.values = new ParameterEnumerationValue[n];
            for (uint32_t i = 0; i < n; ++i)
                p.enumValues.values[i] = ParameterEnumerationValue(i, paramLabel(index, i));
        }
    }

    void initPortGroup(uint32_t groupId, PortGroup& g) override
    {
        // one per Group (Params.hpp), in its order: a missing one left the group's symbol empty, which
        // DPF's LV2 export asserted on and skipped (the Preparation group, added later)
        static const char* const names[] = { "Instrument", "Microphones", "Tuning", "Noises & resonance",
            "Condition", "Defects", "Preparation" };
        static const char* const symbols[] = { "group_instrument", "group_microphones", "group_tuning",
            "group_noises", "group_condition", "group_defects", "group_preparation" };
        static_assert(std::size(names) == kGroupCount && std::size(symbols) == kGroupCount, "a name and a symbol per Group");
        if (groupId < kGroupCount) {
            g.name = names[groupId];
            g.symbol = symbols[groupId];
        }
    }

    // the programs: the shipped full presets (Presets.hpp), Default first (the JACK standalone loads
    // program 0 at its start), so a host lists them in its own preset menu (DPF writes them into the LV2
    // bundle's presets.ttl at the build, every parameter and state)
    void initProgramName(uint32_t index, String& name) override
    {
        const std::vector<const Preset*> p = shippedFullPresets();
        if (index < p.size())
            name = p[index]->name.c_str();
    }
    // every parameter at the preset's value, then its states (the instrument, the rest at their defaults);
    // the states take locks: an LV2 host restores a preset itself, VST3 and CLAP hosts call this off the
    // audio thread in practice. On the audio thread it is a MIDI program change the wrapper caught (the
    // JACK standalone loads program N on channel 1's program change N, the toggle unread, the UI never
    // told): ignored, the MIDI route reads the same message as the toggle says
    void loadProgram(uint32_t index) override
    {
        if (std::this_thread::get_id() == fAudioThread.load(std::memory_order_relaxed))
            return;
        const std::vector<const Preset*> p = shippedFullPresets();
        if (index >= p.size())
            return;
        for (const auto& [i, v] : presetParams(*p[index]))
            setParameterValue(i, v);
        for (const auto& [k, v] : presetStates(*p[index]))
            setState(k.c_str(), v.c_str());
    }

    void initState(uint32_t index, State& s) override
    {
        // the state's version first: DPF restores in this order (LV2 by index, VST3 and CLAP as
        // getState wrote them), so the others are migrated from it (StateVersion.hpp)
        if (index == 0) {
            s.key = kStateVersionKey;
            s.defaultValue = String(kStateVersion);
            s.hints = 0;
            return;
        }
        --index;
        if (index == 0) {
            s.key = "ui";
            s.defaultValue = formatUiState(UiState {}).c_str();
            s.hints = kStateIsOnlyForUI;
        } else if (index == 1) {
            // the instrument's id (its directory's name, Catalog.hpp); empty: the menu's first
            s.key = "instrument";
            s.defaultValue = "";
            s.hints = 0;
        } else if (index == 2) {
            // the velocity curve's points ("1:1,64:80,127:127"; empty: straight), the Touch tab's
            s.key = "velocity_curve";
            s.defaultValue = "";
            s.hints = 0;
        } else if (index == 3) {
            // the morph's partners: instrument ids after the chosen one, comma-separated (Morph.hpp; empty: none)
            s.key = "morph";
            s.defaultValue = "";
            s.hints = 0;
        } else if (index == 4) {
            // the player's felt: bends per quantity ("s:21=6,48=0;p:;c:;d:", FeltWatch.hpp; empty: shipped)
            s.key = "felt";
            s.defaultValue = "";
            s.hints = 0;
        } else if (index == 5) {
            // the Scala file the Temperament menu's "Scala file" plays (a path; empty: none)
            s.key = "scale_file";
            s.defaultValue = "";
            s.hints = kStateIsFilenamePath;
        }
    }
    String getState(const char* key) const override
    {
        if (std::strcmp(key, kStateVersionKey) == 0)
            return String(kStateVersion);
        if (std::strcmp(key, "instrument") == 0) {
            std::lock_guard<std::mutex> lock(fInstrumentMutex);
            return String(fInstrument.c_str());
        }
        if (std::strcmp(key, "velocity_curve") == 0) {
            std::lock_guard<std::mutex> lock(fCurveMutex);
            return String(fCurveText.c_str());
        }
        if (std::strcmp(key, "morph") == 0) {
            std::lock_guard<std::mutex> lock(fInstrumentMutex);
            return String(fMorph.c_str());
        }
        if (std::strcmp(key, "felt") == 0) {
            std::lock_guard<std::mutex> lock(fInstrumentMutex);
            return String(fFelt.text().c_str());
        }
        if (std::strcmp(key, "scale_file") == 0) {
            std::lock_guard<std::mutex> lock(fInstrumentMutex);
            return String(fScaleFile.c_str());
        }
        std::lock_guard<std::mutex> lock(fUiStateMutex);
        return String(std::strcmp(key, "ui") == 0 ? fUiState.c_str() : "");
    }
    void setState(const char* key, const char* value) override
    {
        // an older state's value in today's form (the version came first: initState)
        const std::string migrated = fRestore.apply(key, value != nullptr ? value : "");
        if (std::strcmp(key, kStateVersionKey) == 0)
            return;
        value = migrated.c_str();
        if (std::strcmp(key, "instrument") == 0) {
            {
                std::lock_guard<std::mutex> lock(fInstrumentMutex);
                const std::string id = value != nullptr ? value : "";
                if (id == fInstrument && !id.empty())
                    return; // the UI echoing an instrument MIDI chose: loaded already
                fInstrument = id;
                fFelt.setInstrument(id); // the felt stays: a restore sets both states in its own order
                fInstrumentOut.store(0, std::memory_order_relaxed); // MIDI's last choice is no longer the current one
            }
            fRequest.fetch_add(1); // the worker loads it
            return;
        }
        if (std::strcmp(key, "morph") == 0) {
            {
                std::lock_guard<std::mutex> lock(fInstrumentMutex);
                fMorph = value != nullptr ? value : "";
            }
            fRequest.fetch_add(1); // the worker builds the morph
            return;
        }
        if (std::strcmp(key, "felt") == 0) {
            {
                std::lock_guard<std::mutex> lock(fInstrumentMutex);
                fFelt.setFelt(value != nullptr ? value : "");
            }
            fRequest.fetch_add(1); // the worker builds the felt (measured: a few seconds)
            return;
        }
        if (std::strcmp(key, "scale_file") == 0) {
            {
                std::lock_guard<std::mutex> lock(fInstrumentMutex);
                fScaleFile = value != nullptr ? value : "";
            }
            fRequest.fetch_add(1); // the worker builds the scale
            return;
        }
        if (std::strcmp(key, "velocity_curve") == 0) {
            PeVelocityCurve c;
            pe_velocity_curve_parse(value, &c); // unreadable: straight
            // under the lock: the host may call setState from two threads at once (LV2's restore and
            // its worker), and the slot takes one writer; the audio thread never takes this lock
            std::lock_guard<std::mutex> lock(fCurveMutex);
            fCurveText = value != nullptr ? value : "";
            fCurveSlot.publish(c); // the audio thread sets it on its engine
            return;
        }
        if (std::strcmp(key, "ui") != 0)
            return;
        std::lock_guard<std::mutex> lock(fUiStateMutex);
        fUiState = value;
    }

    float getParameterValue(uint32_t index) const override
    {
        return index < kTotalParams ? fValues[index] : 0.0f;
    }

    // may be called from the audio thread: only stores values and raises a request
    void setParameterValue(uint32_t index, float value) override
    {
        if (index >= kTotalParams || diagIsOutput(index))
            return;
        fValues[index] = value;
        if (index == kDiagMapSeqIn)
            fMapRequest.fetch_add(1); // the UI wrote the MIDI map: the worker reads it again
        if (index >= kParamCount)
            return; // the diagnostic inputs are read in run
        // a value MIDI set that the host never heard of is old news once the host sends one: the UI
        // must not bring it back (applyMidiValue writes the mirror after this)
        if (fRegion) {
            uint32_t seq;
            float m;
            if (pe_telemetry_read_value(fRegion->data(), fRegion->size(), index, &seq, &m) && seq != 0 && std::isfinite(m))
                pe_telemetry_set_value(fRegion->data(), fRegion->size(), index, std::nanf(""));
        }
        // the worker builds an engine for the instrument (the state) and the tuning: a new one when the
        // tuning changes, which the worker itself checks (the host's thread and, through MIDI learn, the
        // audio thread both get here)
        const bool morphMoved = (index == kMorphX || index == kMorphY) && fShared[index].load() != value;
        const bool hardnessMoved = index == kHammerHardness && fShared[index].load() != value;
        fShared[index] = value;
        fTuningWatch.mark();
        if (morphMoved)
            fRequest.fetch_add(1); // the pad's point: the worker builds the morph there
        if (hardnessMoved && fHammerPiano.load(std::memory_order_relaxed))
            fRequest.fetch_add(1); // a hammer piano's hardness goes through the build, its loudness kept
    }

    void bufferSizeChanged(uint32_t newBufferSize) override
    {
        fInput.resize(newBufferSize); // not on the audio thread (the plugin is deactivated)
        fFadeL.resize(newBufferSize);
        fFadeR.resize(newBufferSize);
    }

    void sampleRateChanged(double newSampleRate) override
    {
        fRequestRate = newSampleRate;
        fRequest.fetch_add(1);
        fWake.notify_one();
    }

    void activate() override
    {
        if (fEngine != nullptr)
            pe_all_keys_up(fEngine);
        // a non-zero UI pedal now is stale: mute it until the parameter changes (a drag sets it live again)
        fPedal.setLatch(fValues[kSustainLatch] >= 0.5f); // a setting: the engine is told below
        applyPedal(fPedal.activate(std::max(0.0f, std::min(1.0f, fValues[kDiagPedalIn]))), 0);
        fSostenuto.activate(std::max(0.0f, std::min(1.0f, fValues[kDiagSostenutoIn])));
        fSoft.activate(std::max(0.0f, std::min(1.0f, fValues[kDiagSoftIn])));
        fPanic.activate(fValues[kDiagPanicIn]);
        fOriginal.activate(fValues[kDiagFeltOriginalIn]);
    }

    void run(const float** inputs, float** outputs, uint32_t frames, const MidiEvent* events, uint32_t eventCount) override
    {
        fAudioThread.store(std::this_thread::get_id(), std::memory_order_relaxed);
        swapIn();
        // the audio input, the two channels summed: read before the outputs are written (they may share memory)
        const float* input = nullptr;
        if (fValues[kAudioInput] > kOffDb && frames <= fInput.size()) {
            for (uint32_t i = 0; i < frames; ++i)
                fInput[i] = inputs[0][i] + inputs[1][i];
            input = fInput.data();
        }
        float* const left = outputs[0];
        float* const right = outputs[1];
        if (fValues[kDiagLearnIn] != fLearnSeen) { // another control waits (or none): nothing caught for it yet
            fLearnSeen = fValues[kDiagLearnIn];
            fValues[kDiagLearnedOut] = 0.0f;
        }
        fRouter.setLearn(learnInput(fValues[kDiagLearnIn])); // seen even when no CC comes in this block
        fValues[kDiagInstrumentOut] = (float)fInstrumentOut.load(std::memory_order_relaxed);
        fValues[kDiagBuildingOut] = fBuilding.load(std::memory_order_relaxed) ? 1.0f : 0.0f;
        fValues[kDiagFeltOriginalOut] = fOriginalShared.load(std::memory_order_relaxed) ? 1.0f : 0.0f;
        if (fEngine == nullptr) {
            std::fill(left, left + frames, 0.0f);
            std::fill(right, right + frames, 0.0f);
            for (uint32_t i = kDiagKeys0; i <= kDiagVoices; ++i)
                fValues[i] = 0.0f;
            mixFading(input, left, right, frames);
            // no instrument plays (loading, or it does not load): the ties and the instrument steps
            // still work (stepping away from an instrument that does not load); notes and pedals wait
            for (uint32_t i = 0; i < eventCount; ++i) {
                const MidiEvent& ev = events[i];
                if (ev.size < 2 || ev.size > MidiEvent::kDataSize)
                    continue;
                if ((ev.data[0] & 0xF0) == 0xB0 && ev.size >= 3)
                    routeCc(ev.data[0] & 0x0F, ev.data[1], ev.data[2]);
                else if ((ev.data[0] & 0xF0) == 0xC0)
                    routeProgram(ev.data[1]);
            }
            return;
        }
        PeParams params = engineParams(fValues);
        params.hardness = liveHardness(params.hardness, fEngineHammer);
        pe_set_params(fEngine, &params);
        if (const bool original = fOriginal.set(fValues[kDiagFeltOriginalIn]); original != fOriginalShared.load(std::memory_order_relaxed)) {
            fOriginalShared.store(original, std::memory_order_relaxed);
            if (fEngineHammer)
                fRequest.fetch_add(1); // the worker builds the shipped felt or the player's again (cached)
        }
        PeVelocityCurve curve;
        if (fCurveSlot.take(fEngine, curve)) // changed, or a new engine
            pe_set_velocity_curve(fEngine, &curve);
        if (fPanic.fire(fValues[kDiagPanicIn]))
            panic(0, true);
        const float pedalUi = std::max(0.0f, std::min(1.0f, fValues[kDiagPedalIn]));
        applyPedal(fPedal.setUi(pedalUi), 0);
        applyPedal(fPedal.setLatch(fValues[kSustainLatch] >= 0.5f), 0);
        applySostenuto(fSostenuto.setUi(std::max(0.0f, std::min(1.0f, fValues[kDiagSostenutoIn]))), 0);
        applySoft(fSoft.setUi(std::max(0.0f, std::min(1.0f, fValues[kDiagSoftIn]))), 0);

        for (uint32_t i = 0; i < eventCount; ++i) {
            const MidiEvent& ev = events[i];
            if (ev.size < 2 || ev.size > MidiEvent::kDataSize)
                continue;
            const uint8_t* d = ev.data;
            const uint32_t frame = std::min(ev.frame, frames - 1);
            switch (d[0] & 0xF0) {
            case 0x90:
                if (ev.size >= 3)
                    pe_note_on(fEngine, frame, d[1], d[2]);
                break;
            case 0x80:
                if (ev.size >= 3)
                    pe_note_off_velocity(fEngine, frame, d[1], d[2]); // the release velocity: the damper's fall
                else
                    pe_note_off(fEngine, frame, d[1]);
                break;
            case 0xA0: // key pressure: the clavichord's Bebung on that key
                if (ev.size >= 3)
                    pe_key_pressure(fEngine, frame, d[1], d[2] / 127.0f);
                break;
            case 0xD0: // channel pressure: the Bebung on every held key
                pe_channel_pressure(fEngine, frame, d[1] / 127.0f);
                break;
            case 0xE0: // the pitch wheel: the dan tranh's bend
                if (ev.size >= 3)
                    pe_pitch_bend(fEngine, frame, (float)(((int)d[2] << 7 | d[1]) - 8192) / 8192.0f);
                break;
            case 0xC0:
                routeProgram(d[1]);
                break;
            case 0xB0:
                if (ev.size < 3 || routeCc(d[0] & 0x0F, d[1], d[2]))
                    break;
                if (d[1] == 1) {
                    pe_mod_wheel(fEngine, frame, d[2] / 127.0f); // the Bebung, for keyboards without aftertouch
                } else if (d[1] == 64) {
                    // continuous: half-pedalling with pedals that send it, on/off otherwise
                    applyPedal(fPedal.setCc(d[2]), frame);
                } else if (d[1] == 67) {
                    applySoft(fSoft.setCc(d[2]), frame); // continuous, like the sustain pedal
                } else if (d[1] == 66) {
                    applySostenuto(fSostenuto.setCc(d[2] >= 64 ? 127 : 0), frame);
                } else if (d[1] == 120 || d[1] == 123) {
                    panic(frame, d[1] == 120); // 120 all sound off: silence; 123 all notes off: the releases
                }
                break;
            }
        }
        pe_process(fEngine, input, left, right, frames);
        mixFading(input, left, right, frames);
        PeDiag diag;
        pe_diag(fEngine, &diag);
        for (uint32_t i = 0; i < 8; ++i)
            fValues[kDiagKeys0 + i] = (float)diag.keys[i];
        fValues[kDiagPedalOut] = diag.pedal;
        fValues[kDiagSostenutoOut] = diag.sostenuto;
        fValues[kDiagSoftOut] = diag.soft;
        fValues[kDiagVoices] = (float)diag.voices;
    }

private:
    /// MIDI learn's part of a CC (audio thread): true when it took the CC (caught for a waiting
    /// control, a tie), false for the built-in handling
    bool routeCc(uint8_t channel, uint8_t cc, uint8_t value)
    {
        const MidiRouter::Action a = fRouter.cc(channel, cc, value, learnInput(fValues[kDiagLearnIn]), fTies, fLists);
        switch (a.kind) {
        case MidiRouter::Action::Builtin:
            return false;
        case MidiRouter::Action::Learned:
            fValues[kDiagLearnedOut] = (float)a.learned;
            break;
        case MidiRouter::Action::Set:
            if (!fTies.softTakeover() || fTakeover.accept(a.param, a.value, fValues[a.param]))
                applyMidiValue(a.param, a.value);
            break;
        case MidiRouter::Action::Flip:
            applyMidiValue(a.param, fValues[a.param] >= 0.5f ? 0.0f : 1.0f);
            break;
        case MidiRouter::Action::Nudge:
            applyMidiValue(a.param, nudgeValue(a.param, fValues[a.param], a.step, fLists.choices(a.param)));
            break;
        case MidiRouter::Action::Step:
            fStep.fetch_add(a.step, std::memory_order_relaxed); // the worker switches (its 50 ms poll)
            break;
        case MidiRouter::Action::None:
        case MidiRouter::Action::Program:
            break;
        }
        return true;
    }

    void routeProgram(uint8_t program)
    {
        const MidiRouter::Action a = fRouter.program(program, fTies);
        if (a.kind == MidiRouter::Action::Program)
            fProgram.store((int)a.program, std::memory_order_relaxed);
    }

    /// a value from MIDI, as the user's edit would set it (audio thread): the host told where it
    /// takes change requests, else the value mirror (the UI passes it on); the 8' II and the lute
    /// exclude each other as in the UI
    void applyMidiValue(uint32_t i, float v)
    {
        if (i >= kParamCount || v == fValues[i])
            return;
        setParameterValue(i, v);
        const bool told = canRequestParameterValueChanges() && requestParameterValueChange(i, v);
        if (!told && fRegion)
            pe_telemetry_set_value(fRegion->data(), fRegion->size(), i, v);
        const uint32_t other = exclusiveStop(i);
        if (other < kParamCount && v >= 0.5f)
            applyMidiValue(other, 0.0f);
    }

    /// every key and pedal released; with `silence` (the Panic button, CC120) the engine's sound
    /// faded out and cleared too (pe_silence), else (CC123) the dampers fall
    void panic(uint32_t frame, bool silence)
    {
        if (silence)
            pe_silence(fEngine);
        else
            pe_all_keys_up(fEngine);
        applyPedal(fPedal.panic(), frame); // a held UI pedal does not survive the reset (it is stale); the latch does
        fSostenuto.panic();
        fSoft.panic();
    }

    /// the only place that calls pe_pedal_depth: the mixer says when and how deep
    void applyPedal(const PedalMixer::Call& c, uint32_t frame)
    {
        if (c.call && fEngine != nullptr) // no engine yet: a new one is told the applied depth (swapIn)
            pe_pedal_depth(fEngine, frame, c.depth);
    }

    /// the only place that calls pe_sostenuto: the mixer (CC66 mapped to 0 / 127, and the UI lever) says when
    void applySostenuto(const PedalMixer::Call& c, uint32_t frame)
    {
        if (c.call)
            pe_sostenuto(fEngine, frame, c.depth >= 0.5f);
    }

    /// the only place that calls pe_soft_pedal
    void applySoft(const PedalMixer::Call& c, uint32_t frame)
    {
        if (c.call)
            pe_soft_pedal(fEngine, frame, c.depth);
    }

    // audio thread: take a freshly loaded engine, or drop the one playing, if the retire slot is free
    /// the engine fading out after a handover (FadeOut), added to the outputs; given back once silent
    void mixFading(const float* input, float* left, float* right, uint32_t frames)
    {
        if (fFading == nullptr)
            return;
        if (fFade.active() && frames <= fFadeL.size()) {
            pe_process(fFading, input, fFadeL.data(), fFadeR.data(), frames);
            fFade.addTo(fFadeL.data(), fFadeR.data(), left, right, frames);
        } else {
            fFade.stop(); // a block longer than its buffers: it stops here
        }
        if (!fFade.active() && fHandoff.idle()) {
            fHandoff.retire(fFading);
            fFading = nullptr;
        }
    }

    void swapIn()
    {
        PeEngine* next = nullptr;
        switch (fHandoff.poll(next)) {
        case EngineHandoff<PeEngine>::Change::None:
            return;
        case EngineHandoff<PeEngine>::Change::Drop: // the chosen instrument does not load: silence
            fHandoff.retire(fEngine);
            fEngine = nullptr;
            fCurveSlot.forget(); // the next engine may come at the freed one's address
            return;
        case EngineHandoff<PeEngine>::Change::Next:
            break;
        }
        const bool adopted = fEngine != nullptr && pe_adopt(next, fEngine);
        if (!adopted && fEngine != nullptr) {
            // its notes could not go over (another instrument, another morph mix): it fades out beside the new
            // one; one still fading goes now (the slot for it is free: poll saw to that)
            if (fFading != nullptr)
                fHandoff.retire(fFading);
            fFading = fEngine;
            fFade.start((uint32_t)(kHandoverFadeS * getSampleRate()));
        } else {
            fHandoff.retire(fEngine);
        }
        fEngine = next;
        fEngineHammer = pe_hammer_piano(fEngine); // its notes looked at once, here
        fCurveSlot.forget(); // run() sets the curve on it (an adopted one has it already: no harm)
        if (fRegion)
            pe_telemetry_attach(fEngine, fRegion->data(), fRegion->size(), getSampleRate());
        if (!adopted) {
            applyPedal(fPedal.replayForNewEngine(), 0);
            applySostenuto(fSostenuto.replayForNewEngine(), 0);
            applySoft(fSoft.replayForNewEngine(), 0);
        }
        setLatency(pe_latency(fEngine));
    }

    /// worker: the instrument MIDI chose (a program, steps from the current one, wrapping), as the
    /// "instrument" state; the UI hears of it through kDiagInstrumentOut
    void chooseByMidi(int step, int program, const char* resources)
    {
        const CatalogPtr catalog = sharedCatalog(resources);
        const std::vector<const InstrumentEntry*> menu = catalog->menu();
        if (menu.empty())
            return;
        const int n = (int)menu.size();
        std::lock_guard<std::mutex> il(fInstrumentMutex);
        int pos = -1;
        for (int i = 0; i < n; ++i)
            if (menu[(size_t)i]->id == fInstrument)
                pos = i;
        if (program >= 0 && program < n)
            pos = program;
        else if (program >= 0 && step == 0)
            return; // no such instrument
        if (step != 0)
            pos = pos < 0 ? (step > 0 ? 0 : n - 1) : ((pos + step) % n + n) % n;
        if (pos < 0)
            return;
        fInstrument = menu[(size_t)pos]->id;
        fFelt.midiSwitch(); // the new instrument's shipped felt (the UI clears its edit on kDiagInstrumentOut)
        fMidiChoices = (fMidiChoices + 1) % 16384;
        fInstrumentOut.store((uint32_t)(pos + 1) + 1024u * fMidiChoices, std::memory_order_relaxed);
        fRequest.fetch_add(1);
    }

    void worker()
    {
        lowerWorkerPriority(); // the builds below the host's other work (WorkerPriority.hpp)
        pe_set_build_threads(buildThreadsFor(physicalCores())); // and a core left to the audio thread
        uint64_t done = 0, mapDone = 0;
        bool refineDue = false; // the last engine took a felt's leveling as last measured
        PeCache* cache = pe_cache_new();
        // instruments that change on disk (the editor's preview of a rolled piano, a re-export): the
        // loaded ones' files and the search paths' folders, looked at twice a second
        ReloadWatch reload;
        ListingWatch listing;
        const auto t0 = std::chrono::steady_clock::now();
        const auto seconds = [t0] { return std::chrono::duration<double>(std::chrono::steady_clock::now() - t0).count(); };
        double watched = 0.0;
        std::unique_lock<std::mutex> lock(fMutex);
        while (!fQuit) {
            pe_destroy(fHandoff.retired());
            if (const double now = seconds(); now - watched >= 0.5) {
                watched = now;
                lock.unlock();
                const char* const b = getBundlePath();
                const char* const res = b != nullptr ? getResourcePath(b) : nullptr;
                if (listing.poll(Catalog::searchPaths(res), listDirs))
                    rescanCatalog(res);
                const bool changed = reload.poll(statStamp, now);
                lock.lock();
                if (changed) {
                    // the cache keeps instruments by their folder: a changed one is read anew
                    pe_cache_free(cache);
                    cache = pe_cache_new();
                    fRequest.fetch_add(1);
                }
            }
            if (const uint64_t map = fMapRequest.load(); map != mapDone) { // the MIDI map: at start, after the UI wrote it
                mapDone = map;
                lock.unlock();
                fTies.load(MidiMap::load(midiMapPath()));
                lock.lock();
                continue;
            }
            if (PeTuning changed; fTuningWatch.take(fShared, changed))
                fRequest.fetch_add(1); // another tuning: an engine for it
            const int step = fStep.exchange(0, std::memory_order_relaxed);
            const int program = fProgram.exchange(-1, std::memory_order_relaxed);
            if (step != 0 || program >= 0) {
                lock.unlock();
                const char* const b = getBundlePath();
                chooseByMidi(step, program, b != nullptr ? getResourcePath(b) : nullptr);
                lock.lock();
            }
            const uint64_t request = fRequest.load();
            // the last build stood in a felt's leveling as last measured (a morph's pad dragged): with
            // nothing newer asked, the same settings again, measured
            const bool refine = request == done && refineDue;
            if (request == done && !refine) {
                // the audio thread raises requests without waking us: poll
                fWake.wait_for(lock, std::chrono::milliseconds(50));
                continue;
            }
            // a snapshot of the parameters (torn by a change meanwhile: that change raises another request)
            float values[kParamCount];
            for (uint32_t i = 0; i < kParamCount; ++i)
                values[i] = fShared[i].load();
            const double rate = fRequestRate.load();
            PeTuning tuning = engineTuning(values);
            std::string feltText, scaleFile;
            {
                std::lock_guard<std::mutex> il(fInstrumentMutex);
                feltText = fFelt.text();
                scaleFile = fScaleFile;
            }
            tuning.scale_file = scaleFile.c_str(); // read while it builds: the copy lives until then
            const PeFelt felt = feltForBuild(feltText, fOriginalShared.load(std::memory_order_relaxed), values[kHammerHardness]);
            lock.unlock();
            const char* const bundle = getBundlePath();
            const char* const resources = bundle != nullptr ? getResourcePath(bundle) : nullptr;
            std::string id;
            MorphKey key;
            {
                std::lock_guard<std::mutex> il(fInstrumentMutex);
                const CatalogPtr catalog = sharedCatalog(resources);
                if (fInstrument.empty()) { // the default: the menu's first (saved by its name from now on)
                    if (const InstrumentEntry* first = catalog->first())
                        fInstrument = first->id;
                }
                id = fInstrument;
                // the partners that may join this instrument (another main instrument keeps the compatible ones)
                key = { id, validPartners(parsePartners(fMorph), *catalog, id), rate };
                fMorph = formatPartners(key.partners);
            }
            {
                const CatalogPtr catalog = sharedCatalog(resources);
                const InstrumentEntry* e = catalog->find(id);
                fLists.publish(e != nullptr ? *e : noInstrument()); // the Mallet and Preparation CC bands
            }
            // the main instrument first, then each partner at its weight from the pad's point
            std::vector<std::string> dirs { id.empty() ? std::string() : instrumentDir(resources, id.c_str()) };
            for (const std::string& p : key.partners)
                dirs.push_back(instrumentDir(resources, p.c_str()));
            const std::vector<float> weights = morphWeights((uint32_t)dirs.size(), values[kMorphX], values[kMorphY]);
            PeEngine* engine = nullptr;
            char err[512] = {};
            // the files' stamps before they are read: a change while building is seen afterwards
            reload.loaded(dirs, statStamp, seconds());
            fBuilding.store(true, std::memory_order_relaxed);
            if (std::find(dirs.begin(), dirs.end(), std::string()) != dirs.end())
                d_stderr2("Dussek: instrument '%s' not found (set DUSSEK_INSTRUMENTS)", id.c_str());
            else {
                std::vector<const char*> ptrs;
                for (const std::string& d : dirs)
                    ptrs.push_back(d.c_str());
                pe_cache_set_quick(cache, !refine); // a felt measured in seconds: first as last measured
                if ((engine = pe_create_morph(ptrs.data(), weights.data(), (uint32_t)ptrs.size(), rate, &tuning, &felt, cache, err, sizeof(err))) == nullptr)
                    d_stderr2("Dussek: %s", err);
            }
            refineDue = engine != nullptr && pe_cache_provisional(cache);
            lock.lock();
            done = request;
            if (fRequest.load() != request) {
                // superseded while building: still played when only the point or the tuning moved on (a
                // drag heard as it goes), else dropped; the next request builds the latest either way
                MorphKey now;
                {
                    std::lock_guard<std::mutex> il(fInstrumentMutex);
                    now = { fInstrument, parsePartners(fMorph), fRequestRate.load() };
                }
                if (engine != nullptr && offerSuperseded(key, now))
                    pe_destroy(fHandoff.offer(engine));
                else
                    pe_destroy(engine);
                continue;
            }
            fBuilding.store(false, std::memory_order_relaxed);
            const bool hammer = engine != nullptr && pe_hammer_piano(engine);
            fHammerPiano.store(hammer, std::memory_order_relaxed);
            if (hardnessMovedDuringBuild(hammer, values[kHammerHardness], fShared[kHammerHardness].load()))
                fRequest.fetch_add(1);
            // the new engine replaces one the audio thread has not taken yet; none (the instrument
            // does not load): the engine playing goes, not on with the previous instrument
            pe_destroy(engine != nullptr ? fHandoff.offer(engine) : fHandoff.drop());
        }
        pe_cache_free(cache);
    }

    float fValues[kTotalParams];
    std::vector<float> fInput; // the audio input summed to mono, a buffer's worth (bufferSizeChanged)
    // the UI's own state (open sections, zoom): host and UI threads only
    mutable std::mutex fUiStateMutex;
    std::string fUiState = formatUiState(UiState {});
    PanicTrigger fPanic; // the UI's Panic button (kDiagPanicIn), audio thread only
    PedalMixer fSoft; // CC67 and the UI soft pedal lever, as fPedal
    PedalMixer fSostenuto; // CC66 (on/off) and the UI sostenuto lever, as fPedal
    PedalMixer fPedal; // CC64 and the UI pedal parameter (F4: a stale held UI value is muted), audio thread only

    std::unique_ptr<SharedRegion> fRegion; // before fEngine: outlives the engines

    PeEngine* fEngine = nullptr;               // audio thread only
    PeEngine* fFading = nullptr;               // the previous engine, fading out (audio thread only)
    FadeOut fFade;
    std::vector<float> fFadeL, fFadeR;         // its output, a buffer's worth (bufferSizeChanged)
    static constexpr double kHandoverFadeS = 0.03; // its fade (a held note stops without a click)
    EngineHandoff<PeEngine> fHandoff;            // worker ↔ audio

    std::atomic<uint64_t> fRequest { 0 };
    std::atomic<double> fRequestRate { 48000.0 };
    std::atomic<float> fShared[kParamCount]; // the parameter values for the worker (fValues: the host's thread)
    TuningWatch fTuningWatch; // marked by setParameterValue, compared by the worker
    mutable std::mutex fInstrumentMutex;
    mutable std::mutex fCurveMutex;
    std::string fCurveText; // the "velocity_curve" state, as set
    CurveSlot fCurveSlot;   // its curve, for the audio thread
    StateRestore fRestore;   // the restored state's version (StateVersion.hpp)
    std::string fInstrument; // the "instrument" state: an id (empty: the menu's first, until the worker resolves it)
    std::string fMorph;      // the "morph" state: the partners' ids, comma-separated (under fInstrumentMutex)
    std::string fScaleFile;  // the "scale_file" state: a Scala file's path (under fInstrumentMutex)
    std::atomic<bool> fBuilding { false }; // kDiagBuildingOut: the worker builds an engine
    FeltState fFelt;                          // the "felt" state (under fInstrumentMutex)
    OriginalToggle fOriginal;                 // kDiagFeltOriginalIn (audio thread)
    std::atomic<bool> fOriginalShared { false }; // its value, for the worker
    std::atomic<bool> fHammerPiano { false };    // the last engine built plays a hammer piano (the worker's)
    bool fEngineHammer = false;                  // the engine playing does (audio thread)

    // MIDI learn
    TieTable fTies;          // worker → audio
    InstrumentLists fLists;  // worker → audio
    MidiRouter fRouter;      // audio thread only
    Takeover fTakeover;      // audio thread only
    float fLearnSeen = 0;    // kDiagLearnIn at the last block (audio thread)
    std::atomic<uint64_t> fMapRequest { 1 }; // raised: the worker reads the map's file (1: at start)
    std::atomic<int> fStep { 0 };            // instrument steps from MIDI, for the worker
    std::atomic<int> fProgram { -1 };        // a program change for the worker (-1: none)
    std::atomic<std::thread::id> fAudioThread {}; // run()'s thread (none before the first block)
    std::atomic<uint32_t> fInstrumentOut { 0 }; // kDiagInstrumentOut: MIDI's choice while it is the current one (0: none)
    uint32_t fMidiChoices = 0;               // the worker's count in it (under fInstrumentMutex)

    std::thread fWorker;
    std::mutex fMutex;
    std::condition_variable fWake;
    bool fQuit = false;

    DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PianoPlugin)
};

Plugin* createPlugin()
{
    return new PianoPlugin();
}

END_NAMESPACE_DISTRHO
