/*
 * The plugin's UI: a NanoVG window that draws the layout (Layout.hpp) and the
 * microphone drawing (MicView.hpp) and turns pointer and key events into parameter
 * edits. All logic worth testing lives in the pure modules (ui_tests).
 */
#include "DistrhoUI.hpp"
#include "DistrhoPluginUtils.hpp"
#include "DistrhoStandaloneUtils.hpp"
#include "OpenGL-include.hpp"
#include "About.hpp"
#include "CurveEdit.hpp"
#include "FeltEditor.hpp"
#include "FeltFetch.hpp"
#include "DiagView.hpp"
#include "NoteHistory.hpp"
#include "TelemetryClient.hpp"
#include "EngineValues.hpp"
#include "Format.hpp"
#include "Instruments.hpp"
#include "Layout.hpp"
#include "MicMap.hpp"
#include "MicView.hpp"
#include "MidiMap.hpp"
#include "Morph.hpp"
#include "Panel.hpp"
#include "Paths.hpp"
#include "Params.hpp"
#include "PedalMixer.hpp"
#include "Presets.hpp"
#include "Theme.hpp"
#include "Tooltip.hpp"
#include "StateVersion.hpp"
#include "UiState.hpp"
#include "Widgets.hpp"

#include <algorithm>
#include <chrono>
#include <functional>
#include <cmath>
#include <cstdlib>
#include <cstring>

START_NAMESPACE_DISTRHO
using namespace pe;

/// the instrument menu's last entry
static const char* const kOpenFolder = "Open instrument folder…";

class PianoUI : public UI
{
public:
    PianoUI()
        : UI(DISTRHO_UI_DEFAULT_WIDTH, DISTRHO_UI_DEFAULT_HEIGHT)
    {
        loadSharedResources();
        // the standalone on its own audio (no JACK): the MIDI inputs present open at start, others
        // by the MIDI tab's Rescan (a request reopens them all, cutting held notes)
        if (isUsingNativeAudio() && supportsMIDI() && !isMIDIEnabled())
            requestMIDI();
        fCatalog = sharedCatalog(resources());
        fChoice = choose(*fCatalog, "");
        loadLicense();
        for (uint32_t i = 0; i < kParamCount; ++i)
            fValues[i] = paramInfo(i).def;
        for (uint32_t i = kParamCount; i < kTotalParams; ++i)
            fValues[i] = diagInfo(i).def;
        fDiag.setConcertPitch(fValues[kConcertPitch]);
        fHostScale = getScaleFactor();
        const double s = fHostScale;
        fScaleBefore = s;
        uint32_t minW, minH;
        updateConstraints(s, minW, minH);
        // the controls keep their default view; the diagnostics' header row comes below it
        // (in DISTRHO_UI_DEFAULT_HEIGHT, so hosts asking before the UI exists get it right)
        static_assert(DISTRHO_UI_DEFAULT_HEIGHT == (int)(kDefaultHeight + DiagView::kHeaderH), "default window height");
        if (s != 1.0)
            setSize((uint)(kDefaultWidth * s), (uint)((kDefaultHeight + DiagView::kHeaderH) * s));
        refreshGeometry();
        relayout();
    }

    ~PianoUI() override
    {
        fDiag.cancel();
        if (fLearnTarget >= 0)
            setDiag(kDiagLearnIn, 0.0f); // a closed UI leaves no control waiting: the DSP would keep the next CC
        for (uint32_t index : { (uint32_t)kDiagPedalIn, (uint32_t)kDiagSostenutoIn, (uint32_t)kDiagSoftIn }) {
            if (fValues[index] != 0.0f) { // never leave a UI pedal down behind a closed UI
                editParameter(index, true);
                setParameterValue(index, 0.0f);
                editParameter(index, false);
            }
        }
    }

protected:
    // ---- host → UI
    void parameterChanged(uint32_t index, float value) override
    {
        if (index >= kTotalParams)
            return;
        if (index >= kParamCount) {
            fValues[index] = value;
            if (index >= kDiagKeys0 && index <= kDiagKeys7) {
                float words[8];
                for (int i = 0; i < 8; ++i) words[i] = fValues[kDiagKeys0 + i];
                fDiag.setKeyWords(words);
            } else if (index == kDiagPedalOut) {
                fDiag.setPedal(value); // the engine's depth (MIDI CC64 and the lever)
            } else if (index == kDiagSoftOut) {
                fDiag.setSoft(value);
            } else if (index == kDiagSostenutoOut) {
                fDiag.setSostenuto(value >= 0.5f);
            } else if (index == kDiagToken) {
                fDiag.setToken((uint32_t)value); // the DSP's telemetry region (0: none)
                if ((uint32_t)value != fNoteToken) { // another region: its value mirror's sequences anew
                    fNoteToken = (uint32_t)value;
                    fMirrorSeen = false;
                }
                fNoteClient.open((uint32_t)value, false); // the Touch tab's notes, beside the diagnostics
            } else if (index == kDiagLearnIn && value != 0.0f && fLearnTarget < 0) {
                setDiag(kDiagLearnIn, 0.0f); // a wait this UI did not start (a saved session, a UI closed mid-wait)
            } else if (index == kDiagLearnedOut) {
                learned(value);
            } else if (index == kDiagInstrumentOut) {
                midiInstrument(value);
            } else if (index == kDiagBuildingOut && (tab() == kTabMorph || tab() == kTabFelt)) {
                repaint(); // "building…" under the pad, "measuring…" under the felt's graph
            } else if (index == kDiagFeltOriginalOut) {
                fFeltOriginal = value >= 0.5f; // what plays (this UI's click, another UI's, a fresh activation)
                repaint();
            }
            return; // the diagnostics never refresh the controls
        }
        fValues[index] = value;
        if (index == kConcertPitch)
            fDiag.setConcertPitch(value);
        if (index == kMorphX || index == kMorphY)
            askShippedFelt(); // the felt under the bends moves with the morph's point
        changed(index);
        repaint();
    }

    void stateChanged(const char* key, const char* value) override
    {
        // the host hands the UI the saved values as they were: migrated here as on the DSP side
        // (the `ui` key is the UI's alone; StateVersion.hpp)
        const std::string migrated = fRestore.apply(key, value != nullptr ? value : "");
        if (std::strcmp(key, kStateVersionKey) == 0)
            return;
        value = migrated.c_str();
        if (std::strcmp(key, "instrument") == 0) {
            fChoice = choose(*fCatalog, value != nullptr ? value : "");
            instrumentChanged();
            return;
        }
        if (std::strcmp(key, "felt") == 0) {
            if (fFeltDrag < 0) { // during a drag the host echoes what we set
                PeFelt fe;
                pe_felt_parse(value, &fe); // unreadable: none
                fFelt.set(fe);
            }
            repaint();
            return;
        }
        if (std::strcmp(key, "scale_file") == 0) {
            fScaleFile = value != nullptr ? value : "";
            repaint();
            return;
        }
        if (std::strcmp(key, "morph") == 0) {
            fMorphState = value != nullptr ? value : "";
            askShippedFelt();
            relayout();
            repaint();
            return;
        }
        if (std::strcmp(key, "velocity_curve") == 0) {
            if (fCurveDrag < 0) { // during a drag the host echoes what we set
                PeVelocityCurve c;
                pe_velocity_curve_parse(value, &c); // unreadable: straight
                fEdit.set(c);
            }
            repaint();
            return;
        }
        if (std::strcmp(key, "ui") != 0)
            return;
        // a host's preset (DPF writes every state into the LV2 bundle's presets, this one at its default):
        // the window stays as it is, and the plugin gets its state back
        if (value != nullptr && formatUiState(UiState {}) == value) {
            if (formatUiState(fState) != value)
                saveState();
            return;
        }
        const UiState st = parseUiState(value);
        const bool zoomChanged = st.zoom != fState.zoom;
        if (st.tab != fState.tab)
            fScroll = 0; // another tab: from its top, as a click on it
        fState = st;
        fDiag.setPanelSettings({ st.view, st.lo, st.hi, st.floorDb });
        fDiag.setLevel(st.diag);
        if (zoomChanged) {
            applyZoom();
        } else {
            uint32_t minW, minH;
            updateConstraints(fScaleBefore, minW, minH);
            relayout();
        }
        askShippedFelt(); // a session restored on the Felt tab
        repaint();
    }

    void uiScaleFactorChanged(double scaleFactor) override
    {
        fHostScale = scaleFactor;
        applyZoom();
    }
    void onResize(const ResizeEvent& ev) override
    {
        UI::onResize(ev);
        fWindowResize.hostResized(getHeight(), nowMs()); // the host-applied zoom resize
        relayout();
    }
    void uiFocus(bool focus, CrossingMode mode) override
    {
        UI::uiFocus(focus, mode);
        if (!focus)
            fDiag.cancel(); // no note or pedal drag left on
    }

    // ---- drawing
    void onNanoDisplay() override
    {
        // DPF clears color and depth, not the stencil NanoVG fills concave shapes with
        // (the grand's outline): after a resize it holds garbage that leaks into the fill.
        // NanoVG only flushes its draws at the frame's end, so this clear comes first.
        glClearStencil(0);
        glClear(GL_STENCIL_BUFFER_BIT);
        const float s = uiScale();
        beginPath(); rect(0, 0, getWidth(), getHeight()); fillColor(theme::bg); fill();
        save();
        scale(s, s);
        scissor(0, 0, getWidth() / s, viewHeight()); // the controls' part: above the diagnostics
        fontFaceId(0);
        const InstrumentEntry& inst = entry();
        // fixed: the header, the tab bar, (wide) the drawing and its rows
        for (const Row& r : fLayout.rows)
            if (!r.scrolls)
                drawRow(r, inst);
        drawTabBar(*this, fLayout, tab(), fHotTab);
        if (!fLayout.drawingScrolls)
            drawViews();
        ControlLook pl { kPanicTitle, std::string(), panicLit() ? 1.0f : 0.0f, true, fHotPanic, false, nullptr };
        drawToggle(*this, fLayout.panic, pl);
        ControlLook ml { "⋯", std::string(), fMenuList ? 1.0f : 0.0f, true, fHotMenu, false, nullptr };
        drawToggle(*this, fLayout.menu, ml);
        // the tab's content, scrolled inside its area
        save();
        intersectScissor(fLayout.content.x, fLayout.content.y, fLayout.content.w, fLayout.content.h);
        translate(0, -fScroll);
        for (const Row& r : fLayout.rows)
            if (r.scrolls)
                drawRow(r, inst);
        for (const Subheading& h : fLayout.subheadings)
            drawSubheading(*this, h);
        if (fLayout.emptyNote.w > 0)
            drawEmptyNote(*this, fLayout.emptyNote);
        if (fLayout.curve.w > 0)
            drawCurve();
        if (fLayout.programSwitch.w > 0)
            drawMidiTab();
        if (tab() == kTabMorph)
            drawMorphTab();
        if (tab() == kTabFelt)
            drawFeltTab();
        if (tab() == kTabAbout)
            drawAboutTab(*this, fLayout, fHotCopyLink);
        if (fLayout.drawingScrolls)
            drawViews();
        restore();
        // on top (window-logical): an open list, the tooltip
        if (listOpen())
            drawDropdownList(*this, fListRect, fListItems, fListSelected, fListHot, fListScroll);
        if (fMenuList && fZoomSub) { // the zoom's steps beside the menu
            int current = -1;
            for (int i = 0; i < 8; ++i)
                if (kZoomSteps[i] == fState.zoom) current = i;
            drawDropdownList(*this, fSubRect, fSubItems, current, fSubHot, fSubScroll);
        }
        if (fListParam == (int)kInstrumentRow && fSubGroup >= 0) { // the open group's instruments
            int current = -1;
            for (size_t i = 0; i < fSubIds.size(); ++i)
                if (fSubIds[i] == fChoice.id) current = (int)i;
            drawDropdownList(*this, fSubRect, fSubItems, current, fSubHot, fSubScroll);
        }
        drawTip();
        drawCopiedNote();
        restore();
        save();
        scale(s, s);
        fontFaceId(0);
        fDiag.draw(*this);
        restore();
    }

    void drawViews()
    {
        if (fLayout.topView.w <= 0)
            return;
        const uint32_t pos = micPosition();
        drawMicView(*this, fGeom, kTop, fLayout.topView, fTop, fHotView == kTop ? fHotMic : -1, fHotMarker, pos, "from above");
        drawMicView(*this, fGeom, kFront, fLayout.frontView, fFront, fHotView == kFront ? fHotMic : -1, 0, pos,
            frontAxis(fGeom) == 1 ? "from the side" : "from the front");
    }

    // ---- pointer
    bool onMouse(const MouseEvent& ev) override
    {
        // window-logical (not scrolled); a release reaches the strip wherever the pointer is
        const float wx = (float)ev.pos.getX() / uiScale(), wy = (float)ev.pos.getY() / uiScale();
        if (ev.button == kMouseButtonRight && ev.press)
            return rightClick(wx, wy);
        if (ev.button != 1)
            return false;
        fMods = ev.mod;
        if (ev.press && fLearnTarget >= 0) { // a click elsewhere ends the wait (and goes on as a click)
            cancelLearn();
            repaint();
        }
        if (ev.press) { // a release we never saw (lost outside): that control drag is over, its edit bracket closes now
            endControlDrag();
            fTip.dismiss();
        }
        if (ev.press && wy >= viewHeight()) { // a press on the diagnostics ends a text edit or an open list first
            if (fEditParam >= 0) commitEdit();
            if (listOpen()) { fListParam = -1; closeMenu(); repaint(); }
        }
        if (fDiag.mouse(ev.press, wx, wy))
            return true;
        if (!ev.press) {
            endControlDrag();
            repaint();
            return true;
        }
        fPressInContent = inContent(fLayout, wx, wy);
        const Point2 p = dragPoint(wx, wy); // layout coordinates
        if (fEditParam >= 0) {
            const Row* e = rowFor(shownRow((uint32_t)fEditParam));
            if (e != nullptr && e->value.contains(p.x, p.y))
                return true; // a click in the field being edited keeps editing
            commitEdit();
        }
        if (listOpen() && pressListBar(wx, wy)) return true;
        if (listOpen()) { clickList({ wx, wy }); return true; }
        if (fLayout.panic.contains(wx, wy)) { clickPanic(); return true; }
        if (fLayout.menu.contains(wx, wy)) { openMenu(); return true; }
        const Tab t = tabAt(fLayout, wx, wy);
        if (t != kTabCount) { selectTab(t); return true; }
        if (fLayout.curve.w > 0 && fPressInContent && pressCurve(p, ev.time))
            return true;
        if (tab() == kTabFelt && fPressInContent && pressFelt(p, ev.time))
            return true;
        if (fLayout.programSwitch.w > 0 && fPressInContent && clickMidiTab(p))
            return true;
        if (tab() == kTabMorph && fPressInContent && pressMorph(p))
            return true;
        if (tab() == kTabAbout && fPressInContent && fLayout.aboutCopy.contains(p.x, p.y)) {
            copyLink();
            return true;
        }
        if (viewAt(fLayout, wx, wy, fScroll) >= 0 && clickMicViews(p)) // only where the drawing is shown
            return true;
        const int i = rowAt(fLayout, wx, wy, fScroll);
        if (i >= 0) { clickRow(fLayout.rows[(size_t)i], p, ev.time); return true; }
        return false;
    }

