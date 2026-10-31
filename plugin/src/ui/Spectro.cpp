#include "Spectro.hpp"

#include <algorithm>
#include <cassert>
#include <cmath>
#include <cstdio>

namespace pe {

namespace {
constexpr float kFloorDb = -120.0f;
constexpr double kPi = 3.14159265358979323846;

void fft(std::vector<double>& re, std::vector<double>& im) // in place, radix-2, size a power of two
{
    const size_t n = re.size();
    for (size_t i = 1, j = 0; i < n; ++i) { // bit reversal
        size_t bit = n >> 1;
        for (; j & bit; bit >>= 1) j ^= bit;
        j ^= bit;
        if (i < j) { std::swap(re[i], re[j]); std::swap(im[i], im[j]); }
    }
    for (size_t len = 2; len <= n; len <<= 1) {
        const double ang = -2 * kPi / (double)len;
        const double wr = std::cos(ang), wi = std::sin(ang);
        for (size_t i = 0; i < n; i += len) {
            double cr = 1, ci = 0;
            for (size_t k = 0; k < len / 2; ++k) {
                const size_t a = i + k, b = i + k + len / 2;
                const double xr = re[b] * cr - im[b] * ci, xi = re[b] * ci + im[b] * cr;
                re[b] = re[a] - xr; im[b] = im[a] - xi;
                re[a] += xr; im[a] += xi;
                const double t = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = t;
            }
        }
    }
}
}

namespace {
int validSize(int n)
{
    const bool ok = n >= 2 && (n & (n - 1)) == 0;
    assert(ok && "Spectrogram: the FFT size must be a power of two >= 2");
    return ok ? n : 4096;
}
}

Spectrogram::Spectrogram(int fftSize, int hop)
    : fN(validSize(fftSize)), fHop(hop >= 1 ? hop : fN / 4), fRing((size_t)fN, 0.0f), fWindow((size_t)fN), fColumn((size_t)fN / 2 + 1, kFloorDb),
      fRe((size_t)fN), fIm((size_t)fN)
{
    double sum = 0;
    for (int i = 0; i < fN; ++i) { // Hann
        fWindow[(size_t)i] = (float)(0.5 - 0.5 * std::cos(2 * kPi * i / fN));
        sum += fWindow[(size_t)i];
    }
    fScale = 2.0 / sum; // a full-scale sine has the amplitude 1 at its bin: 0 dBFS
}

void Spectrogram::clear()
{
    std::fill(fRing.begin(), fRing.end(), 0.0f);
    std::fill(fColumn.begin(), fColumn.end(), kFloorDb);
    fFilled = fSinceHop = 0;
}

int Spectrogram::push(const float* lr, size_t frames)
{
    int columns = 0;
    for (size_t i = 0; i < frames; ++i) {
        // the ring as a sliding window: shift by one sample is too slow; keep it circular via fFilled
        fRing[(size_t)(fFilled % fN)] = 0.5f * (lr[2 * i] + lr[2 * i + 1]);
        ++fFilled;
        if (++fSinceHop >= fHop && fFilled >= fN) {
            fSinceHop = 0;
            transform();
            ++columns;
        }
    }
    if (fFilled > 4 * fN) fFilled = fFilled % fN + 2 * fN; // keep the counter bounded; the ring index stays congruent
    return columns;
}

void Spectrogram::transform()
{
    const int start = fFilled % fN; // the oldest sample
    for (int i = 0; i < fN; ++i) {
        fRe[(size_t)i] = fRing[(size_t)((start + i) % fN)] * fWindow[(size_t)i];
        fIm[(size_t)i] = 0;
    }
    fft(fRe, fIm);
    for (int k = 0; k <= fN / 2; ++k) {
        double m = std::sqrt(fRe[(size_t)k] * fRe[(size_t)k] + fIm[(size_t)k] * fIm[(size_t)k]) * fScale;
        if (!std::isfinite(m)) m = m > 0 ? 1e6 : 0; // an inf sample must not put inf/NaN in the column
        fColumn[(size_t)k] = m > 1e-6 ? std::max(kFloorDb, (float)(20.0 * std::log10(m))) : kFloorDb;
    }
}

float freqToRow(float hz, float fLo, float fHi, int rows)
{
    const float t = std::log(std::max(hz, 1e-3f) / fLo) / std::log(fHi / fLo); // 0 at fLo … 1 at fHi
    return (1.0f - std::min(1.0f, std::max(0.0f, t))) * (float)rows;
}

float rowToFreq(float row, float fLo, float fHi, int rows)
{
    const float t = 1.0f - row / (float)rows;
    return fLo * std::pow(fHi / fLo, std::min(1.0f, std::max(0.0f, t)));
}

void columnToRows(const std::vector<float>& bins, float rate, int rows, float fLo, float fHi, float dbFloor, float* out)
{
    columnToRows(bins.data(), (int)bins.size(), rate, rows, fLo, fHi, dbFloor, out);
}

void columnToRows(const float* bins, int count, float rate, int rows, float fLo, float fHi, float dbFloor, float* out)
{
    const int nb = count;
    if (bins == nullptr || nb < 1 || rows <= 0) return;
    if (!(dbFloor < 0.0f)) dbFloor = -90.0f; // 0, positive or NaN would divide by zero or invert the scale: the default
    const float nyquist = rate / 2, binHz = nyquist / (float)std::max(1, nb - 1);
    for (int r = 0; r < rows; ++r) {
        const float hi = rowToFreq((float)r, fLo, fHi, rows), lo = rowToFreq((float)r + 1, fLo, fHi, rows);
        if (!(lo < nyquist)) { out[r] = 0.0f; continue; } // above Nyquist (also fully): nothing to show
        const int b0 = std::max(0, std::min(nb - 1, (int)std::floor(lo / binHz)));
        const int b1 = std::max(b0, std::min(nb - 1, (int)std::ceil(std::min(hi, nyquist) / binHz)));
        float db = -200.0f;
        for (int b = b0; b <= b1; ++b) db = std::max(db, bins[(size_t)b]);
        out[r] = std::min(1.0f, std::max(0.0f, (db - dbFloor) / -dbFloor));
    }
}

float amplitudeDb(float amp, float ref)
{
    if (!(amp > 0.0f) || !(ref > 0.0f)) return -200.0f;
    return 20.0f * std::log10(amp / ref);
}

float partialHeight01(float amp, float refAmp, float dbFloor)
{
    return std::min(1.0f, std::max(0.0f, 1.0f + amplitudeDb(amp, refAmp) / -dbFloor));
}

float keyFrequency(int key) { return 27.5f * std::pow(2.0f, (float)key / 12.0f); }

void keyNameTo(int key, char* out, size_t n)
{
    static const char* names[12] = { "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B" };
    const int midi = std::min(87, std::max(0, key)) + 21;
    std::snprintf(out, n, "%s%d", names[midi % 12], midi / 12 - 1);
}

std::string keyName(int key)
{
    char b[8];
    keyNameTo(key, b, sizeof b);
    return b;
}

void formatHz(float f, char* out, size_t n)
{
    if (f >= 1000.0f) std::snprintf(out, n, "%gk", (double)f / 1000.0);
    else std::snprintf(out, n, "%g", (double)f);
}

DropWatch dropWatchUpdate(DropWatch w, uint32_t counter, uint64_t nowMs)
{
    const uint32_t delta = counter - w.last; // modulo 2^32: a wrap is a small positive step
    if (delta != 0 && delta < 0x80000000u) {
        w.total += delta;
        w.growthAt = nowMs;
        w.grew = true;
    }
    w.last = counter; // down (a "negative" delta) is a re-base
    return w;
}

bool dropsRecent(const DropWatch& w, uint64_t nowMs)
{
    return w.grew && nowMs >= w.growthAt && nowMs - w.growthAt <= kDropNoticeMs;
}

void formatStatus(char* out, size_t n, bool available, uint32_t recentDropped, bool stalled, uint32_t voices, bool frozen)
{
    if (!available) std::snprintf(out, n, "%s", "");
    else if (stalled) std::snprintf(out, n, "%s", "stalled");
    else if (recentDropped > 0)
        std::snprintf(out, n, "%u %s%s, %u dropped", (unsigned)voices, voices == 1 ? "voice" : "voices", frozen ? " (frozen)" : "", (unsigned)recentDropped);
    else std::snprintf(out, n, "%u %s%s", (unsigned)voices, voices == 1 ? "voice" : "voices", frozen ? " (frozen)" : "");
}

namespace {
// the waterfall's ramp: dark blue (0) → cyan → white (1)
void ramp(float v, uint8_t* px)
{
    px[0] = (uint8_t)(255 * std::max(0.0f, std::min(1.0f, v * 1.5f - 0.5f)));
    px[1] = (uint8_t)(255 * std::min(1.0f, v * 1.6f));
    px[2] = (uint8_t)(255 * std::min(1.0f, 0.2f + v * 0.9f));
    px[3] = 255;
}
}

Waterfall::Waterfall(int columns, int rows)
    : fRing(columns), fRows(rows < 1 ? 1 : rows), fRate((size_t)fRing.width, 48000.0f), fBrightness((size_t)fRows)
{
    fImage.resize((size_t)fRing.width * (size_t)fRows * 4);
    clear();
}

void Waterfall::background(int c)
{
    for (int y = 0; y < fRows; ++y) ramp(0.0f, &fImage[((size_t)y * (size_t)fRing.width + (size_t)c) * 4]);
}

void Waterfall::clear()
{
    fRing.reset();
    fCount = 0;
    fBins = 0;
    std::vector<float>().swap(fRaw); // free
    for (int c = 0; c < fRing.width; ++c) background(c);
}

void Waterfall::paint(int c, float lo, float hi, float floorDb)
{
    columnToRows(fRaw.data() + (size_t)c * fBins, (int)fBins, fRate[(size_t)c], fRows, lo, hi, floorDb, fBrightness.data());
    for (int y = 0; y < fRows; ++y) ramp(fBrightness[(size_t)y], &fImage[((size_t)y * (size_t)fRing.width + (size_t)c) * 4]);
}

void Waterfall::add(const std::vector<float>& bins, float rate, float lo, float hi, float floorDb)
{
    if (bins.empty()) return;
    if (bins.size() != fBins) { // first column (or a different transform size): start the retained data over
        fBins = bins.size();
        fRaw.assign(fBins * (size_t)fRing.width, 0.0f);
        fCount = 0;
        for (int c = 0; c < fRing.width; ++c) background(c);
    }
    const int c = fRing.push();
    std::copy(bins.begin(), bins.end(), fRaw.begin() + (long)((size_t)c * fBins));
    fRate[(size_t)c] = rate;
    fCount = std::min(fCount + 1, fRing.width);
    paint(c, lo, hi, floorDb);
}

void Waterfall::remap(float lo, float hi, float floorDb)
{
    for (int l = 0; l < fRing.width; ++l) {
        const int c = fRing.physicalIndex(l);
        if (l >= fRing.width - fCount) paint(c, lo, hi, floorDb);
        else background(c);
    }
}

float centsFromHarmonic(float f, float f0, int& n)
{
    n = 1;
    if (!(f > 0) || !(f0 > 0) || !std::isfinite(f) || !std::isfinite(f0)) return 0.0f;
    n = std::max(1, (int)std::lround(f / f0));
    return 1200.0f * std::log2(f / ((float)n * f0));
}

float hoverCents(float f, int key, float concertPitch, int& n)
{
    return centsFromHarmonic(f, keyFrequency(std::min(87, std::max(0, key))) * concertPitch / 440.0f, n);
}

} // namespace pe
