// the UI's own state, kept in the plugin state key "ui" (not a parameter)
#pragma once
#include <cstdint>
#include <string>
#include <vector>
namespace pe {
// tab: the panel's tab (Layout.hpp Tab); diag: 0 collapsed, 1 strip, 2 strip + analysis, 3 strip +
// visualization; the analysis
// panel's view (0 spectrogram, 1 partials), frequency range (Hz) and dB floor
struct UiState { uint32_t tab = 0; int zoom = 100; int diag = 0; int view = 0; float lo = 27.5f, hi = 16000.0f, floorDb = -90.0f; };
extern const int kZoomSteps[8];
UiState parseUiState(const char* s);
std::string formatUiState(const UiState& s);

/// The scale kept for the next window (the standalone has no session to keep it): `<config>/window.txt`
/// ("zoom=150"), written when the scale is picked. `config` empty: none known (nothing kept).
int parseSavedZoom(const std::string& text);   // a zoom step, else 100
int loadSavedZoom(const std::string& config);   // 100 without the file
bool saveZoom(const std::string& config, int zoom); // the folder made if missing
/// a new window's state: the defaults, at the kept scale
UiState initialUiState(const std::string& config);
/// Minimum size in pixels; zoomed content scrolls rather than raising this limit.
void minimumWindowSize(double hostScale, uint32_t& width, uint32_t& height);
/// Parameters reset by the Reset button.
std::vector<uint32_t> resetParams();
/// Minimum height with the diagnostics shown: their scaled height plus a controls viewport (240 px),
/// but never below the normal minimum window size.
void minimumWindowSize(double hostScale, double scale, float diagLogical, uint32_t& width, uint32_t& height);
/// Tracks asynchronous size requests made by zoom changes. An unmatched host resize rebases the
/// tracker; unanswered requests expire so later zooms can use the current host size.
class WindowResizeTracker {
public:
    static constexpr uint64_t kExpiryMs = 1000;
    /// Scale the whole window while preserving its logical size, honoring the current minimum.
    uint32_t zoomed(uint32_t actualHeight, double oldScale, double newScale, uint32_t minH, uint64_t nowMs = 0);
    void hostResized(uint32_t reportedHeight, uint64_t nowMs = 0);
    bool pending(uint64_t nowMs = 0) const;
private:
    void requested(uint32_t height, uint64_t nowMs);
    static constexpr int kMax = 8;
    uint32_t fReq[kMax] = {};
    int fCount = 0; // outstanding requests, oldest first
    uint64_t fAt = 0; // when the newest was made
};
}
