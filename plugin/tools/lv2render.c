/*
 * Offline LV2 host for testing the plugin: plays MIDI events from a text file and writes
 * a 32-bit float stereo WAV.
 *
 *   lv2render <plugin URI> <events.txt> <out.wav> <seconds> [rate] [max block] [param=value ...]
 *
 * events.txt: one event per line, "<time s> <status> <data1> <data2>" (MIDI bytes, decimal),
 * or a parameter change "@<time s> <symbol>=<value>".
 * LV2RENDER_PACE=<factor>: sleep so the render runs at most <factor> × real time (the
 * worker thread then has time to rebuild after parameter changes).
 * Blocks: random sizes 1…max block (default: fixed 256) to exercise the event splitting.
 * The instrument loads in the plugin's worker thread, so the host first runs 2 s (wall
 * time) of silent blocks; event times count from the end of that pre-roll. The output
 * is not latency-compensated.
 * Build: cc -O2 lv2render.c $(pkg-config --cflags --libs lilv-0) -o lv2render
 */
#include <lilv/lilv.h>
#include <lv2/atom/atom.h>
#include <lv2/atom/util.h>
#include <lv2/buf-size/buf-size.h>
#include <lv2/midi/midi.h>
#include <lv2/options/options.h>
#include <lv2/parameters/parameters.h>
#include <lv2/urid/urid.h>
#include <lv2/worker/worker.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define MAX_URIDS 256
#define ATOM_BUF 65536

static char* uris[MAX_URIDS];
static uint32_t n_uris;

static LV2_URID map_uri(LV2_URID_Map_Handle h, const char* uri)
{
    (void)h;
    for (uint32_t i = 0; i < n_uris; ++i)
        if (!strcmp(uris[i], uri))
            return i + 1;
    if (n_uris == MAX_URIDS)
        return 0;
    uris[n_uris] = strdup(uri);
    return ++n_uris;
}

static const char* unmap_uri(LV2_URID_Unmap_Handle h, LV2_URID urid)
{
    (void)h;
    return urid >= 1 && urid <= n_uris ? uris[urid - 1] : NULL;
}

typedef struct {
    uint64_t frame;
    uint8_t msg[3];
} Ev;

static void write_wav(const char* path, const float* l, const float* r, uint64_t n, uint32_t rate)
{
    FILE* f = fopen(path, "wb");
    if (!f) {
        perror(path);
        exit(1);
    }
    uint32_t data = (uint32_t)(n * 8), u32;
    uint16_t u16;
    fwrite("RIFF", 1, 4, f);
    u32 = 36 + data; fwrite(&u32, 4, 1, f);
    fwrite("WAVEfmt ", 1, 8, f);
    u32 = 16; fwrite(&u32, 4, 1, f);
    u16 = 3; fwrite(&u16, 2, 1, f);          /* IEEE float */
    u16 = 2; fwrite(&u16, 2, 1, f);
    fwrite(&rate, 4, 1, f);
    u32 = rate * 8; fwrite(&u32, 4, 1, f);
    u16 = 8; fwrite(&u16, 2, 1, f);
    u16 = 32; fwrite(&u16, 2, 1, f);
    fwrite("data", 1, 4, f);
    fwrite(&data, 4, 1, f);
    for (uint64_t i = 0; i < n; ++i) {
        fwrite(&l[i], 4, 1, f);
        fwrite(&r[i], 4, 1, f);
    }
    fclose(f);
}

/* worker (the plugin's state needs it): offline, the work runs at once */
static const LV2_Worker_Interface* g_worker;
static LV2_Handle g_handle;

static LV2_Worker_Status worker_respond(LV2_Worker_Respond_Handle h, uint32_t size, const void* data)
{
    (void)h;
    return g_worker && g_worker->work_response ? g_worker->work_response(g_handle, size, data) : LV2_WORKER_SUCCESS;
}

static LV2_Worker_Status worker_schedule(LV2_Worker_Schedule_Handle h, uint32_t size, const void* data)
{
    (void)h;
    return g_worker ? g_worker->work(g_handle, worker_respond, NULL, size, data) : LV2_WORKER_ERR_UNKNOWN;
}