    /// closes the edit bracket of a control drag (slider or microphone) that is still open
    void endControlDrag()
    {
        fBarDrag = 0;
        if (fCurveDrag >= 0) { fCurveDrag = -1; commitCurve(); }
        if (fFeltDrag >= 0) { fFeltDrag = -1; fFelt.dropped(); commitFelt(); } // the felt measured once, on the drop
        if (fPadDrag) { fPadDrag = false; editParameter(kMorphX, false); editParameter(kMorphY, false); }
        if (fDragParam >= 0) { editParameter((uint32_t)fDragParam, false); fDragParam = -1; }
        if (fFree.active()) {
            for (uint32_t p = kMicPosition; p <= kMicRPitch; ++p) editParameter(p, false);
            fFree.end();
            loadGeometry();
            relayout();
        }
        if (fAim.active()) {
            for (uint32_t p = kMicPosition; p <= kMicRPitch; ++p) editParameter(p, false);
            fAim.end();
            loadGeometry();
            relayout();
        }
        if (fMic.active()) { editParameter(kMicDistance, false); editParameter(kMicSpacing, false); fMic.end(); loadGeometry(); relayout(); }
    }

    bool onMotion(const MotionEvent& ev) override
    {
        const float wx = (float)ev.pos.getX() / uiScale(), wy = (float)ev.pos.getY() / uiScale();
        fPointerX = wx;
        fPointerY = wy;
        fPointerInControls = wy < viewHeight();
        if (fBarDrag != 0 && listOpen()) { // a list's thumb follows the pointer
            if (fBarDrag == 2)
                fSubScroll = listScrollAtThumb(fSubRect, (uint32_t)fSubItems.size(), wy - fBarGrab);
            else
                fListScroll = listScrollAtThumb(fListRect, (uint32_t)fListItems.size(), wy - fBarGrab);
            repaint();
            return true;
        }
        // a control's drag keeps the pointer even over the diagnostics
        if (fDragParam < 0 && !micDragging() && fCurveDrag < 0 && fFeltDrag < 0 && fDiag.motion(wx, wy)) {
            fPointerInControls = false;
            updateHover(-1e9f, -1e9f); // nothing above stays highlighted
            return true;
        }
        if (fDragParam < 0 && !micDragging() && fCurveDrag < 0 && fFeltDrag < 0) // a drag keeps where it was pressed
            fPressInContent = inContent(fLayout, wx, wy);
        const Point2 p = dragPoint(wx, wy);
        if (fPadDrag) {
            movePad(p);
            return true;
        }
        if (fCurveDrag >= 0) {
            fEdit.move(fCurveDrag, fLayout.curve, p.x, p.y);
            commitCurve(); // live: the engine follows the drag
            return true;
        }
        if (fFeltDrag >= 0) {
            fFelt.move(fFeltDrag, fLayout.feltGraph, p.x, p.y); // shown now, sent on the drop
            repaint();
            return true;
        }
        if (fDragParam >= 0) {
            const Row* r = rowFor((uint32_t)fDragParam);
            if (r != nullptr) {
                const bool fine = ev.mod & kModifierShift;
                if (fine != fDragFine) { // Shift pressed or released: go on from here
                    fDragFine = fine;
                    fDragRelative = true;
                    fDragX0 = p.x;
                    fDragNorm0 = toNormalized((uint32_t)fDragParam, fValues[fDragParam]);
                }
                const float norm = fDragRelative ? fDragNorm0 + (p.x - fDragX0) / r->control.w * (fine ? 0.1f : 1.0f)
                                                 : (p.x - r->control.x) / r->control.w;
                setParam((uint32_t)fDragParam, fromNormalized((uint32_t)fDragParam, norm));
            }
            return true;
        }
        if (fAim.active()) {
            double a, b;
            (fMicView == kTop ? fTop : fFront).toWorld(p, a, b);
            const int i = fAim.which();
            double yaw = fValues[kMicLYaw + 3 * i], pitch = fValues[kMicLPitch + 3 * i];
            double ma, mb;
            viewAB(fGeom, fMicView, fGeom.mics[i], ma, mb);
            fAim.update(a - ma, b - mb, yaw, pitch);
            if (fMicView == kTop)
                setParam((uint32_t)(kMicLYaw + 3 * i), (float)yaw);
            else
                setParam((uint32_t)(kMicLPitch + 3 * i), (float)pitch);
            loadGeometry(); // the arrow follows the pointer; the views stay as they were
            repaint();
            return true;
        }
        if (fFree.active()) {
            double a, b, q[3];
            (fMicView == kTop ? fTop : fFront).toWorld(p, a, b);
            fFree.update(a, b, q);
            PeMicSetup s = customSetup();
            for (int k = 0; k < 3; ++k) s.pos[fFree.which()][k] = q[k];
            pe_mic_clamp((uint32_t)entryBody(entry()), engineLid(fValues), &s);   // the drag shows only allowed points
            writeCustomPos(fFree.which(), s.pos[fFree.which()]);
            loadGeometry(); // the pair follows the pointer; the views stay as they were (re-fit on release)
            repaint();
            return true;
        }
        if (fMic.active()) {
            double a, b;
            (fMicView == kTop ? fTop : fFront).toWorld(p, a, b);
            const DragValues v = fMic.update(a, b);
            setParam(kMicDistance, v.distance);
            setParam(kMicSpacing, v.spacingCm);
            // the pair follows the pointer; the views stay as they were (re-fit on release)
            pe_mic_geometry((uint32_t)entryBody(entry()), micPosition(), v.spacingCm / 100.0, v.distance, engineLid(fValues), nullptr, &fGeom);
            repaint();
            return true;
        }
        updateHover(wx, wy);
        return false;
    }

    bool onScroll(const ScrollEvent& ev) override
    {
        const float wx = (float)ev.pos.getX() / uiScale(), wy = (float)ev.pos.getY() / uiScale();
        if (listOpen()) { // an open list: the list under the pointer scrolls (a row per step), nothing beneath it
            const float px = (float)ev.delta.getY() * kRowH;
            if (subOpen() && fSubRect.contains(wx, wy))
                fSubScroll = listWheel(fSubRect, (uint32_t)fSubItems.size(), fSubScroll, px);
            else if (fListRect.contains(wx, wy))
                fListScroll = listWheel(fListRect, (uint32_t)fListItems.size(), fListScroll, px);
            updateHover(wx, wy);
            repaint();
            return true;
        }
        if (wy >= viewHeight())
            return true; // the diagnostics: nothing beneath them moves
        // the wheel only scrolls the tab's content, never a control
        const float s = wheelScroll(fLayout, wx, wy, fScroll, (float)ev.delta.getY() * (kLineH + kLineGap));
        fTip.dismiss();
        if (s != fScroll) {
            fScroll = s;
            updateHover(wx, wy);
        }
        repaint();
        return true;
    }

    // ---- typing a value
    bool onKeyboard(const KeyboardEvent& ev) override
    {
        if (ev.press && ev.key == kKeyEscape && fLearnTarget >= 0) {
            cancelLearn();
            repaint();
            return true;
        }
        if (fEditParam < 0 || !ev.press)
            return false;
        if (ev.key == kKeyEnter) commitEdit();
        else if (ev.key == kKeyEscape) fEditParam = -1;
        else if (ev.key == kKeyBackspace && !fEditText.empty()) fEditText.pop_back();
        else return false;
        repaint();
        return true;
    }

    bool onCharacterInput(const CharacterInputEvent& ev) override
    {
        const size_t most = presetRow((uint32_t)std::max(fEditParam, 0)) ? 40 : 24; // a preset's name, a value
        if (fEditParam < 0 || ev.character < 32 || ev.character == 127 || fEditText.size() > most)
            return false;
        fEditText += ev.string;
        repaint();
        return true;
    }

private:
    float fValues[kRowCount] {}; // + the Instrument and Preset rows' (no parameters, always 0)
    // the Touch tab: the velocity curve (the "velocity_curve" state) as edited, the recent notes
    CurveEdit fEdit;
    // the Felt tab: the player's bends (the "felt" state) over the shipped felt, read off this thread
    FeltEditor fFelt;
    FeltFetch fFeltFetch;
    bool fFeltLoaded = true; // the last shipped felt asked for loaded (false: the tab says so)
    std::string fFeltFor;       // the instrument and morph point the shipped felt was asked for
    int fFeltDrag = -1, fFeltClickPoint = -1;
    uint32_t fFeltClickTime = 0;
    bool fFeltOriginal = false; // the Original toggle, as kDiagFeltOriginalOut says it plays
    NoteHistory fNotes;
    TelemetryClient fNoteClient;
    int fCurveDrag = -1, fCurveHot = -1, fCurveClickPoint = -1;
    uint32_t fCurveClickTime = 0, fBarsClickTime = 0;
    bool fHotResetCurve = false;
    uint64_t fNotesPaintMs = 0;
    CatalogPtr fCatalog; // the instruments found (Catalog.hpp)
    StateRestore fRestore; // the restored state's version (StateVersion.hpp)
    Choice fChoice;      // the "instrument" state: the id and its entry (nullptr: missing)
    UiState fState;
    Layout fLayout;
    float fScroll = 0;
    PeMicGeometry fGeom {};
    ViewTransform fTop, fFront;
    MicDrag fMic;
    FreeDrag fFree;
    AimDrag fAim; // never at once with fFree (one press starts one drag)
    View fMicView = kTop, fHotView = kTop;
    int fHotMic = -1, fHotMarker = 0;
    int fHotRow = -1;
    int fHotTab = -1;
    TooltipTimer fTip;
    int fTipShown = -1; // the row whose tooltip is drawn (uiIdle repaints when it changes)
    bool fPressInContent = false; // the press (or the pointer, without a drag) is over the content area
    float fPointerX = -1e9f, fPointerY = -1e9f; // window-logical, at the last motion
    bool fPointerInControls = false;
    int fDragParam = -1;
    float fDragX0 = 0, fDragNorm0 = 0;
    uint fMods = 0; // the modifiers at the last press
    bool fDragRelative = false, fDragFine = false; // relative: from the value at the press (label, Shift)
    uint32_t fLastClickTime = 0;
    int fLastClickParam = -1;
    int fEditParam = -1;
    std::string fEditText;
    int fListParam = -1;
    // the instrument menu: its groups in the list, the hovered (or clicked) group's instruments beside it
    int fSubGroup = -1, fSubHot = -1;
    Rect fSubRect;
    std::vector<std::string> fSubItems;
    std::vector<std::string> fSubIds; // the open group's instruments
    bool fMenuList = false, fZoomSub = false; // the header menu open; its zoom steps beside it
    Rect fListRect;
    std::vector<std::string> fListItems;
    /// the values the open list's rows stand for (empty: min + row)
    std::vector<float> fListValues;
    int fListSelected = -1, fListHot = -1;
    /// the lists' scrolls (a list cut to the view, Layout.hpp); a press on a bar drags its thumb
    /// (fBarDrag 1: the list's, 2: the submenu's; fBarGrab: the pointer below the thumb's top)
    float fListScroll = 0, fSubScroll = 0;
    int fBarDrag = 0;
    float fBarGrab = 0;

    // the host's scale: from the window at creation, then from uiScaleFactorChanged (DGL's
    // own value is fixed at creation)
    double fHostScale = 1.0;
    float uiScale() const { return (float)fHostScale * fState.zoom / 100.0f; }
    float viewHeight() const { return getHeight() / uiScale() - fDiag.height(); } // the controls' part of the window
    /// the chosen instrument's entry (a missing one: noInstrument)
    const InstrumentEntry& entry() const { return fChoice.entry != nullptr ? *fChoice.entry : noInstrument(); }
    const char* resources() const
    {
        const char* const bundle = getBundlePath();
        return bundle != nullptr ? getResourcePath(bundle) : nullptr;
    }
    Tab tab() const { return (Tab)std::min<uint32_t>(fState.tab, kTabCount - 1); }
    UiContext ctx() const
    {
        UiContext c { fChoice.entry, micPosition(), prepMode() };
        for (const Tie& t : fMap.ties())
            if (t.target < kParamCount)
                c.tied.set(t.target);
        c.midiTies = (uint32_t)fMap.ties().size();
        c.nextTied = fMap.find(kTargetNext) != nullptr;
        c.previousTied = fMap.find(kTargetPrevious) != nullptr;
        c.knobTied = fMap.find(kTargetKnob) != nullptr;
        c.partners = (uint32_t)partners().size();
        c.nativeAudio = isUsingNativeAudio();
        c.license = fLicenseFound ? &fLicense : nullptr;
        return c;
    }
    uint32_t stored(uint32_t i) const { return (uint32_t)std::max(0L, std::lround(fValues[i])); }
    /// what is shown depends on these
    static bool affectsLayout(uint32_t i) { return i == kMicPosition || i == kPrepMode; }
    /// window-logical → layout: inside the content (for a drag: pressed there) the scroll is added
    Point2 dragPoint(float wx, float wy) const
    {
        float x = wx, y = wy;
        toLayout(fLayout, fScroll, fPressInContent, x, y);
        return { x, y };
    }
    uint32_t prepMode() const { return (uint32_t)std::max(0L, std::lround(fValues[kPrepMode])); }
    uint32_t micPosition() const { return (uint32_t)std::lround(fValues[kMicPosition]); }
    static bool isMicParam(uint32_t i) { return i == kLid || i == kLidOff || (i >= kMicPosition && i <= kMicRPitch); }
    bool micDragging() const { return fMic.active() || fFree.active() || fAim.active(); }

    /// the free pair as the parameters hold it
    PeMicSetup customSetup() const
    {
        PeMicSetup s {};
        for (int i = 0; i < 2; ++i)
            for (int k = 0; k < 3; ++k)
                s.pos[i][k] = fValues[kMicLX + i * 3 + k];
        for (int i = 0; i < 2; ++i) { // per microphone: pattern (percent), yaw, pitch
            s.pattern[i] = fValues[kMicLPattern + 3 * i] / 100.0;
            s.yaw[i] = fValues[kMicLYaw + 3 * i];
            s.pitch[i] = fValues[kMicLPitch + 3 * i];
        }
        s.wall = fValues[kMicWall];
        return s;
    }

