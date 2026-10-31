/*
 * Silent stand-in for the Rust engine (pianoengine.h): a buildable dummy for when the
 * engine is missing. The UI runs fully, the sound is silence, the diagnostics are zero.
 * Not a real engine: pe_create never fails, pe_process writes zeros.
 */

#include "pianoengine.h"

#include <cstdio>
#include <cstring>

namespace {

/* the real engine's eight, generic stand-in names */
const char* const kTemperaments[8] = { "Equal", "Meantone", "Werckmeister III", "Vallotti",
    "Young II", "Pythagorean", "Just", "Kirnberger III" };

/* every "engine" is this one address; pe_destroy is a no-op, so aliasing is harmless */
int gDummy;

bool validRegion(const void* base, size_t size)
{
    return base != nullptr && (uintptr_t)base % 8 == 0 && size >= pe_telemetry_region_size();
}

/* header: magic at 0, owner pid at 4 (the region is zeroed by pe_telemetry_init) */
constexpr uint32_t kMagic = 0x50454D54u; /* "PEMT" */

} // namespace

extern "C" {

void pe_set_build_threads(uint32_t) {}
PeCache* pe_cache_new(void) { return nullptr; }
void pe_cache_free(PeCache*) {}

PeEngine* pe_create(const char*, double, const PeTuning*, PeCache*, char*, size_t)
{
    return (PeEngine*)&gDummy;
}

void pe_destroy(PeEngine*) {}

uint32_t pe_temperament_count(void) { return 8; }

const char* pe_temperament_name(uint32_t index)
{
    return index < 8 ? kTemperaments[index] : nullptr;
}

/* one fixed grand, enough for the mic view to draw and the drag maths to hold */
bool pe_mic_geometry(uint32_t body, uint32_t position, double spacing, double distance, double /*lid*/,
    const PeMicSetup* custom, PeMicGeometry* out)
{
    if ((body != 0 && body != 1) || position > 5 || out == nullptr)
        return false;
    if (spacing < 0.0)
        spacing = 0.0;
    if (distance <= 0.0)
        distance = position == 3 ? 0.5 : 2.6; // the position's default

    *out = PeMicGeometry {};
    out->body = body;
    out->has_pair = position > 0 ? 1u : 0u;
    out->spread[0] = 1.0; // the pair is across the piano (x)
    out->spacing = spacing;
    out->spacing_max = 2.0;
    out->case_top_count = 5;
    const double top[5][2] = { { -1.55, -2.2 }, { 1.55, -2.2 }, { 1.55, 0.25 },
        { -1.55, 0.25 }, { -1.55, -2.2 } };
    for (int i = 0; i < 5; ++i) {
        out->case_top[i][0] = top[i][0];
        out->case_top[i][1] = top[i][1];
    }
    out->keys_top[0] = -1.05;
    out->keys_top[1] = 0.25;
    out->keys_top[2] = 1.05;
    out->keys_top[3] = 0.25;
    out->case_front[0] = -1.55;
    out->case_front[1] = 0.0;
    out->case_front[2] = 1.55;
    out->case_front[3] = 1.05;
    out->keys_front[0] = -1.05;
    out->keys_front[1] = 0.0;
    out->keys_front[2] = 1.05;
    out->keys_front[3] = 0.05;
    out->lid_front[0][0] = -1.55; // hinge
    out->lid_front[0][1] = 1.05;
    out->lid_front[1][0] = 0.3; // far edge
    out->lid_front[1][1] = 1.6;
    out->floor_z = -0.75;
    for (int i = 1; i < 5; ++i) {
        out->markers[i][0] = 0.0;
        out->markers[i][1] = -1.2 - 0.7 * (i - 1);
    }
    out->markers[0][0] = out->markers[0][1] = 0.0 / 0.0; // none

    if (out->has_pair) {
        if (position == 3) { // close: a height above the board, dragged in the front view
            out->height_drag = 1u;
            out->axis[2] = 1.0;
            out->distance = distance < 0.2 ? 0.2 : (distance > 2.0 ? 2.0 : distance);
            out->distance_min = 0.2;
            out->distance_max = 2.0;
            const double y = -1.0;
            out->mics[0][0] = -spacing / 2.0;
            out->mics[0][1] = y;
            out->mics[0][2] = out->distance;
            out->mics[1][0] = spacing / 2.0;
            out->mics[1][1] = y;
            out->mics[1][2] = out->distance;
        } else {
            out->axis[1] = -1.0;
            out->distance = distance < 1.0 ? 1.0 : (distance > 6.0 ? 6.0 : distance);
            out->distance_min = 1.0;
            out->distance_max = 6.0;
            const double y = -2.6;
            out->mics[0][0] = -spacing / 2.0;
            out->mics[0][1] = y;
            out->mics[0][2] = 1.0;
            out->mics[1][0] = spacing / 2.0;
            out->mics[1][1] = y;
            out->mics[1][2] = 1.0;
        }
        out->origin[0] = 0.0;
        out->origin[1] = 0.0;
        out->origin[2] = 0.0;
    }
    out->wall_y = 0.0 / 0.0; // grand: no wall
    if (position == 5) {
        out->has_pair = 1;
        out->custom = 1;
        if (custom != nullptr)
            for (int i = 0; i < 2; ++i)
                for (int k = 0; k < 3; ++k)
                    out->mics[i][k] = custom->pos[i][k];
    }
    return true;
}

// identity: nothing changes
bool pe_mic_clamp(uint32_t body, double, PeMicSetup*) { (void)body; return false; }

bool pe_adopt(PeEngine*, PeEngine*) { return true; }
uint32_t pe_latency(const PeEngine*) { return 0; }
void pe_set_params(PeEngine*, const PeParams*) {}
size_t pe_format_spec(const PeParams*, const PeTuning*, const PeVelocityCurve*, char* buf, size_t len)
{
    if (buf != nullptr && len > 0)
        buf[0] = 0;
    return 0;
}
void pe_set_velocity_curve(PeEngine*, const PeVelocityCurve*) {}
bool pe_felt_parse(const char*, PeFelt* out)
{
    *out = PeFelt {};
    return true;
}
size_t pe_felt_format(const PeFelt*, char* buf, size_t len)
{
    if (len > 0)
        buf[0] = 0;
    return 0;
}
bool pe_felt_shipped(const char* const*, const float*, uint32_t, PeFeltKeys*, char* err, size_t err_len)
{
    if (err != nullptr && err_len > 0)
        std::snprintf(err, err_len, "dummy engine: no instruments");
    return false;
}
void pe_felt_apply(const PeFeltKeys* shipped, const PeFelt*, PeFeltKeys* out) { *out = *shipped; }
bool pe_hammer_piano(const PeEngine*) { return false; }
bool pe_velocity_curve_parse(const char*, PeVelocityCurve* out)
{
    // the straight line, whatever the text
    *out = PeVelocityCurve {};
    out->n = 2;
    out->vin[0] = out->vout[0] = 1;
    out->vin[1] = out->vout[1] = 127;
    return true;
}
size_t pe_velocity_curve_format(const PeVelocityCurve*, char* buf, size_t len)
{
    static const char kStraight[] = "1:1,127:127";
    if (buf != nullptr && len > 0)
        std::snprintf(buf, len, "%s", kStraight);
    return sizeof(kStraight) - 1;
}
float pe_velocity_curve_eval(const PeVelocityCurve*, float velocity) { return velocity < 1 ? 1 : velocity > 127 ? 127 : velocity; }
void pe_note_on(PeEngine*, uint32_t, uint8_t, uint8_t) {}
void pe_note_off(PeEngine*, uint32_t, uint8_t) {}
void pe_note_off_velocity(PeEngine*, uint32_t, uint8_t, uint8_t) {}
void pe_pedal(PeEngine*, uint32_t, bool) {}
void pe_pedal_depth(PeEngine*, uint32_t, float) {}
void pe_sostenuto(PeEngine*, uint32_t, bool) {}
void pe_soft_pedal(PeEngine*, uint32_t, float) {}
void pe_all_keys_up(PeEngine*) {}

void pe_process(PeEngine* e, const float*, float* left, float* right, uint32_t frames)
{
    if (e == nullptr)
        return;
    std::memset(left, 0, frames * sizeof(float));
    std::memset(right, 0, frames * sizeof(float));
}

void pe_diag(const PeEngine*, PeDiag* out)
{
    if (out != nullptr)
        std::memset(out, 0, sizeof(PeDiag));
}

size_t pe_telemetry_region_size(void) { return 1u << 18; }

bool pe_telemetry_init(void* base, size_t size)
{
    if (base == nullptr || (uintptr_t)base % 8 != 0 || size < pe_telemetry_region_size())
        return false;
    std::memset(base, 0, size);
    *static_cast<uint32_t*>(base) = kMagic;
    return true;
}

bool pe_telemetry_attach(PeEngine*, void*, size_t, double) { return true; }

bool pe_telemetry_set_enabled(void*, size_t, bool) { return true; }

bool pe_telemetry_read_frame(void*, size_t, PeTelemetryFrame*) { return false; }

uint32_t pe_telemetry_read_audio(void*, size_t, uint64_t*, float*, uint32_t) { return 0; }

uint64_t pe_telemetry_audio_head(void*, size_t) { return 0; }
uint64_t pe_telemetry_notes_head(void*, size_t) { return 0; }
uint32_t pe_telemetry_read_notes(void*, size_t, uint64_t*, uint8_t*, uint32_t) { return 0; }
bool pe_telemetry_set_value(void*, size_t, uint32_t, float) { return false; }
bool pe_telemetry_read_value(void*, size_t, uint32_t, uint32_t*, float*) { return false; }

uint32_t pe_telemetry_dropped(void*, size_t) { return 0; }

bool pe_telemetry_set_owner(void* base, size_t size, uint32_t pid)
{
    if (!validRegion(base, size))
        return false;
    static_cast<uint32_t*>(base)[1] = pid;
    return true;
}

uint32_t pe_telemetry_owner(void* base, size_t size)
{
    if (!validRegion(base, size))
        return 0;
    const uint32_t* words = static_cast<const uint32_t*>(base);
    return words[0] == kMagic ? words[1] : 0; // 0: no valid header, or no owner yet
}

} // extern "C"