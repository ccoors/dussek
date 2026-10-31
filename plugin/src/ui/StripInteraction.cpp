#include "StripInteraction.hpp"

namespace pe {

StripInteraction::StripInteraction(StripHost host)
    : fHost(std::move(host)),
      fPlayer([this](int k, int v) { fHost.noteOn(k, v); }, [this](int k) { fHost.noteOff(k); })
{
}

void StripInteraction::endPedal()
{
    // the lever springs back: the UI pedal is only down while dragged (never left on, never saved on)
    if (!fPedalDrag) return;
    fPedalDrag = false;
    fPedalSent = 0;
    fHost.pedal(0.0f, false, true);
}

void StripInteraction::endSostenuto()
{
    if (!fSostenutoDrag) return;
    fSostenutoDrag = false;
    fHost.sostenuto(false, false, true);
}

void StripInteraction::endSoft()
{
    if (!fSoftDrag) return;
    fSoftDrag = false;
    fSoftSent = 0;
    fHost.soft(0.0f, false, true);
}

void StripInteraction::cancel()
{
    fKeyDrag = false;
    fPlayer.release();
    endPedal();
    endSostenuto();
    endSoft();
}

bool StripInteraction::release()
{
    const bool was = dragging();
    cancel();
    return was;
}

StripInteraction::Press StripInteraction::press(float x, float y)
{
    // a release we never saw (lost outside): no ghost glissando or pedal, wherever this press lands
    if (dragging()) cancel();
    const StripLayout& l = fLayout;
    if (y < l.top) return Press::Outside;
    if (fLevel >= 2 && y < l.top + l.panelH) return Press::Panel;
    if (l.keyboardTab.contains(x, y)) { fHost.levelToggle(); return Press::Ours; }
    if (fLevel >= 1 && l.analysisTab.contains(x, y)) { fHost.analysisToggle(); return Press::Ours; }
    if (fLevel >= 1 && l.vizTab.contains(x, y)) { fHost.vizToggle(); return Press::Ours; }
    if (fLevel >= 1 && l.strip.pedal.contains(x, y)) {
        fPedalDrag = true;
        fPedalSent = pedalDepthAt(l.strip, y);
        fHost.pedal(fPedalSent, true, false);
        return Press::Ours;
    }
    if (fLevel >= 1 && l.strip.soft.contains(x, y)) {
        if (fSoftEnabled) {
            fSoftDrag = true;
            fSoftSent = softDepthAt(l.strip, y);
            fHost.soft(fSoftSent, true, false);
        }
        return Press::Ours;
    }
    if (fLevel >= 1 && l.strip.sostenuto.contains(x, y)) {
        fSostenutoDrag = true;
        fHost.sostenuto(true, true, false);
        return Press::Ours;
    }
    if (fLevel >= 1 && l.strip.keysArea.contains(x, y)) {
        fKeyDrag = true;
        fPlayer.press(l.strip, x, y);
        return Press::Ours;
    }
    return Press::Ours; // the rest of the header and strip belongs to us (no click-through to the controls behind)
}

bool StripInteraction::motion(float x, float y)
{
    if (fPedalDrag) {
        fPedalSent = pedalDepthAt(fLayout.strip, y);
        fHost.pedal(fPedalSent, false, false);
        return true;
    }
    if (fSoftDrag) {
        fSoftSent = softDepthAt(fLayout.strip, y);
        fHost.soft(fSoftSent, false, false);
        return true;
    }
    if (fSostenutoDrag) return true; // held: it takes the motion, nothing to send
    if (fKeyDrag) { fPlayer.move(fLayout.strip, x, y); return true; }
    return false;
}

} // namespace pe
