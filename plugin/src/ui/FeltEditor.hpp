// The Felt tab's curves, as the player edits them (pure; tested in ui_tests; design
// 2026-10-22-felt-controls): per quantity (0 stiffness, 1 exponent, 2 the core's force, 3 its depth)
// up to 8 points of bends on the shipped felt, the felt they play (pe_felt_apply), the selected
// quantity's points under the pointer, added, moved and removed, and each graph's range, refit when a
// point is dropped. The stiffness shows dB re the shipped felt; the others their own units.
#pragma once
#include "Layout.hpp"
#include "pianoengine.h"

#include <string>

namespace pe {

class FeltEditor {
public:
    FeltEditor();
    /// the player's felt (the "felt" state's); the ranges refit
    void set(const PeFelt& f);
    const PeFelt& felt() const { return fFelt; }
    /// the "felt" state's text (pe_felt_format)
    std::string text() const;
    /// each key's shipped felt (pe_felt_shipped); the ranges refit
    void setShipped(const PeFeltKeys& k);
    /// some key is struck by a hammer: the tab has something to edit
    bool applies() const;
    const PeFeltKeys& shipped() const { return fShipped; }
    void select(uint32_t q) { fSelected = q < 4 ? q : 0; }
    uint32_t selected() const { return fSelected; }
    /// quantity `q` at `key` (21–108): shipped (`user` false) or played, in its unit (the stiffness in dB re shipped)
    float shown(uint32_t q, int key, bool user) const;
    /// the graph's range of quantity `q`, as last refit
    void range(uint32_t q, float& lo, float& hi) const;
    /// (key, value) of quantity `q` ↔ the logical point in `g` (A0 at its left, C8 at its right)
    void toGraph(const Rect& g, uint32_t q, float key, float value, float& x, float& y) const;
    void fromGraph(const Rect& g, uint32_t q, float x, float y, float& key, float& value) const;
    /// the selected quantity's point within kHitRadius of (x, y) in `g`, the nearest (-1: none)
    int pointAt(const Rect& g, float x, float y) const;
    /// a point at (x, y), its key whole: its index, or -1 (none added: at kMaxPoints, or on a point's key)
    int add(const Rect& g, float x, float y);
    /// point `index` towards (x, y): its key between its neighbors', its value as dragged (past the
    /// graph's range too, not past the felt's limits: it stops there)
    void move(int index, const Rect& g, float x, float y);
    bool remove(int index);
    /// a point let go (or added, or removed): the ranges refit; true when one changed
    bool dropped();
    /// no points
    void reset();

    static constexpr int kMaxPoints = 8;
    static constexpr float kHitRadius = 8;

private:
    void play();
    /// the selected quantity's point `index` set to the bend that plays at its key
    void settle(uint32_t index);
    bool refit();
    float shippedValue(uint32_t q, int key) const;

    PeFelt fFelt {};
    PeFeltKeys fShipped {};
    PeFeltKeys fPlayed {};
    uint32_t fSelected = 0;
    float fLo[4] {}, fHi[4] {};
};

} // namespace pe
