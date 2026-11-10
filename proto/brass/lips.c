/* The per-sample loop of sim.simulate in C (the spike's renders were Python-bound).
   Lips: one or more mechanical modes driven by the pressure across them, each with its own
   frequency (a multiple of the lip frequency), Q, sign (+1 outward, -1 inward) and compliance;
   the opening is their sum. The bore: complex first-order modes per fingering. */
#include <math.h>
#include <complex.h>

void simulate(int N, double sr, const double *pm, const double *lipf, const double *mus,
              const int *seg, const double *kick, const double *noise,
              const int *moff, const int *mcount,
              const double *Er, const double *Ei, const double *rr, const double *ri,
              int nlip, const double *lratio, const double *lq, const double *lsign,
              const double *lcomp, double width, double y0, double soft, double rho,
              double *u, double *p)
{
    double T = 1.0 / sr;
    double complex x[64] = {0};
    double ly[8] = {0}, lv[8] = {0};
    double uprev = 0.0, pprev = 0.0;
    for (int k = 0; k < N; k++) {
        int s = seg[k], off = moff[s], m = mcount[s];
        double G = 0.0;
        for (int j = 0; j < m; j++) G += 2.0 * rr[off + j];
        double dp = pm[k] - pprev, y = y0;
        for (int l = 0; l < nlip; l++) {
            double wl = 2.0 * M_PI * lipf[k] * lratio[l];
            lv[l] += T * (lsign[l] * lcomp[l] * dp / mus[k] - wl / lq[l] * lv[l] - wl * wl * ly[l]);
            if (l == 0 && kick[k] != 0.0) lv[l] += kick[k];
            ly[l] += T * lv[l];
            y += ly[l];
        }
        double S = soft > 0.0 ? width * 0.5 * (y + sqrt(y * y + soft * soft))
                              : (y > 0.0 ? width * y : 0.0);
        double ph = 0.0;
        double complex xh[64];
        for (int j = 0; j < m; j++) {
            double complex E = Er[off + j] + I * Ei[off + j];
            double complex r = rr[off + j] + I * ri[off + j];
            xh[j] = E * x[j] + r * uprev * E;
            ph += 2.0 * creal(xh[j]);
        }
        double P = pm[k] - ph, uk = 0.0;
        if (S > 0.0) {
            double b = 2.0 * S * S * G / rho, c = 2.0 * S * S * fabs(P) / rho;
            uk = copysign(-b / 2.0 + sqrt(b * b / 4.0 + c), P);
        }
        uk += noise[k] * fabs(uk);
        for (int j = 0; j < m; j++) {
            double complex r = rr[off + j] + I * ri[off + j];
            x[j] = xh[j] + r * uk;
        }
        double pk = ph + G * uk;
        u[k] = uk; p[k] = pk; uprev = uk; pprev = pk;
    }
}