int main(int argc, char** argv)
{
    if (argc < 5) {
        fprintf(stderr, "usage: lv2render <URI> <events.txt> <out.wav> <seconds> [rate] [max block] [symbol=value ...]\n");
        return 1;
    }
    const double rate = argc > 5 ? atof(argv[5]) : 48000.0;
    const uint32_t max_block = argc > 6 ? (uint32_t)atoi(argv[6]) : 0;
    const uint64_t total = (uint64_t)(atof(argv[4]) * rate);

    /* events */
    FILE* ef = fopen(argv[2], "r");
    if (!ef) {
        perror(argv[2]);
        return 1;
    }
    Ev* evs = NULL;
    size_t n_evs = 0, cap = 0;
    struct { uint64_t frame; char sym[64]; float value; int port; } sets[64];
    size_t n_sets = 0;
    double t;
    int s, d1, d2;
    char line[256];
    while (fgets(line, sizeof(line), ef)) {
        char sym[64];
        float v;
        if (line[0] == '@') {
            if (n_sets < 64 && sscanf(line + 1, "%lf %63[^=]=%f", &t, sym, &v) == 3) {
                sets[n_sets].frame = (uint64_t)(t * rate);
                strcpy(sets[n_sets].sym, sym);
                sets[n_sets].value = v;
                sets[n_sets++].port = -1;
            }
            continue;
        }
        if (sscanf(line, "%lf %d %d %d", &t, &s, &d1, &d2) != 4)
            continue;
        if (n_evs == cap)
            evs = realloc(evs, (cap = cap ? 2 * cap : 256) * sizeof(Ev));
        evs[n_evs++] = (Ev){ (uint64_t)(t * rate), { (uint8_t)s, (uint8_t)d1, (uint8_t)d2 } };
    }
    fclose(ef);
    const char* pace_env = getenv("LV2RENDER_PACE");
    const double pace = pace_env ? atof(pace_env) : 0.0;

    /* plugin */
    LilvWorld* world = lilv_world_new();
    lilv_world_load_all(world);
    LilvNode* uri = lilv_new_uri(world, argv[1]);
    const LilvPlugin* plugin = lilv_plugins_get_by_uri(lilv_world_get_all_plugins(world), uri);
    if (!plugin) {
        fprintf(stderr, "plugin %s not found (LV2_PATH?)\n", argv[1]);
        return 1;
    }
    LV2_URID_Map map = { NULL, map_uri };
    LV2_URID_Unmap unmap = { NULL, unmap_uri };
    LV2_Feature map_f = { LV2_URID__map, &map }, unmap_f = { LV2_URID__unmap, &unmap };
    const uint32_t buf_frames = max_block ? max_block : 256;
    const int32_t max_len = (int32_t)buf_frames, min_len = 1;
    const float rate_f = (float)rate;
    const LV2_URID t_int = map_uri(NULL, LV2_ATOM__Int), t_float = map_uri(NULL, LV2_ATOM__Float);
    LV2_Options_Option options[] = {
        { LV2_OPTIONS_INSTANCE, 0, map_uri(NULL, LV2_BUF_SIZE__maxBlockLength), sizeof(int32_t), t_int, &max_len },
        { LV2_OPTIONS_INSTANCE, 0, map_uri(NULL, LV2_BUF_SIZE__minBlockLength), sizeof(int32_t), t_int, &min_len },
        { LV2_OPTIONS_INSTANCE, 0, map_uri(NULL, LV2_PARAMETERS__sampleRate), sizeof(float), t_float, &rate_f },
        { LV2_OPTIONS_INSTANCE, 0, 0, 0, 0, NULL },
    };
    LV2_Feature options_f = { LV2_OPTIONS__options, options };
    LV2_Feature bounded_f = { LV2_BUF_SIZE__boundedBlockLength, NULL };
    LV2_Worker_Schedule schedule = { NULL, worker_schedule };
    LV2_Feature worker_f = { LV2_WORKER__schedule, &schedule };
    const LV2_Feature* features[] = { &map_f, &unmap_f, &options_f, &bounded_f, &worker_f, NULL };
    LilvInstance* inst = lilv_plugin_instantiate(plugin, rate, features);
    if (!inst) {
        fprintf(stderr, "instantiation failed\n");
        return 1;
    }
    g_handle = lilv_instance_get_handle(inst);
    g_worker = (const LV2_Worker_Interface*)lilv_instance_get_extension_data(inst, LV2_WORKER__interface);

    LilvNode* audio = lilv_new_uri(world, LILV_URI_AUDIO_PORT);
    LilvNode* control = lilv_new_uri(world, LILV_URI_CONTROL_PORT);
    LilvNode* atom = lilv_new_uri(world, LV2_ATOM__AtomPort);
    LilvNode* input = lilv_new_uri(world, LILV_URI_INPUT_PORT);
    const uint32_t n_ports = lilv_plugin_get_num_ports(plugin);
    float* controls = calloc(n_ports, sizeof(float));
    float* outs[2] = { NULL, NULL };
    int n_out = 0;
    uint8_t* atom_buf = aligned_alloc(64, ATOM_BUF);
    LV2_URID midi_type = map_uri(NULL, LV2_MIDI__MidiEvent);
    LV2_URID seq_type = map_uri(NULL, LV2_ATOM__Sequence);
    LV2_URID chunk_type = map_uri(NULL, LV2_ATOM__Chunk);
    uint8_t* atom_out = aligned_alloc(64, ATOM_BUF);
    for (uint32_t i = 0; i < n_ports; ++i) {
        const LilvPort* port = lilv_plugin_get_port_by_index(plugin, i);
        if (lilv_port_is_a(plugin, port, audio)) {
            if (n_out < 2) {
                outs[n_out] = calloc(buf_frames, sizeof(float));
                lilv_instance_connect_port(inst, i, outs[n_out++]);
            }
        } else if (lilv_port_is_a(plugin, port, control)) {
            LilvNode* def;
            lilv_port_get_range(plugin, port, &def, NULL, NULL);
            controls[i] = def ? lilv_node_as_float(def) : 0.0f;
            const char* sym = lilv_node_as_string(lilv_port_get_symbol(plugin, port));
            for (size_t k = 0; k < n_sets; ++k)
                if (!strcmp(sets[k].sym, sym))
                    sets[k].port = (int)i;
            for (int a = 7; a < argc; ++a) {
                const char* eq = strchr(argv[a], '=');
                if (eq && (size_t)(eq - argv[a]) == strlen(sym) && !strncmp(argv[a], sym, strlen(sym)))
                    controls[i] = (float)atof(eq + 1);
            }
            lilv_instance_connect_port(inst, i, &controls[i]);
        } else if (lilv_port_is_a(plugin, port, atom)) {
            lilv_instance_connect_port(inst, i, lilv_port_is_a(plugin, port, input) ? atom_buf : atom_out);
        }
    }
    if (n_out != 2) {
        fprintf(stderr, "expected 2 audio outputs\n");
        return 1;
    }

    lilv_instance_activate(inst);
    /* pre-roll in silence while the instrument loads in the plugin's worker thread */
    {
        LV2_Atom_Sequence* seq = (LV2_Atom_Sequence*)atom_buf;
        const struct timespec ts = { 0, 10 * 1000 * 1000 };
        for (int i = 0; i < 200; ++i) {
            seq->atom.type = seq_type;
            seq->atom.size = sizeof(LV2_Atom_Sequence_Body);
            seq->body.unit = 0;
            seq->body.pad = 0;
            ((LV2_Atom*)atom_out)->type = chunk_type;
            ((LV2_Atom*)atom_out)->size = ATOM_BUF - sizeof(LV2_Atom);
            lilv_instance_run(inst, buf_frames);
            nanosleep(&ts, NULL);
        }
    }

    float* L = calloc(total, sizeof(float));
    float* R = calloc(total, sizeof(float));
    uint64_t pos = 0;
    size_t e = 0, k = 0;
    srand(1);
    while (pos < total) {
        uint32_t n = max_block ? 1 + (uint32_t)(rand() % max_block) : buf_frames;
        if (pos + n > total)
            n = (uint32_t)(total - pos);
        for (; k < n_sets && sets[k].frame <= pos; ++k) {
            if (sets[k].port < 0)
                fprintf(stderr, "unknown parameter %s\n", sets[k].sym);
            else
                controls[sets[k].port] = sets[k].value;
        }
        if (pace > 0.0) {
            const long ns = (long)(n / rate / pace * 1e9);
            const struct timespec ts = { ns / 1000000000L, ns % 1000000000L };
            nanosleep(&ts, NULL);
        }
        LV2_Atom_Sequence* seq = (LV2_Atom_Sequence*)atom_buf;
        seq->atom.type = seq_type;
        seq->atom.size = sizeof(LV2_Atom_Sequence_Body);
        seq->body.unit = 0;
        seq->body.pad = 0;
        while (e < n_evs && evs[e].frame < pos + n) {
            uint8_t raw[sizeof(LV2_Atom_Event) + 8] __attribute__((aligned(8)));
            memset(raw, 0, sizeof(raw));
            LV2_Atom_Event* ev = (LV2_Atom_Event*)raw;
            ev->time.frames = (int64_t)(evs[e].frame - pos);
            ev->body.type = midi_type;
            ev->body.size = 3;
            memcpy(raw + sizeof(LV2_Atom_Event), evs[e].msg, 3);
            lv2_atom_sequence_append_event(seq, ATOM_BUF - sizeof(LV2_Atom), ev);
            ++e;
        }
        ((LV2_Atom*)atom_out)->type = chunk_type;
        ((LV2_Atom*)atom_out)->size = ATOM_BUF - sizeof(LV2_Atom);
        lilv_instance_run(inst, n);
        memcpy(L + pos, outs[0], n * sizeof(float));
        memcpy(R + pos, outs[1], n * sizeof(float));
        pos += n;
    }
    lilv_instance_deactivate(inst);
    lilv_instance_free(inst);
    write_wav(argv[3], L, R, total, (uint32_t)rate);
    printf("wrote %s: %.1f s, %zu events\n", argv[3], total / rate, n_evs);
    lilv_world_free(world);
    return 0;
}
