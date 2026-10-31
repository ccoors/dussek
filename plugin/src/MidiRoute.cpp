#include "MidiRoute.hpp"
#include "Format.hpp"

#include <cmath>

namespace pe {

uint32_t learnInput(float v)
{
    if (!(v >= 0.0f && v <= (float)kTargetCount))
        return 0;
    return (uint32_t)std::lround(v);
}

TieTable::TieTable()
{
    for (auto& side : fTarget)
        for (auto& ch : side)
            for (auto& t : ch)
                t.store(-1, std::memory_order_relaxed);
}

void TieTable::load(const MidiMap& m)
{
    int16_t next[2][16][128];
    for (auto& side : next)
        for (auto& ch : side)
            for (int16_t& t : ch)
                t = -1;
    for (const Tie& t : m.ties())
        for (int zero = 0; zero < 2; ++zero)
            if (!stepTarget(t.target) || t.relative || t.zero == (zero == 1))
                next[zero][t.channel & 15][t.cc & 127] = (int16_t)(t.target | (t.relative ? kRelative : 0) | (t.flip ? kFlip : 0));
    for (int s = 0; s < 2; ++s)
        for (int c = 0; c < 16; ++c)
            for (int k = 0; k < 128; ++k)
                fTarget[s][c][k].store(next[s][c][k], std::memory_order_relaxed);
    fProgramChange.store(m.programChange, std::memory_order_relaxed);
    fSoftTakeover.store(m.softTakeover, std::memory_order_relaxed);
}

InstrumentLists::InstrumentLists()
{
    for (auto& p : fPrep)
        p.store(0, std::memory_order_relaxed);
    publish(noInstrument());
}

void InstrumentLists::publish(const InstrumentEntry& e)
{
    const ListChoices prep = listChoices(kPrepMode, e);
    for (uint32_t i = 0; i < prep.count; ++i)
        fPrep[i].store(prep.values[i], std::memory_order_relaxed);
    fPreps.store(prep.count, std::memory_order_relaxed);
    fMallets.store(listChoices(kMallet, e).count, std::memory_order_relaxed);
}

ListChoices InstrumentLists::choices(uint32_t param) const
{
    ListChoices c;
    if (param == kMallet) {
        c.count = fMallets.load(std::memory_order_relaxed);
        for (uint32_t i = 0; i < c.count; ++i)
            c.values[i] = (uint8_t)i;
    } else if (param == kPrepMode) {
        c.count = fPreps.load(std::memory_order_relaxed);
        for (uint32_t i = 0; i < c.count; ++i)
            c.values[i] = fPrep[i].load(std::memory_order_relaxed);
    } else {
        c = listChoices(param, noInstrument()); // the other lists are the same on every instrument
    }
    return c;
}

MidiRouter::Action MidiRouter::cc(uint8_t channel, uint8_t cc, uint8_t value, uint32_t learnIn, const TieTable& t,
    const InstrumentLists& l)
{
    Action a;
    setLearn(learnIn);
    if (learnIn != 0 && !fCaught) {
        if (!learnableCc(cc)) {
            a.kind = Action::Builtin; // a pedal keeps working while a control waits
            return a;
        }
        fCaught = true;
        a.kind = Action::Learned;
        a.learned = packLearned(channel, cc, value == 0);
        return a;
    }
    const TieTable::Entry e = t.entry(channel, cc, value);
    const int target = e.target;
    if (target < 0) {
        // the CC's other side steps: this one does nothing
        a.kind = t.tied(channel, cc) ? Action::None : Action::Builtin;
    } else if (e.relative) {
        const int steps = relativeSteps(value);
        if ((uint32_t)target < kParamCount) {
            const ListChoices c = l.choices((uint32_t)target);
            const ParamInfo& info = paramInfo((uint32_t)target);
            a.kind = Action::Nudge;
            a.param = (uint32_t)target;
            a.step = (c.count > 0 || info.integer) && !info.boolean ? gather((uint32_t)target, steps) : steps;
        } else if (stepTarget((uint32_t)target)) {
            a.kind = Action::Step;
            a.step = gather((uint32_t)target, steps) * ((uint32_t)target == kTargetPrevious ? -1 : 1);
        }
        if (a.step == 0)
            a.kind = Action::None;
    } else if (e.flip) {
        if (value > 0) { // the press; the release passes
            a.kind = Action::Flip;
            a.param = (uint32_t)target;
        }
    } else if ((uint32_t)target < kParamCount) {
        a.kind = Action::Set;
        a.param = (uint32_t)target;
        a.value = ccToValue(a.param, value, l.choices(a.param));
    } else if (stepTarget((uint32_t)target)) {
        // every value on the tie's side: a button that sends the same value at each push steps each
        // time, a pad's release (0) is the other side
        a.kind = Action::Step;
        a.step = (uint32_t)target == kTargetNext ? 1 : -1;
    }
    return a;
}

Takeover::Takeover()
{
    for (uint32_t i = 0; i < kParamCount; ++i)
        fSet[i] = fLast[i] = std::nanf("");
}

bool Takeover::accept(uint32_t param, float next, float current)
{
    if (param >= kParamCount)
        return false;
    const ParamInfo& info = paramInfo(param);
    if (info.boolean || listChoices(param, noInstrument()).count > 0 || param == kMallet || param == kPrepMode)
        return true;
    const float n = toNormalized(param, next), c = toNormalized(param, current), last = fLast[param];
    fLast[param] = n;
    // the slider where MIDI left it, near the knob, or passed between two of its values
    const bool ok = current == fSet[param] || std::fabs(n - c) <= kPickup || (!std::isnan(last) && (last - c) * (n - c) <= 0.0f);
    if (ok)
        fSet[param] = next;
    return ok;
}

int MidiRouter::gather(uint32_t target, int steps)
{
    int& g = fGathered[target];
    if ((g > 0 && steps < 0) || (g < 0 && steps > 0))
        g = 0;
    g += steps;
    const int entries = g / kRelativeStepsPerEntry; // toward zero: the rest keeps its sign
    g -= entries * kRelativeStepsPerEntry;
    return entries;
}

void MidiRouter::setLearn(uint32_t learnIn)
{
    if (learnIn != fLearnIn) { // another wait (or none): catch again
        fLearnIn = learnIn;
        fCaught = false;
    }
}

MidiRouter::Action MidiRouter::program(uint8_t program, const TieTable& t) const
{
    Action a;
    if (t.programChange()) {
        a.kind = Action::Program;
        a.program = program;
    }
    return a;
}

} // namespace pe
