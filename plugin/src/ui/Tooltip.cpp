#include "Tooltip.hpp"
#include "Instruments.hpp"
#include "Params.hpp"

#include <algorithm>

namespace pe {

void TooltipTimer::hover(int row, uint64_t nowMs)
{
    if (row == fRow)
        return;
    fRow = row;
    fSince = nowMs;
    fDismissed = false;
}

void TooltipTimer::dismiss() { fDismissed = true; }

int TooltipTimer::visible(uint64_t nowMs, bool busy) const
{
    return fRow >= 0 && !fDismissed && !busy && nowMs >= fSince + kTooltipDelayMs ? fRow : -1;
}

Rect tooltipRect(const Rect& row, float labelRight, float w, float h, float viewW, float viewH)
{
    constexpr float m = 4;
    w = std::max(1.0f, std::min(w, viewW - 2 * m));
    const float x = std::max(m, std::min(labelRight, viewW - w - m));
    float y = row.y + row.h + m;
    if (y + h > viewH - m)
        y = row.y - h - m;
    y = std::max(m, std::min(y, viewH - h - m));
    return { x, std::max(m, y), w, h };
}

std::string tooltipTitle(uint32_t param, const InstrumentEntry& e)
{
    return param == kHammerHardness ? displayName(param, e) : paramInfo(param).name;
}

const char* const kPanicTitle = "Panic";
const char* const kPanicDescription = "Silence at once: every note, the strings ringing along and the room's "
                                      "tail stop within 20 ms; every key and pedal is released (the Sustain "
                                      "latch stays as set). MIDI CC120 (all sound off) does the same.";
const char* const kMenuTitle = "Menu";
const char* const kMenuDescription = "Zoom: the window's size. Copy CLI: a command line (the standalone's render) for the current "
                                     "settings to the clipboard (the instrument, the sample rate and every setting "
                                     "that differs from the default; replace in.mid and out.wav with your files). "
                                     "Reset: every parameter but the instrument back to its default, the velocity "
                                     "curve straight (click twice).";
const char* const kCurveTitle = "Velocity curve";
const char* const kProgramChangeTitle = "Program change selects the instrument";
const char* const kProgramChangeDescription = "A MIDI program change selects the instrument: program 1 the menu's first, 2 the "
                                              "second, and so on, on any channel. Off by default: many MIDI files start with a "
                                              "program change and would switch the instrument.";
const char* const kTakeoverTitle = "Soft takeover";
const char* const kTakeoverDescription = "A knob or fader tied to a slider takes it over only once it reaches the slider's "
                                         "value, after the slider moved otherwise (the mouse, the host, a preset): the slider "
                                         "never jumps. Lists, switches and relative knobs are not affected.";
const char* const kKnobDescription = "Turn an endless knob in relative mode to step through the instrument menu, one "
                                     "instrument per 3 clicks either way (past the end it starts over). Click learn, then "
                                     "turn the knob.";
const char* const kStepDescription = "Steps through the instrument menu once per press of the button tied to it (past the "
                                     "end it starts over). Click learn, then press a button or pad on your controller. "
                                     "A + and a − button on one CC (one sends 127, the other 0) can be Next and "
                                     "Previous both: learn each with its own button.";
const char* const kCurveDescription = "How each note's velocity (from the keyboard or a MIDI file, across) becomes "
                                      "the velocity the instrument plays (up). Steep over a range spreads the notes "
                                      "there apart, flat squeezes them together; raise the left end for a "
                                      "minimum loudness, lower the right end to cap it. The dots: the last notes "
                                      "played; the bars below: where the notes of the last 30 s fell (a "
                                      "double-click on them clears them).";
const char* const kCurveGestures = "Click to add a point, drag to move one, double-click to remove it";

const char* tooltipGestures(Kind kind, bool learnable)
{
    switch (kind) {
    case Kind::Slider:
        return learnable ? "Drag · Shift+drag: fine · click the value to type · double-click: default · right-click: MIDI learn"
                         : "Drag · Shift+drag: fine · click the value to type · double-click: default";
    case Kind::Toggle: return learnable ? "Click to switch on or off · right-click: MIDI learn" : "Click to switch on or off";
    case Kind::Dropdown: return learnable ? "Click to choose · right-click: MIDI learn" : "Click to choose";
    }
    return "";
}

const char* const kMorphAddTitle = "Add an instrument";
const char* const kMorphAddDescription = "Adds an instrument to the morph: only those that can mix with the chosen one "
                                         "are offered, up to five in all. It joins at 0 % while the point is on the "
                                         "chosen instrument's corner: move the point to hear it.";
const char* const kMorphPadTitle = "Morph";
const char* const kMorphPadDescription = "The point picks the mix: a corner is that instrument alone, an edge mixes its "
                                         "two ends, inside every instrument has a share (with four or more, the list's "
                                         "order decides which pairs an edge mixes). A move rebuilds the instrument in "
                                         "the background (about 0.3 s); notes held across a rebuild fade out.";
const char* const kMorphPadGestures = "Drag or click to move the point · Right-click: MIDI learn of Morph X and Y";
const char* const kMorphMainDescription = "The chosen instrument: the pad's top corner, its body (where the microphones "
                                          "go). Change it with the instrument menu; the partners that can mix with "
                                          "the new one stay.";
const char* const kMorphShareDescription = "This instrument's share of the morph, from where the point is on the pad.";
const char* const kMorphUpDescription = "Moves the instrument one place up the list: its corner moves on the pad, and "
                                        "with it the mix where the point stays.";
const char* const kMorphDownDescription = "Moves the instrument one place down the list: its corner moves on the pad, "
                                          "and with it the mix where the point stays.";
const char* const kMorphRemoveDescription = "Removes the instrument from the morph. The point goes to the middle (to the "
                                            "chosen instrument's corner when it was the last partner).";
} // namespace pe