    void writeCustomPos(int which, const double p[3])
    {
        for (int k = 0; k < 3; ++k)
            setParam((uint32_t)(kMicLX + which * 3 + k), (float)p[k]);
    }

    void loadGeometry()
    {
        const PeMicSetup s = customSetup();
        pe_mic_geometry((uint32_t)entryBody(entry()), micPosition(), fValues[kMicSpacing] / 100.0, fValues[kMicDistance], engineLid(fValues), &s, &fGeom);
    }

    /// a drag from a preset starts from the preset's microphones: copy them into the free pair and switch to Custom
    /// (a preset reports pattern 0, omni, and the aim towards the board: the sound stays until a pattern changes)
    void seedCustomFromGeometry()
    {
        const PeMicGeometry g = fGeom; // each setParam may refresh fGeom
        for (int i = 0; i < 2; ++i) {
            writeCustomPos(i, g.mics[i]);
            setParam((uint32_t)(kMicLPattern + 3 * i), 0.0f);
            setParam((uint32_t)(kMicLYaw + 3 * i), (float)g.yaw[i]);
            setParam((uint32_t)(kMicLPitch + 3 * i), (float)g.pitch[i]);
        }
        if (std::isfinite(g.wall_y))
            setParam(kMicWall, (float)g.wall_y);
        setParam(kMicPosition, 5.0f);
    }
    float clampTo(uint32_t i, float v) const { const ParamInfo& p = paramInfo(i); v = std::min(p.max, std::max(p.min, v)); return p.integer ? std::round(v) : v; }

    void setParam(uint32_t i, float v)
    {
        v = clampTo(i, v);
        if (!std::isfinite(v) || v == fValues[i])
            return;
        fValues[i] = v;
        setParameterValue(i, v);
        const uint32_t other = exclusiveStop(i);
        if (other < kParamCount && v >= 0.5f && fValues[other] >= 0.5f) {
            editParameter(other, true);
            setParam(other, 0.0f);
            editParameter(other, false);
        }
        changed(i);
        repaint();
    }

    /// after a parameter changed: the drawing's geometry and (where what is shown depends on it) the
    /// layout; a running microphone drag owns the drawing until its release
    /// another instrument (chosen here or by the host's state): its body, its controls
    void instrumentChanged()
    {
        loadLicense();
        askShippedFelt(); // the Felt tab's graphs: the new instrument's felt
        if (micDragging())
            return;
        loadGeometry();
        relayout();
        repaint();
    }

    void chooseInstrument(const std::string& id)
    {
        fChoice = choose(*fCatalog, id);
        // the new instrument's shipped felt: the bends were the old one's. Cleared first: a build
        // between the two states then plays the old instrument unbent (cheap, dropped), never
        // measures the old bends on the new one (3–7 s)
        fFelt.reset();
        fFeltDrag = -1;
        setState("felt", "");
        setState("instrument", id.c_str());
        // the partners that may join this instrument stay
        const std::vector<std::string> before = parsePartners(fMorphState), kept = partners();
        if (kept != before) {
            fMorphState = formatPartners(kept);
            setState("morph", fMorphState.c_str());
            if (kept.size() != before.size())
                placeMorphPoint(morphPointAfter(before.size(), kept.size(), { fValues[kMorphX], fValues[kMorphY] }));
        }
        instrumentChanged();
    }

    void changed(uint32_t i)
    {
        if (micDragging())
            return;
        if (isMicParam(i))
            loadGeometry();
        if (affectsLayout(i))
            relayout();
        else if (isMicParam(i))
            fitViews();
    }


    void refreshGeometry()
    {
        loadGeometry();
        fitViews();
    }

    void fitViews()
    {
        if (fLayout.topView.w <= 0)
            return; // narrow, not the Mics tab: no drawing
        fTop = fitView(fGeom, kTop, fLayout.topView);
        fFront = fitView(fGeom, kFront, fLayout.frontView);
    }

    void relayout()
    {
        const float s = uiScale();
        fDiag.setSoftEnabled(entrySoftPedal(entry())); // plucked strings: the soft pedal does nothing
        int lo = 21, hi = 108;
        entryRecorded(entry(), lo, hi);
        fDiag.setRecorded(lo, hi);
        fDiag.setReversedKeys(entry().info.reversed_keys);
        fDiag.setBars(entry().info.bars);
        fLayout = computeLayout(getWidth() / s, viewHeight(), tab(), ctx());
        fScroll = std::min(fScroll, maxScroll(fLayout));
        fDiag.layout(getWidth() / s, getHeight() / s);
        fHotRow = fHotTab = -1; // row indices change with the layout
        fTipShown = -1;
        fTip.hover(-1, nowMs());
        fitViews();
        if (fPointerInControls)
            updateHover(fPointerX, fPointerY); // what is under the pointer now
    }

    /// keep a small controls viewport available beneath the expanded diagnostics
    void updateConstraints(double scale, uint32_t& minW, uint32_t& minH)
    {
        minimumWindowSize(fHostScale, scale, fDiag.height(), minW, minH);
        setGeometryConstraints(minW, minH, false, false);
    }

