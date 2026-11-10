// Spectrogram columns and the frequency/dB mappings of the analysis panel (pure; tested).
#pragma once
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace pe {

class Spectrogram {
public:
    /// `fftSize` must be a power of two >= 2 and `hop` >= 1. Anything else asserts in a debug build; in a
    /// release build an invalid size falls back to 4096 and an invalid hop to a quarter of the size.
    explicit Spectrogram(int fftSize = 4096, int hop = 1024);
    /// feed interleaved stereo (mono mix); returns how many columns were completed. The
    /// newest is `column()`: dBFS per bin (a full-scale sine = 0 dB, floor −120). Finite input is not
    /// limited above (a sample of 1e30 reads +600 dB); a non-finite magnitude counts as 1e6,
    /// i.e. +120 dBFS, so the column is always finite (an infinite input sample makes the transform NaN, which
    /// reads as the floor; for such input the column never exceeds +120).
    int push(const float* interleaved, size_t frames);
    const std::vector<float>& column() const { return fColumn; }
    int size() const { return fN; }
    void clear();
private:
    void transform();
    int fN, fHop, fFilled = 0, fSinceHop = 0;
    std::vector<float> fRing, fWindow, fColumn;
    std::vector<double> fRe, fIm;
    double fScale;
};

/// rows from the top (fHi) to the bottom (fLo), log frequency; each 0…1 from the loudest bin in the row:
/// (dB − dbFloor) / −dbFloor, clamped. dbFloor must be < 0 (e.g. −90); 0, a positive or a NaN floor is
/// taken as the default −90. Rows above Nyquist are 0;
/// empty bins or rows <= 0 write nothing; NaN rate/range gives blank rows.
void columnToRows(const float* bins, int count, float rate, int rows, float fLo, float fHi, float dbFloor, float* out01);
void columnToRows(const std::vector<float>& bins, float rate, int rows, float fLo, float fHi, float dbFloor, float* out01);
float freqToRow(float hz, float fLo, float fHi, int rows); // fractional; row 0 = fHi, `rows` = fLo
float rowToFreq(float row, float fLo, float fHi, int rows);
float amplitudeDb(float amp, float ref);                      // −200 for amp ≤ 0 or ref ≤ 0
float partialHeight01(float amp, float refAmp, float dbFloor); // 1 at ref … 0 at the floor
float keyFrequency(int key);                                   // 27.5 · 2^(key/12), key 0 = A0
/// "A0" … "C8" for key 0 … 87 (clamped)
std::string keyName(int key);
/// cents of f from the nearest harmonic n·f0 (n = max(1, round(f/f0)), returned in n). f <= 0, f0 <= 0 or
/// non-finite: 0 cents and n = 1.
float centsFromHarmonic(float f, float f0, int& n);
/// what the partials hover shows: the cents of the partial `f` from the nearest harmonic of the nominal
/// fundamental of the voice's `key` (clamped to 0 … 87) at `concertPitch` (A4, Hz): f0 = keyFrequency(key) · pitch / 440.
/// A 4\' voice's fundamental lands on n = 2. Guards as centsFromHarmonic.
float hoverCents(float f, int key, float concertPitch, int& n);

/// "A0" … "C8" into `out` (NUL-terminated, truncated to `n`); no allocation
void keyNameTo(int key, char* out, size_t n);
/// a frequency as the panel prints it: "55", "1.5k" (%g, kilo from 1000); no allocation
void formatHz(float hz, char* out, size_t n);
/// Watches the engine's dropped-frame counter so the panel mentions drops only while they are happening.
struct DropWatch {
    uint32_t last = 0;      // the counter as last seen
    uint32_t total = 0;     // growth seen since the watch was (re)based
    uint64_t growthAt = 0;  // ms: when the counter last grew
    bool grew = false;      // any growth since the rebase
};
constexpr uint64_t kDropNoticeMs = 2000;
/// Feed the counter as read now. Growth (a wrap at 2^32 included) adds to `total` and stamps `growthAt`; a
/// counter that went down (region re-initialized, panel reopened) is a re-base: no growth, no notice.
DropWatch dropWatchUpdate(DropWatch w, uint32_t counter, uint64_t nowMs);
/// true while the counter grew within the last kDropNoticeMs
bool dropsRecent(const DropWatch& w, uint64_t nowMs);

/// `voices` for a panel that counts none (a bell's players are no voices of the strings)
constexpr uint32_t kNoVoices = 0xFFFFFFFFu;
/// the panel's status text, no allocation: "" when the region is unavailable; "stalled" when no frames come;
/// else "N voice(s)" with " (frozen)" and, when `recentDropped` > 0, ", N dropped" (the drops of the last ~2 s);
/// with kNoVoices "frozen", "N dropped" or nothing
void formatStatus(char* out, size_t n, bool available, uint32_t recentDropped, bool stalled, uint32_t voices, bool frozen);

/// The write position of a ring of `width` columns: logical x 0 is the oldest column, width-1 the newest; a
/// new column overwrites the oldest and nothing moves. Drawn as two rectangles (see split()).
struct ColumnRing {
    int width = 1, head = 0; ///< `head`: the physical index of the oldest column = where the next one is written
    explicit ColumnRing(int w = 1) : width(w < 1 ? 1 : w) {}
    /// the physical index to write the new column to; the ring advances
    int push() { const int i = head; head = (head + 1) % width; return i; }
    int physicalIndex(int logicalX) const { return ((head + logicalX) % width + width) % width; }
    /// the picture left to right: the physical columns [first, first + firstCount) (the older part) then
    /// [0, secondCount) (the newer part); secondCount is 0 when the head is at 0
    struct Split { int first, firstCount, secondCount; };
    Split split() const { return { head, width - head, head }; }
    void reset() { head = 0; }
};

/// The analysis panel's waterfall: an RGBA picture (`columns` × `rows`, physical layout of a ColumnRing, rows from the
/// top) plus the raw dB columns it was made from (only the ones on screen, ≈ 480 × 2049 floats, freed by clear()),
/// so a change of the range or the floor can re-render the picture instead of blanking it.
class Waterfall {
public:
    Waterfall(int columns, int rows);
    /// an empty picture, the retained data freed
    void clear();
    /// the newest column from `bins` (Spectrogram::column()) at sample rate `rate`, mapped with the given range/floor
    void add(const std::vector<float>& bins, float rate, float lo, float hi, float floorDb);
    /// re-render every retained column for a new range/floor (columns not yet retained stay background)
    void remap(float lo, float hi, float floorDb);
    const uint8_t* rgba() const { return fImage.data(); }
    const ColumnRing& ring() const { return fRing; }
    int columns() const { return fRing.width; }
    int rows() const { return fRows; }
    int retained() const { return fCount; } ///< columns with data (<= columns())
    size_t retainedBytes() const { return fRaw.capacity() * sizeof(float); }
private:
    void paint(int physical, float lo, float hi, float floorDb); // the column from the retained bins
    void background(int physical);
    ColumnRing fRing;
    int fRows, fCount = 0;
    size_t fBins = 0;
    std::vector<uint8_t> fImage;
    std::vector<float> fRaw, fRate, fBrightness;
};

} // namespace pe
