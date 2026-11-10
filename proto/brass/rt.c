/* The model in blocks, for playing live: lips.c's loop with its state kept, then what sim.radiate does
   to whole files as streaming filters. The mouthpiece pressure from the modes steepens in a short
   delay line, comes down to the base rate, gains the part of the impedance the modes leave out
   (FIR from the flow) and leaves through the bell (FIR, with the microphone's correction). A note
   change crossfades the filters over 10 ms, as the offline crossfade does. */
#include <math.h>
#include <complex.h>
#include <string.h>
#include <stdlib.h>

#define OS 4
#define MAXM 64
#define HMAX 4096
#define DMAX 256
#define SD 32
#define SD_D 24

typedef struct {
    double complex x[MAXM];
    double ly[8], lv[8];
    double uprev, pprev;
    long noise_pos;
    double sd[SD];
    int si;
    double dp[2 * DMAX], du[2 * DMAX];
    int dpos;
    double ht[2 * HMAX], hu[2 * HMAX];
    int hpos_t, hpos_u;
    const double *h, *r, *h_old, *r_old;
    int nh, nr, nh_old, nr_old, fade;
} RT;

RT *rt_new(void) { return calloc(1, sizeof(RT)); }
void rt_free(RT *s) { free(s); }

void rt_filters(RT *s, const double *h, int nh, const double *r, int nr)
{
    s->h_old = s->h; s->nh_old = s->nh; s->r_old = s->r; s->nr_old = s->nr;
    s->h = h; s->nh = nh; s->r = r; s->nr = nr;
    s->fade = s->h_old ? 441 : 0;
}

static inline double dot(const double *a, const double *b, int n)
{
    double t = 0.0;
    for (int i = 0; i < n; i++) t += a[i] * b[i];
    return t;
}

/* n base-rate samples; controls ramp from *0 to *1 across them */
void rt_process(RT *s, int n, double pm0, double pm1, double lip0, double lip1, double burst0, double burst1,
                double kick, double mu, int m, const double *Er, const double *Ei, const double *rr, const double *ri,
                int nlip, const double *lratio, const double *lq, const double *lsign, const double *lcomp,
                double width, double y0, double soft, double rho, double sr,
                const double *noise, long nnoise, double nl, const double *dec, int ndec, double *out)
{
    double T = 1.0 / (sr * OS), G = 0.0;
    double complex E[MAXM], r[MAXM];
    for (int j = 0; j < m; j++) {
        E[j] = Er[j] + I * Ei[j]; r[j] = rr[j] + I * ri[j];
        G += 2.0 * rr[j];
    }
    int total = n * OS;
    for (int i = 0; i < n; i++) {
        for (int o = 0; o < OS; o++) {
            int k = i * OS + o;
            double w = (double)k / total;
            double pm = pm0 + (pm1 - pm0) * w, lf = lip0 + (lip1 - lip0) * w, bu = burst0 + (burst1 - burst0) * w;
            double dp = pm - s->pprev, y = y0;
            for (int l = 0; l < nlip; l++) {
                double wl = 2.0 * M_PI * lf * lratio[l];
                s->lv[l] += T * (lsign[l] * lcomp[l] * dp / mu - wl / lq[l] * s->lv[l] - wl * wl * s->ly[l]);
                if (l == 0 && kick != 0.0) s->lv[l] += kick / total;  /* over the block: at once it clicks */
                s->ly[l] += T * s->lv[l];
                y += s->ly[l];
            }
            double S = width * 0.5 * (y + sqrt(y * y + soft * soft));
            double ph = 0.0;
            double complex xh[MAXM];
            for (int j = 0; j < m; j++) {
                xh[j] = E[j] * s->x[j] + r[j] * s->uprev * E[j];
                ph += 2.0 * creal(xh[j]);
            }
            double P = pm - ph, uk = 0.0;
            if (S > 0.0) {
                double b = 2.0 * S * S * G / rho, c = 2.0 * S * S * fabs(P) / rho;
                uk = copysign(-b / 2.0 + sqrt(b * b / 4.0 + c), P);
            }
            uk += noise[s->noise_pos] * bu * fabs(uk);
            if (++s->noise_pos >= nnoise) s->noise_pos = 0;
            for (int j = 0; j < m; j++) s->x[j] = xh[j] + r[j] * uk;
            double pk = ph + G * uk;
            s->uprev = uk; s->pprev = pk;
            /* steepening: a sample arrives earlier the higher its pressure */
            s->sd[s->si] = pk;
            int ri0 = (s->si - SD_D + SD) % SD;
            double adv = nl * s->sd[ri0];
            if (adv > SD_D - 2) adv = SD_D - 2;
            if (adv < -(SD - SD_D - 2)) adv = -(SD - SD_D - 2);
            double pos = (double)(s->si - SD_D + SD) + adv;
            int a = (int)floor(pos);
            double fr = pos - a;
            double ys = s->sd[a % SD] * (1.0 - fr) + s->sd[(a + 1) % SD] * fr;
            s->si = (s->si + 1) % SD;
            /* the anti-alias FIR's history, for the base rate */
            s->dpos = s->dpos == 0 ? ndec - 1 : s->dpos - 1;
            s->dp[s->dpos] = s->dp[s->dpos + ndec] = ys;
            s->du[s->dpos] = s->du[s->dpos + ndec] = uk;
        }
        double pb = dot(dec, s->dp + s->dpos, ndec), ub = dot(dec, s->du + s->dpos, ndec);
        s->hpos_u = s->hpos_u == 0 ? HMAX - 1 : s->hpos_u - 1;
        s->hu[s->hpos_u] = s->hu[s->hpos_u + HMAX] = ub;
        double a = dot(s->r, s->hu + s->hpos_u, s->nr);
        if (s->fade > 0) {  /* the part the modes leave out crossfades with the bell's filter */
            double g = s->fade / 441.0;
            a = a * (1.0 - g) + dot(s->r_old, s->hu + s->hpos_u, s->nr_old) * g;
        }
        a += pb;
        s->hpos_t = s->hpos_t == 0 ? HMAX - 1 : s->hpos_t - 1;
        s->ht[s->hpos_t] = s->ht[s->hpos_t + HMAX] = a;
        double y = dot(s->h, s->ht + s->hpos_t, s->nh);
        if (s->fade > 0) {
            double yo = dot(s->h_old, s->ht + s->hpos_t, s->nh_old);
            double g = s->fade / 441.0;
            y = y * (1.0 - g) + yo * g;
            s->fade--;
        }
        out[i] = y;
    }
}