    /// the window's overall logical size changes only for explicit zoom/host-scale changes
    void applyZoom()
    {
        const double s = fHostScale * fState.zoom / 100.0;
        uint32_t minW, minH;
        updateConstraints(s, minW, minH);
        const uint32_t h = fWindowResize.zoomed(getHeight(), fScaleBefore, s, minH, nowMs());
        setSize((uint)std::max<double>(minW, std::lround(getWidth() / fScaleBefore * s)), (uint)h);
        fScaleBefore = s;
        relayout();
    }
    double fScaleBefore = 1.0; // the scale the current window size was made for
    static uint64_t nowMs() { return (uint64_t)std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now().time_since_epoch()).count(); }
    WindowResizeTracker fWindowResize;

    void saveState() { setState("ui", formatUiState(fState).c_str()); }

    // the diagnostics at the window's bottom (window-logical coordinates, not scrolled)
    DiagView fDiag { DiagHost {
        [this](int key, int velocity) { sendNote(0, (uint8_t)(21 + key), (uint8_t)velocity); },
        [this](int key) { sendNote(0, (uint8_t)(21 + key), 0); },
        [this](float depth, bool begin, bool end) {
            if (begin) editParameter(kDiagPedalIn, true);
            setParameterValue(kDiagPedalIn, depth);
            fValues[kDiagPedalIn] = depth;
            if (end) editParameter(kDiagPedalIn, false);
        },
        [this](bool down, bool begin, bool end) {
            if (begin) editParameter(kDiagSostenutoIn, true);
            setParameterValue(kDiagSostenutoIn, down ? 1.0f : 0.0f);
            fValues[kDiagSostenutoIn] = down ? 1.0f : 0.0f;
            if (end) editParameter(kDiagSostenutoIn, false);
        },
        [this](float depth, bool begin, bool end) {
            if (begin) editParameter(kDiagSoftIn, true);
            setParameterValue(kDiagSoftIn, depth);
            fValues[kDiagSoftIn] = depth;
            if (end) editParameter(kDiagSoftIn, false);
        },
        [this] { repaint(); },
        [this] { diagChanged(); } } };
    void diagChanged()
    {
        fState.diag = fDiag.level();
        const DiagView::PanelSettings ps = fDiag.panelSettings();
        fState.view = ps.view;
        fState.lo = ps.lo;
        fState.hi = ps.hi;
        fState.floorDb = ps.floorDb;
        saveState();
        uint32_t minW, minH;
        updateConstraints(fScaleBefore, minW, minH);
        relayout();
        repaint();
    }

    void selectTab(Tab t)
    {
        if (t == tab())
            return;
        fState.tab = t;
        fScroll = 0;
        saveState();
        relayout();
        askShippedFelt();
        repaint();
    }

    /// the Temperament row's text for a Scala file: its name, or why it does not play
    std::string scaleLabel() const
    {
        if (fScaleFile.empty())
            return "Scala file: none";
        char msg[256];
        if (pe_scale_info(fScaleFile.c_str(), msg, sizeof(msg)) == 0)
            return "Scala file: unreadable";
        const size_t slash = fScaleFile.find_last_of('/');
        return fScaleFile.substr(slash == std::string::npos ? 0 : slash + 1);
    }

    /// the file browser for the instrument menu's last entry: an instrument folder anywhere, by its
    /// instrument.json (`uiFileBrowserSelected` takes the answer)
    void chooseInstrumentFolder()
    {
        fBrowsing = Browsing::Instrument;
        FileBrowserOptions o;
        o.title = "Choose an instrument folder (its instrument.json)";
        openFileBrowser(o);
    }

    /// the file browser for the Temperament menu's "Scala file" (`uiFileBrowserSelected` takes the answer)
    void chooseScaleFile()
    {
        fBrowsing = Browsing::Scale;
        FileBrowserOptions o;
        o.title = "Choose a Scala scale (.scl)";
        fScaleDir = fScaleFile.empty() ? std::string() : fScaleFile.substr(0, fScaleFile.find_last_of('/'));
        o.startDir = fScaleDir.empty() ? nullptr : fScaleDir.c_str(); // the options hold the pointer only
        openFileBrowser(o);
    }

    void uiFileBrowserSelected(const char* path) override
    {
        if (path == nullptr || path[0] == '\0')
            return; // cancelled: as it was
        if (fBrowsing == Browsing::Instrument) {
            // the folder of the file chosen (its instrument.json, or any file in it)
            std::string dir = path;
            dir = dir.substr(0, std::max<size_t>(1, dir.find_last_of('/')));
            if (Catalog::outside(dir) == nullptr) {
                d_stderr2("Dussek: %s: no instrument (an instrument.json and a data.bin the engine accepts)", dir.c_str());
                return;
            }
            chooseInstrument(dir);
            return;
        }
        char msg[256];
        if (pe_scale_info(path, msg, sizeof(msg)) == 0) {
            d_stderr2("Dussek: %s", msg);
            return; // not a scale: the temperament as it was
        }
        fScaleFile = path;
        setState("scale_file", path);
        editParameter(kTemperament, true);
        setParam(kTemperament, (float)pe_temperament_count());
        editParameter(kTemperament, false);
        repaint();
    }

    const Row* rowFor(uint32_t param) const
    {
        for (const Row& r : fLayout.rows)
            if (r.param == param)
                return &r;
        return nullptr;
    }

    void drawRow(const Row& r, const InstrumentEntry& inst)
    {
        ControlLook c;
        c.label = displayName(r.param, inst);
        c.value = r.param == kInstrumentRow ? (fChoice.entry != nullptr ? morphLabel(fChoice.entry->name, partnersPlaying()) : "Missing: " + fChoice.id)
            : presetRow(r.param)          ? presetLabel(offered(r.param), currentPreset(r.param))
            : r.param == kMicPosition && currentPreset(kMicPresetRow) >= 0 ? presetLabel(offered(kMicPresetRow), currentPreset(kMicPresetRow))
            : r.param == kMicDistance     ? formatMicDistance(fValues[kMicDistance], fGeom.has_pair, fGeom.distance)
            : r.param == kMallet          ? malletLabel(inst, effectiveMallet(inst, stored(kMallet)))
            : r.param == kPrepMode        ? formatValue(kPrepMode, (float)effectivePrep(inst, stored(kPrepMode)))
            : r.param == kTemperament && fValues[kTemperament] == (float)pe_temperament_count() ? scaleLabel()
                                          : formatValue(r.param, fValues[r.param]);
        c.norm = r.param == kInstrumentRow || presetRow(r.param) ? 0.0f : toNormalized(r.param, fValues[r.param]);
        c.active = true; // only what applies is shown
        c.hot = (int)(&r - fLayout.rows.data()) == fHotRow || (int)r.param == fDragParam;
        c.editing = (int)r.param == fEditParam || (r.param == kMicPosition && fEditParam == (int)kMicPresetRow);
        c.editText = &fEditText;
        const bool waiting = (int)r.param == fLearnTarget;
        if (waiting)
            c.value = "learning…"; // the value field fits about ten characters
        if (r.compact) {
            drawCompactRow(*this, r, c);
        } else {
            switch (r.kind) {
            case Kind::Slider: drawSlider(*this, r.rect, c); break;
            case Kind::Toggle: drawToggle(*this, r.rect, c); break;
            case Kind::Dropdown: drawDropdown(*this, r.rect, c); break;
            }
        }
        if (waiting)
            drawWaiting(*this, r);
        if (r.param < kParamCount)
            if (const Tie* t = fMap.find(r.param))
                drawTieTag(*this, r, tieLabel(*t).c_str(), c.label);
    }

    bool tipBusy() const { return fDragParam >= 0 || fEditParam >= 0 || listOpen() || micDragging() || fCurveDrag >= 0; }

    /// the tooltip indices of the Panic and the menu buttons: past the rows (they are no parameters)
    int panicTip() const { return (int)fLayout.rows.size(); }
    int menuTip() const { return (int)fLayout.rows.size() + 1; }
    int curveTip() const { return (int)fLayout.rows.size() + 2; }
    /// the MIDI tab's switches, then its rows
    int programTip() const { return (int)fLayout.rows.size() + 3; }
    int takeoverTip() const { return (int)fLayout.rows.size() + 4; }
    int midiTip(size_t k) const { return (int)fLayout.rows.size() + 5 + (int)k; }
    /// the Morph tab's: 0 the plus, 1 the pad, then per row 2 + 4k + (0 the row, 1 up, 2 down, 3 remove)
    int morphTip(int i) const { return midiTip(fLayout.midiRows.size()) + i; }

    void drawTip()
    {
        if (fTipShown < 0 || !fPointerInControls || tipBusy())
            return;
        if (fTipShown == panicTip() || fTipShown == menuTip()) {
            const bool panic = fTipShown == panicTip();
            const char* title = panic ? kPanicTitle : kMenuTitle;
            const char* body = panic ? kPanicDescription : kMenuDescription;
            const float vw = getWidth() / uiScale(), vh = viewHeight();
            const float w = std::min(kTooltipW, vw - 8);
            const char* gestures = "Click";
            const Rect& b = panic ? fLayout.panic : fLayout.menu;
            const Rect box = tooltipRect(b, b.x, w, tooltipHeight(*this, w, title, body, gestures), vw, vh);
            drawTooltip(*this, box, title, body, gestures);
            return;
        }
        if (fTipShown == curveTip()) {
            const float vw = getWidth() / uiScale(), vh = viewHeight();
            const float w = std::min(kTooltipW, vw - 8);
            const Rect b = shown(fLayout.curve, true, fScroll);
            const Rect box = tooltipRect({ b.x, b.y, b.w, 1 }, b.x, w, tooltipHeight(*this, w, kCurveTitle, kCurveDescription, kCurveGestures), vw, vh);
            drawTooltip(*this, box, kCurveTitle, kCurveDescription, kCurveGestures);
            return;
        }
        if (fTipShown >= programTip() && (size_t)(fTipShown - programTip()) <= fLayout.midiRows.size() + 1) {
            const bool program = fTipShown == programTip(), takeover = fTipShown == takeoverTip(), sw = program || takeover;
            const Rect a = program ? fLayout.programSwitch : takeover ? fLayout.takeoverSwitch : fLayout.midiRows[(size_t)(fTipShown - midiTip(0))].rect;
            const uint32_t target = sw ? kParamCount : midiTarget(fLayout.midiRows[(size_t)(fTipShown - midiTip(0))]);
            if (!sw && target < kParamCount)
                return; // a control's tie: its own row has the tooltip
            const char* title = program ? kProgramChangeTitle : takeover ? kTakeoverTitle : targetName(target);
            const char* body = program ? kProgramChangeDescription : takeover ? kTakeoverDescription
                : target == kTargetKnob ? kKnobDescription : kStepDescription;
            const char* gestures = sw ? "Click to switch on or off"
                : target == kTargetKnob ? "Click learn, then turn the knob · × removes the tie"
                                        : "Click learn, then press a button · × removes the tie";
            const float vw = getWidth() / uiScale(), vh = viewHeight();
            const float w = std::min(kTooltipW, vw - 8);
            const Rect b = shown(a, true, fScroll);
            const Rect box = tooltipRect(b, b.x + std::min(b.w, 260.0f), w, tooltipHeight(*this, w, title, body, gestures), vw, vh);
            drawTooltip(*this, box, title, body, gestures);
            return;
        }
        if (fTipShown >= morphTip(0) && fTipShown < morphTip(2 + 4 * (int)fLayout.morphRows.size())) {
            const int i = fTipShown - morphTip(0);
            const std::vector<std::string> names = morphNames();
            Rect a;
            std::string title;
            const char *body = nullptr, *gestures = "Click";
            if (i == 0) {
                a = fLayout.morphPlus;
                title = kMorphAddTitle;
                body = kMorphAddDescription;
            } else if (i == 1) {
                a = fLayout.morphPad;
                title = kMorphPadTitle;
                body = kMorphPadDescription;
                gestures = kMorphPadGestures;
            } else {
                const MorphRow& r = fLayout.morphRows[(size_t)(i - 2) / 4];
                const int part = (i - 2) % 4;
                a = part == 1 ? r.up : part == 2 ? r.down : part == 3 ? r.remove : r.rect;
                title = part == 1 ? "Move up" : part == 2 ? "Move down" : part == 3 ? "Remove"
                    : (size_t)r.index < names.size() ? names[(size_t)r.index] : std::string();
                body = part == 1 ? kMorphUpDescription : part == 2 ? kMorphDownDescription : part == 3 ? kMorphRemoveDescription
                    : r.index == 0 ? kMorphMainDescription : kMorphShareDescription;
                if (part == 0)
                    gestures = "";
            }
            const float vw = getWidth() / uiScale(), vh = viewHeight();
            const float w = std::min(kTooltipW, vw - 8);
            const Rect b = shown(a, true, fScroll);
            const Rect box = tooltipRect(b, b.x + std::min(b.w, 260.0f), w, tooltipHeight(*this, w, title.c_str(), body, gestures), vw, vh);
            drawTooltip(*this, box, title.c_str(), body, gestures);
            return;
        }
        if ((size_t)fTipShown >= fLayout.rows.size())
            return;
        const Row& r = fLayout.rows[(size_t)fTipShown];
        const std::string title = tooltipTitle(r.param, entry());
        const char* body = displayDescription(r.param, entry());
        const char* gestures = tooltipGestures(r.kind, r.param < kParamCount);
        const float vw = getWidth() / uiScale(), vh = viewHeight();
        const float w = std::min(kTooltipW, vw - 8);
        const Rect box = tooltipRect(shown(r.rect, r.scrolls, fScroll), r.label.x + r.label.w, w,
            tooltipHeight(*this, w, title.c_str(), body, gestures), vw, vh);
        drawTooltip(*this, box, title.c_str(), body, gestures);
    }

    // the panic button: flips its hidden input (the audio side silences on the change,
    // PanicTrigger), lit for a moment to show it took the click
    std::chrono::steady_clock::time_point fPanicAt {};
    bool fHotPanic = false, fPanicShown = false;
    bool panicLit() const { return std::chrono::steady_clock::now() - fPanicAt < std::chrono::milliseconds(300); }

    void clickPanic()
    {
        const float v = panicNext(fValues[kDiagPanicIn]);
        editParameter(kDiagPanicIn, true);
        setParameterValue(kDiagPanicIn, v);
        fValues[kDiagPanicIn] = v;
        editParameter(kDiagPanicIn, false);
        fPanicAt = std::chrono::steady_clock::now();
        repaint();
    }

    // the menu's Copy CLI: the midi2wav command line for the settings (EngineValues: the DSP's own
    // mapping) to the clipboard; a note under the menu button says so for a moment
    std::chrono::steady_clock::time_point fCopiedAt {};
    bool fHotMenu = false, fCopiedShown = false;
    // the note for a moment: "Copied" under the menu button, a preset saved or not under its row
    std::string fNoteTitle, fNoteBody;
    Rect fNoteAnchor {};
    int fNoteMs = 1500;
    bool copiedNote() const { return std::chrono::steady_clock::now() - fCopiedAt < std::chrono::milliseconds(fNoteMs); }
    void showNote(const std::string& title, const std::string& body, const Rect& anchor, int ms = 1500)
    {
        fNoteTitle = title;
        fNoteBody = body;
        fNoteAnchor = anchor;
        fNoteMs = ms;
        fCopiedAt = std::chrono::steady_clock::now();
        repaint();
    }

    void copyCommandLine()
    {
        // what the instrument plays: its own preparation and mallet for the stored values
        std::vector<float> v(fValues, fValues + kParamCount);
        v[kPrepMode] = (float)effectivePrep(entry(), stored(kPrepMode));
        v[kMallet] = (float)effectiveMallet(entry(), stored(kMallet));
        const std::string dir = fChoice.entry != nullptr ? fChoice.entry->dir : std::string();
        std::vector<std::pair<std::string, float>> morph;
        const std::vector<std::string> ps = partners();
        const std::vector<float> w = morphWeights(1 + (uint32_t)ps.size(), fValues[kMorphX], fValues[kMorphY]);
        for (size_t i = 0; i < ps.size(); ++i)
            if (const InstrumentEntry* e = fCatalog->find(ps[i]))
                morph.push_back({ e->dir, w[i + 1] });
        const PeFelt& fe = fFelt.felt();
        const bool bent = !fFeltOriginal && std::any_of(std::begin(fe.curve), std::end(fe.curve), [](const PeFeltCurve& c) { return c.n > 0; });
        const std::string program = standaloneExecutable(getBinaryFilename(), std::strcmp(getPluginFormatName(), "JACK/Standalone") == 0,
            std::getenv("PATH"));
        const std::string line = midi2wavLine(v.data(), dir, getSampleRate(), &fEdit.curve(), morph, bent ? fFelt.text() : std::string(),
            fScaleFile, program);
        if (!setClipboard("text/plain", line.c_str(), line.size())) {
            d_stderr2("Dussek: the clipboard took nothing; the command line: %s", line.c_str());
            return;
        }
        showNote("Copied", "The command line (dussek render) for the current settings is on the clipboard.", fLayout.menu);
    }

    // ---- the About tab: the instrument's LICENSE.md, read when the instrument changes (not per frame)
    std::vector<TextLine> fLicense;
    bool fLicenseFound = false;
    bool fHotCopyLink = false;

    void loadLicense()
    {
        std::string md;
        fLicenseFound = fChoice.entry != nullptr && readTextFile(fChoice.entry->dir + "/LICENSE.md", md);
        fLicense = markdownLines(md);
    }

    /// the About tab's Copy link: the project's link to the clipboard (DPF opens no browser)
    void copyLink()
    {
        if (!setClipboard("text/plain", kProjectUrl, std::strlen(kProjectUrl))) {
            d_stderr2("Dussek: the clipboard took nothing; the link: %s", kProjectUrl);
            return;
        }
        showNote("Copied", "The project's link is on the clipboard.", shown(fLayout.aboutCopy, true, fScroll));
    }

    void drawCopiedNote()
    {
        if (!copiedNote())
            return;
        const float vw = getWidth() / uiScale(), vh = viewHeight();
        const float w = std::min(kTooltipW, vw - 8);
        const Rect& b = fNoteAnchor;
        const Rect box = tooltipRect(b, b.x, w, tooltipHeight(*this, w, fNoteTitle.c_str(), fNoteBody.c_str(), ""), vw, vh);
        drawTooltip(*this, box, fNoteTitle.c_str(), fNoteBody.c_str(), "");
    }

    // ---- presets (Presets.hpp): the Mics tab's row (the microphones, for this body) and the Sound tab's (everything)
    std::vector<Preset> fUserPresets = loadUserPresets(presetDir());
    std::chrono::steady_clock::time_point fDeleteArmedAt {};
    uint32_t fDeleteArmedRow = 0;
    bool fDeleteShown = false;

    static bool presetRow(uint32_t row) { return row == kMicPresetRow || row == kPresetRow; }
    /// the Mic position list's preset items: this + the preset's index (the positions are 0 … 5)
    static constexpr int kMicPresetCode = 1000;
    /// the row a preset scope shows in: the microphone presets in the Mic position list (no row of their own)
    static uint32_t shownRow(uint32_t row) { return row == kMicPresetRow ? kMicPosition : row; }
    PresetScope scopeOf(uint32_t row) const { return row == kMicPresetRow ? PresetScope::Mics : PresetScope::All; }
    uint32_t bodyNow() const { return (uint32_t)entryBody(entry()); }
    std::vector<const Preset*> offered(uint32_t row) const { return presetsFor(scopeOf(row), bodyNow(), fUserPresets); }
    bool deleteArmed(uint32_t row) const
    {
        return fDeleteArmedRow == row && std::chrono::steady_clock::now() - fDeleteArmedAt < std::chrono::seconds(3);
    }

    /// the preset the controls still have (a full one: the instrument too), -1: none; the user's
    /// before the shipped (saved under a name, the row shows that name)
    int currentPreset(uint32_t row) const
    {
        const std::vector<const Preset*> o = offered(row);
        std::vector<size_t> order;
        for (int pass = 0; pass < 2; ++pass)
            for (size_t k = 0; k < o.size(); ++k)
                if (o[k]->shipped == (pass == 1))
                    order.push_back(k);
        for (size_t k : order) {
            if (matchingPreset({ o[k] }, fValues) != 0)
                continue;
            if (scopeOf(row) == PresetScope::All && presetInstrument(*o[k]) != fChoice.id)
                continue;
            return (int)k;
        }
        return -1;
    }

    /// the instrument a full preset chooses: its own, else (Default, a user's preset without one) the menu's first
    std::string presetInstrument(const Preset& p) const
    {
        for (const auto& [k, v] : pe::presetStates(p))
            if (k == "instrument" && !v.empty())
                return v;
        const InstrumentEntry* first = fCatalog->first();
        return first != nullptr ? first->id : std::string();
    }

    /// the sound-carrying states as they are now (a full preset's)
    std::vector<std::pair<std::string, std::string>> currentStates() const
    {
        return { { "instrument", fChoice.id }, { "velocity_curve", fEdit.text() }, { "morph", fMorphState },
                 { "felt", fFelt.text() }, { "scale_file", fScaleFile } };
    }

    /// a menu item picked: a preset applied, the name asked for, the delete armed (true: the menu
    /// stays open for the second click) or done
    bool presetAction(uint32_t row, int action)
    {
        const std::vector<const Preset*> o = offered(row);
        if (action >= 0 && (size_t)action < o.size()) {
            applyPreset(*o[(size_t)action]);
            return false;
        }
        if (action == kPresetSaveAs) {
            const int cur = currentPreset(row);
            fEditParam = (int)row;
            fEditText = cur >= 0 && !o[(size_t)cur]->shipped ? o[(size_t)cur]->name : std::string();
            getWindow().focus(); // keys to us, not the host (embedded windows)
            return false;
        }
        if (action == kPresetDelete) {
            const int cur = currentPreset(row);
            if (cur < 0 || o[(size_t)cur]->shipped)
                return false;
            if (!deleteArmed(row)) {
                fDeleteArmedAt = std::chrono::steady_clock::now();
                fDeleteArmedRow = row;
                if (const Row* r = rowFor(shownRow(row))) {
                    openList(*r);
                    return true;
                }
                return false;
            }
            const std::string name = o[(size_t)cur]->name;
            fDeleteArmedAt = {};
            if (!deleteUserPreset(*o[(size_t)cur]))
                d_stderr2("Dussek: could not delete the preset %s", o[(size_t)cur]->file.c_str());
            fUserPresets = loadUserPresets(presetDir());
            if (const Row* r = rowFor(shownRow(row)))
                showNote("Deleted", "The preset “" + name + "” is gone.", shown(r->control, r->scrolls, fScroll));
        }
        return false;
    }

    void applyPreset(const Preset& p)
    {
        if (p.scope == PresetScope::All) {
            // the instrument first (choosing it clears the old one's felt), then the other states (the
            // default where the preset has none)
            const std::vector<std::pair<std::string, std::string>> states = pe::presetStates(p);
            if (const std::string v = presetInstrument(p); v != fChoice.id && !v.empty()) {
                if (fCatalog->find(v) == nullptr && v.find('/') == std::string::npos) {
                    if (const Row* r = rowFor(kPresetRow))
                        showNote("Missing instrument", "The preset’s instrument “" + v + "” is not installed: its settings apply to this one.",
                                 shown(r->control, r->scrolls, fScroll), 4000);
                } else {
                    chooseInstrument(v);
                }
            }
            for (const auto& [k, v] : states)
                if (k != "instrument") {
                    setState(k.c_str(), v.c_str());
                    stateChanged(k.c_str(), v.c_str());
                }
        }
        for (const auto& [i, v] : presetParams(p)) {
            editParameter(i, true);
            setParam(i, v);
            editParameter(i, false);
        }
        relayout();
        repaint();
    }

    void savePreset(uint32_t row, const std::string& typed)
    {
        const size_t a = typed.find_first_not_of(' '), b = typed.find_last_not_of(' ');
        if (a == std::string::npos)
            return; // no name: nothing saved
        const std::string name = typed.substr(a, b - a + 1);
        const Preset p = presetFrom(name, scopeOf(row), bodyNow(), fValues, currentStates());
        std::string err;
        const Row* r = rowFor(shownRow(row));
        const Rect anchor = r != nullptr ? shown(r->control, r->scrolls, fScroll) : fLayout.menu;
        if (!saveUserPreset(presetDir(), p, err)) {
            showNote("Not saved", err, anchor, 4000);
            return;
        }
        fUserPresets = loadUserPresets(presetDir());
        std::string kind = bodyName(bodyNow());
        std::replace(kind.begin(), kind.end(), '_', ' ');
        showNote("Saved", "“" + name + "” is among the presets" + (row == kMicPresetRow ? " of every " + kind + "." : std::string(".")), anchor);
    }

    // the menu's Reset: the first click arms it for 3 s (the menu stays open, "click again to
    // confirm"), the second resets every parameter but the instrument (zoom and the tab are UI
    // state, kept)
    std::chrono::steady_clock::time_point fResetArmed {};
    bool resetArmed() const { return std::chrono::steady_clock::now() - fResetArmed < std::chrono::seconds(3); }

    void resetParameters()
    {
        fResetArmed = {};
        // the velocity curve is a state, not a parameter: straight again too
        resetCurve();
        for (uint32_t i : resetParams()) {
            const float def = paramInfo(i).def;
            if (fValues[i] == def)
                continue;
            editParameter(i, true);
            setParam(i, def);
            editParameter(i, false);
        }
    }

    void uiIdle() override
    {
        // the worker rescanned the instrument folders (a new one appeared, as the editor's preview)
        if (const CatalogPtr c = sharedCatalog(resources()); c != fCatalog) {
            fCatalog = c;
            fChoice = choose(*fCatalog, fChoice.id);
            loadLicense();
            relayout();
            repaint();
        }
        fDiag.idle(); // telemetry poll and repaint while the analysis panel is open
        if (PeFeltKeys k {}; fFeltFetch.poll(k, &fFeltLoaded)) { // the shipped felt, read off this thread
            fFelt.setShipped(k); // not loaded: none (the old instrument's would be drawn)
            relayout();
            repaint();
        }
        pollNotes();
        pollMirror();
        // the armed Reset falls back after its 3 s
        if (fResetShown != resetArmed()) {
            fResetShown = resetArmed();
            if (fMenuList)
                fListItems = menuItems(fState.zoom, fResetShown);
            repaint();
        }
        // a preset's armed delete falls back after its 3 s (its open menu with it)
        if (fDeleteShown != deleteArmed(fDeleteArmedRow)) {
            fDeleteShown = deleteArmed(fDeleteArmedRow);
            if (fListParam == (int)shownRow(fDeleteArmedRow))
                if (const Row* r = rowFor(shownRow(fDeleteArmedRow)))
                    openList(*r);
            repaint();
        }
        if (fPanicShown != panicLit()) {
            fPanicShown = panicLit();
            repaint();
        }
        if (fCopiedShown != copiedNote()) {
            fCopiedShown = copiedNote();
            repaint();
        }
        // DPF does not pass on the pointer leaving the window: a tooltip can still come up 0.6 s
        // after the pointer left from a row (it goes at the next motion or press in the window)
        const int tip = fPointerInControls ? fTip.visible(nowMs(), tipBusy()) : -1;
        if (tip != fTipShown) {
            fTipShown = tip;
            repaint();
        }
    }
    bool fResetShown = false;

    void clickRow(const Row& row, Point2 p, uint32_t time)
    {
        const Row r = row; // setParam may lay out anew
        const bool inValue = r.kind == Kind::Slider && r.value.contains(p.x, p.y);
        const bool doubleClick = (int)r.param == fLastClickParam && time - fLastClickTime < 400;
        fLastClickParam = inValue ? -1 : (int)r.param; // the value text: no double-click reset
        fLastClickTime = time;
        if (doubleClick && !inValue && r.kind == Kind::Slider) { // reset to the default
            editParameter(r.param, true);
            setParam(r.param, paramInfo(r.param).def);
            editParameter(r.param, false);
            return;
        }
        switch (r.kind) {
        case Kind::Toggle:
            editParameter(r.param, true);
            setParam(r.param, fValues[r.param] >= 0.5f ? 0.0f : 1.0f);
            editParameter(r.param, false);
            break;
        case Kind::Dropdown:
            openList(r);
            break;
        case Kind::Slider:
            if (inValue) {
                fEditParam = (int)r.param;
                fEditText.clear();
                getWindow().focus(); // keys to us, not the host (embedded windows)
            } else {
                // the track jumps to the pointer; the label or Shift move from the value
                fDragParam = (int)r.param;
                fDragFine = fMods & kModifierShift;
                fDragRelative = fDragFine || !r.control.contains(p.x, p.y); // the label (or Shift): from the value
                fDragX0 = p.x;
                fDragNorm0 = toNormalized(r.param, fValues[r.param]);
                editParameter(r.param, true);
                if (!fDragRelative)
                    setParam(r.param, fromNormalized(r.param, (p.x - r.control.x) / r.control.w));
            }
            break;
        }
        repaint();
    }

    void commitEdit()
    {
        float v;
        const uint32_t i = (uint32_t)fEditParam;
        fEditParam = -1;
        if (presetRow(i)) {
            savePreset(i, fEditText);
        } else if (parseValue(i, fEditText, v)) {
            editParameter(i, true);
            setParam(i, v);
            editParameter(i, false);
        }
        repaint();
    }

    bool clickMicViews(Point2 p)
    {
        for (View v : { kTop, kFront }) {
            const Rect& r = v == kTop ? fLayout.topView : fLayout.frontView;
            if (!r.contains(p.x, p.y))
                continue;
            const ViewTransform& t = v == kTop ? fTop : fFront;
            if (micPosition() == 5) { // an arrow's tip turns the microphone (above: the yaw, front: the pitch)
                const int i = hitAim(fGeom, t, v, kAimLen, p, 8);
                if (i >= 0) {
                    for (uint32_t q = kMicPosition; q <= kMicRPitch; ++q) editParameter(q, true);
                    if (fAim.begin(fGeom, v, i)) {
                        fMicView = v;
                        return true;
                    }
                    for (uint32_t q = kMicPosition; q <= kMicRPitch; ++q) editParameter(q, false);
                }
            }
            const int mic = hitMic(fGeom, t, v, p, 10);
            double a, b;
            t.toWorld(p, a, b);
            if (mic >= 0 && fGeom.has_pair && ((fMods & kModifierAlt) || micPosition() == 5)) {
                for (uint32_t p = kMicPosition; p <= kMicRPitch; ++p) editParameter(p, true);
                if (micPosition() != 5) {
                    seedCustomFromGeometry(); // geometry now describes the free pair
                    t.toWorld(p, a, b); // the switch to Custom may have re-fitted the views (fewer rows beside them)
                }
                loadGeometry();
                if (fFree.begin(fGeom, v, mic, a, b)) {
                    fMicView = v;
                    return true;
                }
                for (uint32_t p = kMicPosition; p <= kMicRPitch; ++p) editParameter(p, false);
            }
            if (mic >= 0 && fMic.begin(fGeom, v, mic, a, b)) {
                fMicView = v;
                editParameter(kMicDistance, true);
                editParameter(kMicSpacing, true);
                return true;
            }
            if (v == kTop)
                if (const int m = hitMarker(fGeom, t, p, 9, micPosition())) {
                    // switching position: its default distance (the old one may be meaningless there)
                    editParameter(kMicPosition, true); setParam(kMicPosition, (float)m); editParameter(kMicPosition, false);
                    editParameter(kMicDistance, true); setParam(kMicDistance, 0.0f); editParameter(kMicDistance, false);
                    return true;
                }
            return true;
        }
        return false;
    }

    void openList(const Row& r)
    {
        fListParam = (int)r.param;
        closeMenu();
        fListItems.clear();
        fListValues.clear();
        fSubGroup = fSubHot = fListHot = -1; // (a stale hot group: hovering it again opened nothing)
        const Rect a = shown(r.control, r.scrolls, fScroll);
        if (r.param == kMicPosition) { // the positions, then the presets for this body, Save as…, the delete
            const int cur = currentPreset(kMicPresetRow);
            for (uint32_t i = 0, n = paramLabelCount(kMicPosition); i < n; ++i) {
                fListItems.push_back(paramLabel(kMicPosition, i));
                fListValues.push_back((float)i);
            }
            const PresetMenu m = presetMenu(offered(kMicPresetRow), cur, deleteArmed(kMicPresetRow));
            for (size_t k = 0; k < m.items.size(); ++k) {
                fListItems.push_back(m.items[k]);
                // a preset: kMicPresetCode + its index; Save as…, the delete: their (negative) codes
                fListValues.push_back(m.actions[k] >= 0 ? (float)(kMicPresetCode + m.actions[k]) : (float)m.actions[k]);
            }
            fListSelected = cur >= 0 ? (int)paramLabelCount(kMicPosition) + cur : (int)micPosition();
            placeList(a);
            repaint();
            return;
        }
        if (presetRow(r.param)) { // the presets offered, Save as…, the user's preset's delete
            const int cur = currentPreset(r.param);
            const PresetMenu m = presetMenu(offered(r.param), cur, deleteArmed(r.param));
            fListItems = m.items;
            for (int act : m.actions)
                fListValues.push_back((float)act);
            fListSelected = cur;
            placeList(a);
            repaint();
            return;
        }
        if (r.param == kInstrumentRow) { // grouped: the groups first, each opening its instruments
            const std::vector<std::string> groups = fCatalog->groups();
            fListSelected = -1;
            for (size_t i = 0; i < groups.size(); ++i) {
                fListItems.push_back(groups[i] + "  ▸");
                if (fChoice.entry != nullptr && groups[i] == fChoice.entry->group) fListSelected = (int)i;
            }
            fListItems.push_back(kOpenFolder); // an instrument kept anywhere (`chooseInstrumentFolder`)
            placeList(a);
            repaint();
            return;
        }
        if (r.param == kMallet || r.param == kPrepMode) { // the instrument's own mallets, its own preparations
            const InstrumentEntry& inst = entry();
            std::vector<uint32_t> values;
            if (r.param == kMallet) {
                for (uint32_t i = 0, n = entryMallets(inst); i < n; ++i) values.push_back(i);
            } else {
                uint32_t v[5];
                values.push_back(0); // None
                for (uint32_t i = 0, n = entryPreparations(inst, v); i < n; ++i) values.push_back(v[i]);
            }
            const uint32_t cur = r.param == kMallet ? effectiveMallet(inst, stored(kMallet)) : effectivePrep(inst, stored(kPrepMode));
            fListSelected = -1;
            for (size_t i = 0; i < values.size(); ++i) {
                fListItems.push_back(r.param == kMallet ? malletLabel(inst, values[i]) : std::string(paramLabel(r.param, values[i])));
                fListValues.push_back((float)values[i]);
                if (values[i] == cur) fListSelected = (int)i;
            }
            placeList(a);
            repaint();
            return;
        }
        for (uint32_t i = 0, n = paramLabelCount(r.param); i < n; ++i)
            fListItems.push_back(paramLabel(r.param, i));
        fListSelected = (int)std::lround(fValues[r.param] - paramInfo(r.param).min);
        placeList(a);
        repaint();
    }

    /// the open list's rect at `anchor`, scrolled to its current entry
    void placeList(const Rect& anchor)
    {
        fListRect = dropdownListRect(anchor, (uint32_t)fListItems.size(), 0, viewHeight(), itemsWidth(), viewRight());
        fListScroll = listScrollTo(fListRect, (uint32_t)fListItems.size(), fListSelected);
        fBarDrag = 0;
    }
    /// the row of the open list (of the submenu beside it) under window-logical y (-1: none)
    int listRow(float y) const { return listRowAt(fListRect, (uint32_t)fListItems.size(), fListScroll, y); }
    int subRow(float y) const { return listRowAt(fSubRect, (uint32_t)fSubItems.size(), fSubScroll, y); }
    /// a submenu is shown beside the list (an instrument group's, the zoom's steps)
    bool subOpen() const { return (fListParam == (int)kInstrumentRow && fSubGroup >= 0) || (fMenuList && fZoomSub); }

    /// a press on a list's bar: its thumb follows the pointer (pressed beside the thumb: centered there first)
    bool pressListBar(float wx, float wy)
    {
        for (int which : { 2, 1 }) {
            if (which == 2 && !subOpen())
                continue;
            const Rect& r = which == 2 ? fSubRect : fListRect;
            const uint32_t n = (uint32_t)(which == 2 ? fSubItems : fListItems).size();
            float& scroll = which == 2 ? fSubScroll : fListScroll;
            const ListBar b = listBar(r, n, scroll);
            if (b.track.w <= 0 || !b.track.contains(wx, wy))
                continue;
            fBarDrag = which;
            fBarGrab = b.thumb.contains(wx, wy) ? wy - b.thumb.y : b.thumb.h / 2;
            scroll = listScrollAtThumb(r, n, wy - fBarGrab);
            updateHover(wx, wy);
            repaint();
            return true;
        }
        return false;
    }

    /// the open list's longest item (estimated) and the view's right edge, for its rect
    float itemsWidth() const
    {
        float w = 0;
        for (const std::string& i : fListItems)
            w = std::max(w, textWidth(i.c_str()));
        return w;
    }
    float viewRight() const { return getWidth() / uiScale(); }

    /// the header menu (its button's list, right-aligned under it)
    void openMenu()
    {
        fMenuList = true;
        fZoomSub = false;
        fListParam = -1;
        fListItems = menuItems(fState.zoom, resetArmed());
        fListValues.clear();
        fListSelected = fListHot = -1;
        const Rect a = fLayout.menu;
        placeList(a);
        fListRect.x = std::max(0.0f, a.x + a.w - fListRect.w);
        repaint();
    }

    /// the zoom's steps beside the menu's Zoom
    void openZoomSteps()
    {
        fZoomSub = true;
        fSubHot = -1;
        fSubItems.clear();
        for (int i = 0; i < 8; ++i)
            fSubItems.push_back(std::to_string(kZoomSteps[i]) + " %");
        fSubRect = submenuRect(fListRect, kMenuZoom, 8, 0, viewHeight(), viewRight());
        int current = -1;
        for (int i = 0; i < 8; ++i)
            if (kZoomSteps[i] == fState.zoom) current = i;
        fSubScroll = listScrollTo(fSubRect, 8, current);
    }

    void closeMenu()
    {
        fMenuList = false;
        fZoomSub = false;
        fLearnMenu = -1;
        fLearnPair = false;
        fMorphMenu = false;
    }

    /// shows group g's instruments beside the instrument menu's list
    void openGroup(int g)
    {
        const std::vector<std::string> groups = fCatalog->groups();
        if (g < 0 || g >= (int)groups.size()) { // no group (the menu's last: open a folder)
            if (fSubGroup >= 0) { fSubGroup = -1; repaint(); }
            return;
        }
        fSubGroup = g;
        fSubHot = -1;
        fSubIds.clear();
        fSubItems.clear();
        for (const InstrumentEntry* e : fCatalog->menu())
            if (e->group == groups[(size_t)g]) {
                fSubIds.push_back(e->id);
                fSubItems.push_back(e->name);
            }
        fSubRect = submenuRect(fListRect, g, (uint32_t)fSubItems.size(), 0, viewHeight(), (float)getWidth() / uiScale());
        int current = -1;
        for (size_t i = 0; i < fSubIds.size(); ++i)
            if (fSubIds[i] == fChoice.id) current = (int)i;
        fSubScroll = listScrollTo(fSubRect, (uint32_t)fSubItems.size(), current);
        repaint();
    }

    void clickList(Point2 p)
    {
        if (fMorphMenu) { // the plus button's instruments
            fMorphMenu = false;
            if (fListRect.contains(p.x, p.y)) {
                const int i = listRow(p.y);
                if (i >= 0 && (size_t)i < fMorphMenuIds.size())
                    setPartners([&](std::vector<std::string>& ids) { ids.push_back(fMorphMenuIds[(size_t)i]); });
            }
            updateHover(fPointerX, fPointerY);
            repaint();
            return;
        }
        if (fLearnMenu >= 0) { // a control's MIDI learn menu (the pad's: Morph X's items, then Morph Y's)
            uint32_t target = (uint32_t)fLearnMenu;
            const bool pair = fLearnPair;
            fLearnMenu = -1;
            fLearnPair = false;
            if (fListRect.contains(p.x, p.y)) {
                int i = listRow(p.y);
                const int xItems = (int)learnMenuItems(fMap, kMorphX).size();
                if (pair && i >= xItems) {
                    target = kMorphY;
                    i -= xItems;
                }
                if (i == 0 || i == 1)
                    startLearn(target, i == 1); // the second: a relative knob's, a switch's flipping pad
                else if (i == 2)
                    commitMap([target](MidiMap& m) { m.forget(target); });
            }
            updateHover(fPointerX, fPointerY);
            repaint();
            return;
        }
        if (fMenuList) {
            if (fZoomSub && fSubRect.contains(p.x, p.y)) {
                fState.zoom = kZoomSteps[std::min(7, std::max(0, subRow(p.y)))];
                saveState();
                applyZoom();
            } else if (fListRect.contains(p.x, p.y)) {
                const int i = listRow(p.y);
                if (i == kMenuZoom) { // its steps (and the menu stays open)
                    openZoomSteps();
                    repaint();
                    return;
                }
                if (i == kMenuReset && !resetArmed()) { // armed: the menu stays open for the second click
                    fResetArmed = std::chrono::steady_clock::now();
                    fListItems = menuItems(fState.zoom, true);
                    repaint();
                    return;
                }
                if (i == kMenuReset)
                    resetParameters();
                else if (i == kMenuCopy)
                    copyCommandLine();
            }
            closeMenu();
            updateHover(fPointerX, fPointerY); // the list is gone: what is under the pointer now
            repaint();
            return;
        }
        if (fListParam == (int)kInstrumentRow) {
            if (fSubGroup >= 0 && fSubRect.contains(p.x, p.y)) {
                const int i = subRow(p.y);
                if (i >= 0 && (size_t)i < fSubIds.size()) {
                    const std::string id = fSubIds[(size_t)i]; // the lists close below
                    fListParam = -1;
                    fSubGroup = -1;
                    chooseInstrument(id);
                    updateHover(fPointerX, fPointerY);
                    return;
                }
            } else if (fListRect.contains(p.x, p.y)) { // a group: open it (and keep the menu open)
                const int row = listRow(p.y);
                if (row == (int)fCatalog->groups().size()) { // the last: an instrument kept anywhere
                    fListParam = -1;
                    fSubGroup = -1;
                    chooseInstrumentFolder();
                    repaint();
                    return;
                }
                openGroup(row);
                repaint();
                return;
            }
            fListParam = -1;
            fSubGroup = -1;
            updateHover(fPointerX, fPointerY); // the list is gone: what is under the pointer now
            repaint();
            return;
        }
        if (const int i = listRow(p.y); fListRect.contains(p.x, p.y) && i >= 0) {
            const float picked = fListValues.empty() ? 0.0f : fListValues[(size_t)std::min(std::max(i, 0), (int)fListValues.size() - 1)];
            if (fListParam == (int)kMicPosition && (picked < 0 || picked >= kMicPresetCode)) { // a preset's item
                fListParam = -1;
                if (presetAction(kMicPresetRow, picked < 0 ? (int)picked : (int)picked - kMicPresetCode))
                    return; // the delete armed: its menu stays open
            } else if (fListParam >= 0 && presetRow((uint32_t)fListParam)) {
                const uint32_t row = (uint32_t)fListParam;
                fListParam = -1;
                if (presetAction(row, (int)fListValues[(size_t)std::min(std::max(i, 0), (int)fListValues.size() - 1)]))
                    return; // the delete armed: its menu stays open
            } else if (fListParam >= 0) {
                const uint32_t param = (uint32_t)fListParam;
                const float v = !fListValues.empty() ? fListValues[(size_t)std::min(std::max(i, 0), (int)fListValues.size() - 1)]
                                                     : paramInfo(param).min + (float)i;
                if (param == kTemperament && v == (float)pe_temperament_count()) {
                    chooseScaleFile(); // the menu's "Scala file": the file first, the temperament once it is chosen
                } else {
                    editParameter(param, true);
                    setParam(param, v);
                    editParameter(param, false);
                }
                if (param == kMicPosition) { editParameter(kMicDistance, true); setParam(kMicDistance, 0.0f); editParameter(kMicDistance, false); }
            }
        }
        fListParam = -1;
        updateHover(fPointerX, fPointerY); // the list is gone: what is under the pointer now
        repaint();
    }

    // ---- the Touch tab: the velocity curve and the recent notes (layout coordinates)
    /// where a press reaches the curve's points: its plotting area and a point's hit radius around it
    Rect curveArea() const
    {
        const float r = CurveEdit::kHitRadius;
        return { fLayout.curve.x - r, fLayout.curve.y - r, fLayout.curve.w + 2 * r, fLayout.curve.h + 2 * r };
    }

    /// the curve to the engine (through the host) and to the screen
    void commitCurve()
    {
        setState("velocity_curve", fEdit.text().c_str());
        repaint();
    }

    void resetCurve()
    {
        fEdit.set(CurveEdit().curve());
        fCurveDrag = -1;
        commitCurve();
    }

    /// a press at p: Reset curve, a point (drag; a double-click removes it), or a new point there
    bool pressCurve(Point2 p, uint32_t time)
    {
        if (fLayout.resetCurve.contains(p.x, p.y)) { resetCurve(); return true; }
        if (fLayout.history.contains(p.x, p.y)) { // a double-click clears the bars (and the dots)
            if (fBarsClickTime != 0 && time - fBarsClickTime < 400) {
                fNotes.clear();
                fBarsClickTime = 0;
                repaint();
            } else {
                fBarsClickTime = time;
            }
            return true;
        }
        if (!curveArea().contains(p.x, p.y))
            return false;
        const Rect& g = fLayout.curve;
        const int i = fEdit.pointAt(g, p.x, p.y);
        const bool doubleClick = i >= 0 && i == fCurveClickPoint && time - fCurveClickTime < 400;
        fCurveClickPoint = i;
        fCurveClickTime = time;
        if (doubleClick) {
            fCurveClickPoint = -1;
            if (fEdit.remove(i))
                commitCurve();
            return true;
        }
        if (i >= 0) {
            fCurveDrag = i;
            return true;
        }
        if (const int k = fEdit.add(g, p.x, p.y); k >= 0) { // dragged from where the rules put it
            fCurveDrag = k;
            fCurveClickPoint = k;
            commitCurve();
        }
        return true;
    }

    /// the notes the engine saw since the last poll; repaints the Touch tab while its dots fade,
    /// and once a second while its bars hold notes (they age out over 30 s)
    void pollNotes()
    {
        uint8_t v[64];
        const double t = nowMs() / 1000.0;
        bool any = false;
        for (size_t n; (n = fNoteClient.pollNotes(v, sizeof v)) > 0; any = true)
            for (size_t i = 0; i < n; ++i)
                fNotes.add(v[i], t);
        if (tab() != kTabTouch)
            return;
        const uint64_t now = nowMs();
        const auto bars = fNotes.bars(t);
        const bool aging = now - fNotesPaintMs >= 1000 && std::any_of(bars.begin(), bars.end(), [](float b) { return b > 0; });
        if (any || fNotes.fading(t) || aging) {
            fNotesPaintMs = now;
            repaint();
        }
    }

    void drawCurve()
    {
        const Rect& g = fLayout.curve;
        const PeVelocityCurve& c = fEdit.curve();
        float x, y;
        ControlLook rl { "Reset curve", std::string(), 0.0f, true, fHotResetCurve, false, nullptr };
        drawToggle(*this, fLayout.resetCurve, rl);
        beginPath(); rect(g.x, g.y, g.w, g.h); fillColor(theme::panel); fill();
        // the grid and its velocities
        fontSize(theme::smallFont);
        for (float v : { 1.0f, 32.0f, 64.0f, 96.0f, 127.0f }) {
            CurveEdit::toGraph(g, v, v, x, y);
            beginPath(); moveTo(x, g.y); lineTo(x, g.y + g.h); moveTo(g.x, y); lineTo(g.x + g.w, y);
            strokeColor(theme::line); strokeWidth(1); stroke();
            char label[8];
            std::snprintf(label, sizeof label, "%d", (int)v);
            fillColor(theme::faint);
            textAlign(ALIGN_CENTER | ALIGN_TOP);
            text(x, g.y + g.h + 3, label, nullptr);
            textAlign(ALIGN_RIGHT | ALIGN_MIDDLE);
            text(g.x - 4, y, label, nullptr);
        }
        fillColor(theme::dim);
        textAlign(ALIGN_LEFT | ALIGN_TOP);
        text(g.x + 6, g.y + 4, "velocity played", nullptr);
        textAlign(ALIGN_RIGHT | ALIGN_BOTTOM);
        text(g.x + g.w - 6, g.y + g.h - 4, "velocity in", nullptr);
        // the straight line, dashed
        beginPath();
        for (float f = 0; f < 1; f += 0.04f) {
            moveTo(g.x + f * g.w, g.y + g.h - f * g.h);
            lineTo(g.x + (f + 0.02f) * g.w, g.y + g.h - (f + 0.02f) * g.h);
        }
        strokeColor(theme::marker); strokeWidth(1); stroke();
        // the curve, a point per logical unit, as the engine evaluates it
        beginPath();
        for (float px = 0; px <= g.w; px += 1) {
            const float vin = 1 + 126 * px / g.w;
            CurveEdit::toGraph(g, vin, pe_velocity_curve_eval(&c, vin), x, y);
            if (px == 0) moveTo(x, y); else lineTo(x, y);
        }
        strokeColor(theme::accentText); strokeWidth(2); stroke();
        // the recent notes on it
        const double t = nowMs() / 1000.0;
        for (const auto& d : fNotes.dots(t)) {
            CurveEdit::toGraph(g, d.first, pe_velocity_curve_eval(&c, d.first), x, y);
            Color dot = theme::accent;
            dot.alpha = d.second;
            beginPath(); circle(x, y, 3.5f); fillColor(dot); fill();
        }
        // the points
        for (uint32_t i = 0; i < c.n; ++i) {
            CurveEdit::toGraph(g, c.vin[i], c.vout[i], x, y);
            beginPath(); circle(x, y, (int)i == fCurveHot || (int)i == fCurveDrag ? 5.5f : 4.0f);
            fillColor(theme::text); fill();
        }
        // the bars: where the notes of the last 30 s fell
        const Rect& h = fLayout.history;
        beginPath(); rect(h.x, h.y, h.w, h.h); fillColor(theme::panel); fill();
        const auto bars = fNotes.bars(t);
        const float bw = h.w / NoteHistory::kBins;
        Color bar = theme::accent;
        bar.alpha = 0.7f;
        for (int i = 0; i < NoteHistory::kBins; ++i)
            if (bars[(size_t)i] > 0) {
                beginPath(); rect(h.x + i * bw + 0.5f, h.y + h.h * (1 - bars[(size_t)i]), bw - 1, h.h * bars[(size_t)i]);
                fillColor(bar); fill();
            }
        fillColor(theme::faint);
        textAlign(ALIGN_LEFT | ALIGN_TOP);
        text(h.x, h.y + h.h + 3, "recent notes, last 30 s (double-click: clear)", nullptr);
    }

    // ---- the Felt tab: the player's felt as bends over the keyboard (FeltEditor), the shipped felt under it
    /// the shipped felt of the instrument and morph point playing, asked for while the tab shows (a
    /// morph's point dragged asks again; the newest request wins)
    void askShippedFelt()
    {
        if (tab() != kTabFelt || fChoice.entry == nullptr)
            return;
        std::vector<std::string> dirs { fChoice.entry->dir };
        const std::vector<std::string> ps = partners();
        const std::vector<float> w = morphWeights(1 + (uint32_t)ps.size(), fValues[kMorphX], fValues[kMorphY]);
        std::vector<float> weights { w[0] };
        for (size_t i = 0; i < ps.size(); ++i)
            if (const InstrumentEntry* e = fCatalog->find(ps[i])) {
                dirs.push_back(e->dir);
                weights.push_back(w[i + 1]);
            }
        std::string key;
        for (size_t i = 0; i < dirs.size(); ++i)
            key += dirs[i] + "=" + std::to_string(weights[i]) + ",";
        if (key == fFeltFor)
            return;
        fFeltFor = key;
        fFeltFetch.request(std::move(dirs), std::move(weights));
    }

    /// the felt to the engine (through the host: measured, a few seconds) and to the screen
    void commitFelt()
    {
        setState("felt", fFelt.text().c_str());
        repaint();
    }

    /// a press at p: a chip, Original, Reset, a point (drag; a double-click removes it), or a new point there
    bool pressFelt(Point2 p, uint32_t time)
    {
        for (uint32_t i = 0; i < 4; ++i)
            if (fLayout.feltChips[i].contains(p.x, p.y)) {
                fFelt.select(i);
                fFeltClickPoint = -1;
                repaint();
                return true;
            }
        if (fLayout.feltOriginal.contains(p.x, p.y)) {
            fFeltOriginal = !fFeltOriginal; // shown at once; kDiagFeltOriginalOut confirms it
            setDiag(kDiagFeltOriginalIn, panicNext(fValues[kDiagFeltOriginalIn])); // a flip, as Panic's
            repaint();
            return true;
        }
        if (fLayout.feltReset.contains(p.x, p.y)) {
            fFelt.reset();
            fFeltDrag = -1;
            commitFelt();
            return true;
        }
        const Rect& g = fLayout.feltGraph;
        const float r = FeltEditor::kHitRadius;
        if (g.w <= 0 || !Rect { g.x - r, g.y - r, g.w + 2 * r, g.h + 2 * r }.contains(p.x, p.y))
            return false;
        const int i = fFelt.pointAt(g, p.x, p.y);
        const bool doubleClick = i >= 0 && i == fFeltClickPoint && time - fFeltClickTime < 400;
        fFeltClickPoint = i;
        fFeltClickTime = time;
        if (doubleClick) {
            fFeltClickPoint = -1;
            if (fFelt.remove(i)) {
                fFelt.dropped();
                commitFelt();
            }
            return true;
        }
        if (i >= 0) {
            fFeltDrag = i;
            return true;
        }
        if (const int k = fFelt.add(g, p.x, p.y); k >= 0) { // dragged from where it landed; sent on the release
            fFeltDrag = k;
            fFeltClickPoint = k;
            repaint();
        }
        return true;
    }

    /// quantity q's value as a label
    static std::string feltLabel(uint32_t q, float v)
    {
        char b[24];
        switch (q) {
        case 0: std::snprintf(b, sizeof b, std::fabs(v) < 0.5f ? "0 dB" : "%+.0f dB", v); break;
        case 1: std::snprintf(b, sizeof b, "%.1f", v); break;
        case 2: std::snprintf(b, sizeof b, "%.0f N", v); break;
        default: std::snprintf(b, sizeof b, "%.1f mm", v); break;
        }
        return b;
    }

    /// quantity q in g: the keys' grid, the shipped felt dashed (dotted where a key has no core, for
    /// the core's depth), the player's solid; `large`: its labels and the points
    void drawFeltGraph(const Rect& g, uint32_t q, bool large)
    {
        static const char* const names[4] = { "Stiffness", "Exponent", "Core force", "Core depth" };
        static const char* const units[4] = { "dB re as shipped, the force at 0.5 mm", "how much brighter struck harder",
            "N at 0.5 mm into the core", "mm of felt before the core" };
        beginPath(); rect(g.x, g.y, g.w, g.h); fillColor(theme::panel); fill();
        float lo, hi, x, y;
        fFelt.range(q, lo, hi);
        fontSize(theme::smallFont);
        for (int key : { 21, 24, 36, 48, 60, 72, 84, 96, 108 }) {
            fFelt.toGraph(g, q, (float)key, lo, x, y);
            beginPath(); moveTo(x, g.y); lineTo(x, g.y + g.h); strokeColor(theme::line); strokeWidth(1); stroke();
            if (large && key != 24) { // C1 beside A0: one label there
                char label[8];
                if (key == 21) std::snprintf(label, sizeof label, "A0"); else std::snprintf(label, sizeof label, "C%d", key / 12 - 1);
                fillColor(theme::faint);
                textAlign((key == 21 ? ALIGN_LEFT : key == 108 ? ALIGN_RIGHT : ALIGN_CENTER) | ALIGN_TOP); // the ends inside the graph
                text(x, g.y + g.h + 3, label, nullptr);
            }
        }
        for (float v : { lo, 0.5f * (lo + hi), hi }) {
            fFelt.toGraph(g, q, 21, v, x, y);
            beginPath(); moveTo(g.x, y); lineTo(g.x + g.w, y); strokeColor(theme::line); strokeWidth(1); stroke();
            if (large) {
                fillColor(theme::faint); textAlign(ALIGN_RIGHT | ALIGN_MIDDLE);
                text(g.x - 4, y, feltLabel(q, v).c_str(), nullptr);
            }
        }
        fillColor(theme::dim);
        textAlign(ALIGN_LEFT | ALIGN_TOP);
        text(g.x + 6, g.y + 4, large ? units[q] : names[q], nullptr);
        // the shipped felt: dashes of two keys, dots of one where a key has no core (its depth then a default)
        save();
        intersectScissor(g.x, g.y, g.w, g.h);
        const PeFeltKeys& k = fFelt.shipped();
        for (int key = 21; key < 108; ++key) {
            const bool dotted = q == 3 && k.core[key - 21] == 0;
            if (dotted ? (key % 2) : (key / 2) % 2)
                continue;
            if (k.hammer[key - 21] == 0 || k.hammer[key - 20] == 0)
                continue;
            float x1, y1;
            fFelt.toGraph(g, q, (float)key, fFelt.shown(q, key, false), x, y);
            fFelt.toGraph(g, q, (float)key + 1, fFelt.shown(q, key + 1, false), x1, y1);
            beginPath(); moveTo(x, y); lineTo(x1, y1); strokeColor(theme::marker); strokeWidth(1.5f); stroke();
        }
        // the player's
        beginPath();
        bool started = false;
        for (int key = 21; key <= 108; ++key) {
            if (k.hammer[key - 21] == 0) { started = false; continue; }
            fFelt.toGraph(g, q, (float)key, fFelt.shown(q, key, true), x, y);
            if (!started) moveTo(x, y); else lineTo(x, y);
            started = true;
        }
        strokeColor(theme::accentText); strokeWidth(large ? 2.0f : 1.5f); stroke();
        if (large && q == fFelt.selected()) {
            const PeFeltCurve& c = fFelt.felt().curve[q];
            for (uint32_t i = 0; i < c.n; ++i) {
                fFelt.toGraph(g, q, c.key[i], fFelt.shown(q, c.key[i], true), x, y);
                beginPath(); circle(x, y, (int)i == fFeltDrag ? 5.5f : 4.0f); fillColor(theme::text); fill();
            }
        }
        restore();
    }

    void drawFeltTab()
    {
        const Rect& n = fLayout.feltNote;
        fontSize(theme::smallFont);
        if (fLayout.feltGraph.w <= 0 || !fFelt.applies()) {
            fillColor(theme::dim);
            textAlign(ALIGN_LEFT | ALIGN_MIDDLE);
            text(n.x, n.y + n.h / 2, fLayout.feltGraph.w <= 0 ? "The felt applies to the pianos' hammers."
                    : fFeltLoaded                     ? "reading the felt…"
                                                      : "The felt could not be read.", nullptr);
            return;
        }
        static const char* const chips[4] = { "Stiffness", "Exponent", "Core force", "Core depth" };
        for (uint32_t i = 0; i < 4; ++i) {
            ControlLook c { chips[i], std::string(), fFelt.selected() == i ? 1.0f : 0.0f, true, false, false, nullptr };
            drawToggle(*this, fLayout.feltChips[i], c);
        }
        ControlLook o { "Original", std::string(), fFeltOriginal ? 1.0f : 0.0f, true, false, false, nullptr };
        drawToggle(*this, fLayout.feltOriginal, o);
        ControlLook r { "Reset", std::string(), 0.0f, true, false, false, nullptr };
        drawToggle(*this, fLayout.feltReset, r);
        const uint32_t q = fFelt.selected();
        drawFeltGraph(fLayout.feltGraph, q, true);
        for (uint32_t i = 0, j = 0; i < 4; ++i)
            if (i != q)
                drawFeltGraph(fLayout.feltSmall[j++], i, false);
        fontSize(theme::smallFont);
        textAlign(ALIGN_LEFT | ALIGN_MIDDLE);
        const bool measuring = fValues[kDiagBuildingOut] >= 0.5f;
        fillColor(measuring ? theme::accentText : theme::faint);
        text(n.x, n.y + n.h / 2, measuring ? "measuring…" : "click: add a point · drag: move it · double-click: remove it", nullptr);
    }

    // ---- MIDI learn: the map (the settings file, MidiMap.hpp), a control's right-click menu, the wait for a CC
    MidiMap fMap = MidiMap::load(midiMapPath());
    int fLearnMenu = -1;   // the control whose MIDI learn menu is open
    int fLearnTarget = -1; // the target waiting for a CC (kDiagLearnIn)
    bool fLearnAlt = false; // … learned the menu's second way (a relative knob, a flipping pad)
    uint32_t fNoteToken = 0;
    bool fMirrorSeen = false; // the value mirror read once since the region opened
    uint32_t fMirrorSeqs[kParamCount] {};
    float fInstrumentOut = 0;

    bool listOpen() const { return fListParam >= 0 || fMenuList || fLearnMenu >= 0 || fMorphMenu; }

    void setDiag(uint32_t index, float v)
    {
        fValues[index] = v;
        editParameter(index, true);
        setParameterValue(index, v);
        editParameter(index, false);
    }

    /// a right-click: ends a wait; on a control (not the Instrument menu) opens its MIDI learn menu
    bool rightClick(float wx, float wy)
    {
        fTip.dismiss();
        if (fLearnTarget >= 0) {
            cancelLearn();
            repaint();
            return true;
        }
        if (fEditParam >= 0) commitEdit();
        if (listOpen()) {
            fListParam = -1;
            fSubGroup = -1;
            closeMenu();
            repaint();
            return true;
        }
        if (wy >= viewHeight())
            return false;
        if (const Point2 p = dragPoint(wx, wy); tab() == kTabMorph && inContent(fLayout, wx, wy) && fLayout.morphPad.contains(p.x, p.y)) {
            // the pad: Morph X's learn items, then Morph Y's
            fLearnMenu = (int)kMorphX;
            fLearnPair = true;
            fListItems.clear();
            for (uint32_t t : { (uint32_t)kMorphX, (uint32_t)kMorphY })
                for (const std::string& s : learnMenuItems(fMap, t))
                    fListItems.push_back(std::string(paramInfo(t).name) + ": " + s);
            fListValues.clear();
            fListSelected = fListHot = -1;
            placeList({ wx, wy, 1, 1 });
            repaint();
            return true;
        }
        const int i = rowAt(fLayout, wx, wy, fScroll);
        if (i < 0 || fLayout.rows[(size_t)i].param >= kParamCount)
            return false;
        const Row& r = fLayout.rows[(size_t)i];
        fLearnMenu = (int)r.param;
        fListItems = learnMenuItems(fMap, r.param);
        fListValues.clear();
        fListSelected = fListHot = -1;
        placeList(shown(r.control, r.scrolls, fScroll));
        repaint();
        return true;
    }

    /// `alt`: the menu's second way, a relative knob's (a switch: a pad flipping it)
    void startLearn(uint32_t target, bool alt = false)
    {
        fLearnTarget = (int)target;
        fLearnAlt = alt;
        setDiag(kDiagLearnIn, (float)(target + 1));
    }

    void cancelLearn()
    {
        fLearnTarget = -1;
        setDiag(kDiagLearnIn, 0.0f);
    }

    /// the DSP caught a CC for the waiting control: the tie, written
    void learned(float v)
    {
        uint8_t ch, cc;
        bool zero;
        if (fLearnTarget < 0 || !unpackLearned(v, ch, cc, zero))
            return;
        const uint32_t target = (uint32_t)fLearnTarget;
        const bool alt = fLearnAlt, toggle = target < kParamCount && paramInfo(target).boolean;
        cancelLearn();
        commitMap([&](MidiMap& m) { m.tie(target, ch, cc, zero, alt && !toggle, alt && toggle); });
    }

    /// a change to the map, made to its file as it is now (another instance's ties stay) and the
    /// DSP told to read it again; a file that cannot be written: no change (the DSP would never
    /// hear of it), said on stderr
    void commitMap(const std::function<void(MidiMap&)>& edit)
    {
        const std::string path = midiMapPath();
        if (fMap.update(path, edit))
            setDiag(kDiagMapSeqIn, std::fmod(fValues[kDiagMapSeqIn] + 1.0f, 16777216.0f));
        else
            d_stderr2("Dussek: the MIDI map could not be written to '%s': the change is not kept", path.c_str());
        relayout(); // the MIDI tab's rows, the tags' room
        repaint();
    }

    // ---- the MIDI tab (layout coordinates)
    int fHotMidi = -1;
    int fHotSwitch = 0; // the MIDI tab's switch under the pointer: 1 program change, 2 soft takeover

    uint32_t midiTarget(const MidiRow& r) const
    {
        const std::vector<Tie>& ties = fMap.ties();
        return r.tie >= 0 && (size_t)r.tie < ties.size() ? ties[(size_t)r.tie].target : r.target;
    }

    // ---- the Morph tab
    std::string fScaleFile; // the "scale_file" state: the Temperament menu's Scala file
    std::string fScaleDir;  // the file browser's start folder (its options keep a pointer to it)
    enum class Browsing { Scale, Instrument } fBrowsing = Browsing::Scale; // what the file browser is open for
    std::string fMorphState; // the "morph" state's text, as set (the shown partners: partners())
    bool fPadDrag = false;              // the pad's point follows the pointer
    bool fMorphMenu = false;            // the plus button's list is open (its ids: fMorphMenuIds)
    bool fLearnPair = false;            // the open learn menu is the pad's (Morph X and Y)
    std::vector<std::string> fMorphMenuIds;

    /// the partners that join the chosen instrument (what the DSP plays), in list order
    std::vector<std::string> partners() const { return shownPartners(fMorphState, *fCatalog, fChoice.id); }

    /// the partners with a share of the morph (0 while the main instrument plays alone)
    uint32_t partnersPlaying() const
    {
        const std::vector<float> w = morphWeights(1 + (uint32_t)partners().size(), fValues[kMorphX], fValues[kMorphY]);
        uint32_t n = 0;
        for (size_t i = 1; i < w.size(); ++i)
            n += w[i] > 0 ? 1 : 0;
        return w[0] < 1.0f ? n : 0;
    }

    /// the partner list changed by `f`, kept valid, to the host and the DSP
    template <typename F> void setPartners(F f)
    {
        std::vector<std::string> ids = partners();
        const size_t before = ids.size();
        f(ids);
        ids = validPartners(ids, *fCatalog, fChoice.id);
        fMorphState = formatPartners(ids);
        setState("morph", fMorphState.c_str());
        const Point2 here { fValues[kMorphX], fValues[kMorphY] }, there = morphPointAfter(before, ids.size(), here);
        if (there.x != here.x || there.y != here.y)
            placeMorphPoint(there);
        relayout();
        repaint();
    }

    void setMorphPoint(Point2 q)
    {
        const Point2 c = morphClamp(1 + (uint32_t)partners().size(), q.x, q.y);
        for (uint32_t i : { (uint32_t)kMorphX, (uint32_t)kMorphY }) {
            fValues[i] = i == kMorphX ? c.x : c.y;
            setParameterValue(i, fValues[i]);
        }
        repaint();
    }

    void movePad(Point2 p) { setMorphPoint(padToPlane(fLayout.morphPad, p.x, p.y)); }

    /// the point set at once (an edit of its own, not a drag)
    void placeMorphPoint(Point2 q)
    {
        for (uint32_t i : { (uint32_t)kMorphX, (uint32_t)kMorphY }) {
            editParameter(i, true);
            fValues[i] = i == kMorphX ? q.x : q.y;
            setParameterValue(i, fValues[i]);
            editParameter(i, false);
        }
        repaint();
    }

    /// a press in the Morph tab (layout coordinates): a row's button, the plus, the pad
    bool pressMorph(Point2 p)
    {
        for (const MorphRow& r : fLayout.morphRows) {
            const size_t k = (size_t)r.index - 1; // the partner's place in partners()
            if (r.remove.contains(p.x, p.y)) {
                setPartners([k](std::vector<std::string>& ids) { if (k < ids.size()) ids.erase(ids.begin() + (long)k); });
                return true;
            }
            if (r.up.contains(p.x, p.y) || r.down.contains(p.x, p.y)) {
                const bool up = r.up.contains(p.x, p.y);
                setPartners([k, up](std::vector<std::string>& ids) {
                    if (up && k > 0 && k < ids.size()) std::swap(ids[k], ids[k - 1]);
                    if (!up && k + 1 < ids.size()) std::swap(ids[k], ids[k + 1]);
                });
                return true;
            }
        }
        if (fLayout.morphPlus.contains(p.x, p.y)) {
            if (!fLayout.morphFull)
                openMorphMenu();
            return true;
        }
        if (fLayout.morphPad.contains(p.x, p.y) && !partners().empty()) {
            fPadDrag = true;
            editParameter(kMorphX, true);
            editParameter(kMorphY, true);
            movePad(p);
            return true;
        }
        return false;
    }

    /// the plus button's list: the instruments that may join, not in the list yet
    void openMorphMenu()
    {
        fMorphMenuIds.clear();
        fListItems.clear();
        const std::vector<std::string> ps = partners();
        for (const InstrumentEntry* e : fCatalog->menu()) {
            std::vector<std::string> with = ps;
            with.push_back(e->id);
            if (std::find(ps.begin(), ps.end(), e->id) == ps.end() && validPartners(with, *fCatalog, fChoice.id).size() > ps.size()) {
                fMorphMenuIds.push_back(e->id);
                fListItems.push_back(e->name);
            }
        }
        if (fListItems.empty())
            return;
        fMorphMenu = true;
        fListValues.clear();
        fListSelected = fListHot = -1;
        placeList(shown(fLayout.morphPlus, true, fScroll));
        repaint();
    }

    /// the list's names: the chosen instrument, then the partners
    std::vector<std::string> morphNames() const
    {
        const std::vector<std::string> ps = partners();
        std::vector<std::string> names;
        for (size_t i = 0; i <= ps.size(); ++i) {
            const InstrumentEntry* e = i == 0 ? fChoice.entry : fCatalog->find(ps[i - 1]);
            names.push_back(e != nullptr ? e->name : i == 0 ? fChoice.id : ps[i - 1]);
        }
        return names;
    }

    void drawMorphTab()
    {
        if (fLayout.morphNote.w > 0) {
            drawMorphNote(*this, fLayout.morphNote);
            return;
        }
        const std::vector<std::string> names = morphNames();
        const uint32_t n = (uint32_t)names.size();
        const std::vector<float> w = morphWeights(n, fValues[kMorphX], fValues[kMorphY]);
        for (const MorphRow& r : fLayout.morphRows)
            if ((size_t)r.index < names.size())
                drawMorphRow(*this, r, names[(size_t)r.index], w[(size_t)r.index], false);
        drawPlainButton(*this, fLayout.morphPlus, "+ Add", !fLayout.morphFull, false);
        drawMorphPad(*this, fLayout.morphPad, names, fValues[kMorphX], fValues[kMorphY], fValues[kDiagBuildingOut] > 0.5f,
            fPadDrag);
    }

    void drawMidiTab()
    {
        const std::vector<Tie>& ties = fMap.ties();
        for (size_t k = 0; k < fLayout.midiRows.size(); ++k) {
            const MidiRow& r = fLayout.midiRows[k];
            const uint32_t target = midiTarget(r);
            const bool tied = r.tie >= 0 && (size_t)r.tie < ties.size();
            const bool waiting = (int)target == fLearnTarget;
            const std::string detail = tied ? tieLabel(ties[(size_t)r.tie]) + " · channel " + tieChannel(ties[(size_t)r.tie])
                                            : std::string("not tied");
            drawMidiRow(*this, r, targetName(target), detail, tied ? "×" : waiting ? "learning…" : "learn", (int)k == fHotMidi,
                waiting);
        }
        for (int i = 0; i < 2; ++i) {
            const Rect& s = i == 0 ? fLayout.programSwitch : fLayout.takeoverSwitch;
            const char* const title = i == 0 ? kProgramChangeTitle : kTakeoverTitle;
            const bool on = i == 0 ? fMap.programChange : fMap.softTakeover;
            Row r { kParamCount, Kind::Toggle, s };
            r.compact = true;
            const float lw = std::min(textWidth(title) + 8, std::max(1.0f, s.w - 50));
            r.label = { s.x, s.y, lw, s.h };
            r.control = { s.x + lw + 8, s.y + 3, 34, 16 };
            r.value = r.control;
            ControlLook c { title, std::string(), on ? 1.0f : 0.0f, true, fHotSwitch == i + 1, false, nullptr };
            drawCompactRow(*this, r, c);
        }
        if (fLayout.rescanMidi.w > 0) { // the standalone on its own audio
            fontSize(theme::fontSize);
            fillColor(theme::dim);
            textAlign(ALIGN_LEFT | ALIGN_MIDDLE);
            const Rect& c0 = fLayout.bufferChips[0];
            text(fLayout.rescanMidi.x + 4, c0.y + c0.h / 2, "Buffer", nullptr);
            const uint now = getBufferSize();
            for (int i = 0; i < kBufferSizeCount; ++i) {
                const std::string n = std::to_string(kBufferSizes[i]);
                ControlLook c { n.c_str(), std::string(), now == kBufferSizes[i] ? 1.0f : 0.0f, supportsBufferSizeChanges(), false, false,
                    nullptr };
                drawToggle(*this, fLayout.bufferChips[i], c);
            }
            const MidiRow r { -1, kParamCount, fLayout.rescanMidi, fLayout.rescanButton };
            drawMidiRow(*this, r, "MIDI inputs", isMIDIEnabled() ? "open (Rescan for ones plugged in since)" : "none open", "Rescan",
                false, false);
        }
    }

    bool clickMidiTab(Point2 p)
    {
        if (fLayout.rescanButton.contains(p.x, p.y)) {
            if (supportsMIDI())
                requestMIDI();
            repaint();
            return true;
        }
        for (int i = 0; i < kBufferSizeCount; ++i)
            if (fLayout.bufferChips[i].contains(p.x, p.y)) {
                if (supportsBufferSizeChanges())
                    requestBufferSizeChange(kBufferSizes[i]);
                repaint();
                return true;
            }
        if (fLayout.programSwitch.contains(p.x, p.y)) {
            const bool on = !fMap.programChange;
            commitMap([on](MidiMap& m) { m.programChange = on; });
            return true;
        }
        if (fLayout.takeoverSwitch.contains(p.x, p.y)) {
            const bool on = !fMap.softTakeover;
            commitMap([on](MidiMap& m) { m.softTakeover = on; });
            return true;
        }
        for (const MidiRow& r : fLayout.midiRows) {
            if (!r.button.contains(p.x, p.y))
                continue;
            const uint32_t target = midiTarget(r);
            if (r.tie >= 0) {
                commitMap([target](MidiMap& m) { m.forget(target); });
            } else if ((int)target != fLearnTarget) {
                startLearn(target);
                repaint();
            }
            return true;
        }
        return false;
    }

    /// an instrument MIDI chose (kDiagInstrumentOut: 1 + its menu position + 1024 × a count, 0 once
    /// another choice replaced it): the UI shows it and passes it on to the host
    void midiInstrument(float v)
    {
        if (v == fInstrumentOut)
            return;
        fInstrumentOut = v;
        const long pos = std::lround(v) % 1024 - 1;
        const std::vector<const InstrumentEntry*> menu = fCatalog->menu();
        if (pos >= 0 && pos < (long)menu.size() && menu[(size_t)pos]->id != fChoice.id)
            chooseInstrument(menu[(size_t)pos]->id);
    }

    /// values MIDI set that the host never heard of (it takes no change request): to the host and the
    /// screen, those set while the UI was closed too
    void pollMirror()
    {
        for (uint32_t i = 0; i < kParamCount; ++i) {
            uint32_t seq;
            float v;
            if (!fNoteClient.readValue(i, seq, v))
                return;
            if (mirrorApplies(!fMirrorSeen, seq, fMirrorSeqs[i], v, fValues[i])) {
                editParameter(i, true);
                setParam(i, v);
                editParameter(i, false);
            }
            fMirrorSeqs[i] = seq;
        }
        fMirrorSeen = true;
    }

    void updateHover(float wx, float wy)
    {
        const Point2 w { wx, wy }; // the lists are window-logical
        if (fListParam == (int)kInstrumentRow) {
            // over a group: it opens; over its instruments: the hovered one; elsewhere the open group stays
            if (fSubGroup >= 0 && fSubRect.contains(w.x, w.y)) {
                const int h = subRow(w.y);
                if (h != fSubHot || fListHot != fSubGroup) { fSubHot = h; fListHot = fSubGroup; repaint(); }
            } else if (fListRect.contains(w.x, w.y)) {
                const int h = listRow(w.y);
                if (h >= 0 && h != fSubGroup) openGroup(h);
                if (h != fListHot || fSubHot != -1) { fListHot = h; fSubHot = -1; repaint(); }
            } else if (fSubHot != -1) {
                fSubHot = -1;
                repaint();
            }
            return;
        }
        if (fMenuList) {
            // over Zoom: its steps open; over another item they close; over the steps: the hovered one
            if (fZoomSub && fSubRect.contains(w.x, w.y)) {
                const int h = subRow(w.y);
                if (h != fSubHot || fListHot != kMenuZoom) { fSubHot = h; fListHot = kMenuZoom; repaint(); }
            } else if (fListRect.contains(w.x, w.y)) {
                const int h = listRow(w.y);
                if (h == kMenuZoom && !fZoomSub) openZoomSteps();
                else if (h != kMenuZoom) fZoomSub = false;
                if (h != fListHot || fSubHot != -1) { fListHot = h; fSubHot = -1; repaint(); }
            } else if (fSubHot != -1) {
                fSubHot = -1;
                repaint();
            }
            return;
        }
        if (fListParam >= 0 || fLearnMenu >= 0 || fMorphMenu) {
            const int h = fListRect.contains(w.x, w.y) ? listRow(w.y) : -1;
            if (h != fListHot) { fListHot = h; repaint(); }
            return;
        }
        const int hotRow = rowAt(fLayout, wx, wy, fScroll);
        const Tab t = tabAt(fLayout, wx, wy);
        const int hotTab = t == kTabCount ? -1 : (int)t;
        const Point2 p = dragPoint(wx, wy); // the drawing may scroll (narrow, Mics)
        int hotMic = -1, hotMarker = 0;
        View hotView = kTop;
        if (const int vi = viewAt(fLayout, wx, wy, fScroll); vi >= 0) { // only where the drawing is shown
            const View v = vi == 0 ? kTop : kFront;
            hotView = v;
            hotMic = hitMic(fGeom, v == kTop ? fTop : fFront, v, p, 10);
            if (v == kTop && hotMic < 0) hotMarker = hitMarker(fGeom, fTop, p, 9, micPosition());
        }
        const bool hotPanic = fLayout.panic.contains(wx, wy), hotMenu = fLayout.menu.contains(wx, wy);
        const bool inC = inContent(fLayout, wx, wy);
        const bool hotReset = inC && fLayout.resetCurve.contains(p.x, p.y);
        const bool overCurve = inC && fLayout.curve.w > 0 && curveArea().contains(p.x, p.y);
        const int hotPoint = overCurve ? fEdit.pointAt(fLayout.curve, p.x, p.y) : -1;
        if (hotReset != fHotResetCurve || hotPoint != fCurveHot) { fHotResetCurve = hotReset; fCurveHot = hotPoint; repaint(); }
        int hotMidi = -1;
        for (size_t k = 0; inC && k < fLayout.midiRows.size(); ++k)
            if (fLayout.midiRows[k].rect.contains(p.x, p.y))
                hotMidi = (int)k;
        const int hotSwitch = !inC ? 0 : fLayout.programSwitch.contains(p.x, p.y) ? 1 : fLayout.takeoverSwitch.contains(p.x, p.y) ? 2 : 0;
        int hotMorph = -1;
        if (inC && tab() == kTabMorph) {
            if (fLayout.morphPlus.contains(p.x, p.y))
                hotMorph = morphTip(0);
            else if (fLayout.morphPad.contains(p.x, p.y))
                hotMorph = morphTip(1);
            for (size_t k = 0; hotMorph < 0 && k < fLayout.morphRows.size(); ++k) {
                const MorphRow& r = fLayout.morphRows[k];
                const int b = morphTip(2 + 4 * (int)k);
                hotMorph = r.up.contains(p.x, p.y) ? b + 1 : r.down.contains(p.x, p.y) ? b + 2
                    : r.remove.contains(p.x, p.y) ? b + 3 : r.rect.contains(p.x, p.y) ? b : -1;
            }
        }
        const bool hotCopy = inC && tab() == kTabAbout && fLayout.aboutCopy.contains(p.x, p.y);
        if (hotCopy != fHotCopyLink) { fHotCopyLink = hotCopy; repaint(); }
        if (hotMidi != fHotMidi || hotSwitch != fHotSwitch) { fHotMidi = hotMidi; fHotSwitch = hotSwitch; repaint(); }
        fTip.hover(hotPanic ? panicTip() : hotMenu ? menuTip() : overCurve ? curveTip() : hotSwitch == 1 ? programTip() : hotSwitch == 2 ? takeoverTip()
                : hotMidi >= 0 ? midiTip((size_t)hotMidi) : hotMorph >= 0 ? hotMorph : hotRow, nowMs());
        if (hotPanic != fHotPanic) { fHotPanic = hotPanic; repaint(); }
        if (hotMenu != fHotMenu) { fHotMenu = hotMenu; repaint(); }
        if (hotRow != fHotRow || hotMic != fHotMic || hotMarker != fHotMarker || hotTab != fHotTab || hotView != fHotView) {
            fHotRow = hotRow; fHotMic = hotMic; fHotMarker = hotMarker; fHotTab = hotTab; fHotView = hotView;
            repaint();
        }
    }

    DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PianoUI)
};

UI* createUI() { return new PianoUI(); }

END_NAMESPACE_DISTRHO
