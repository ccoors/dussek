//! The references' bodies' main directions (principal components of their weighted vectors around
//! their mean). The first is warmth (+ warm: more bass radiated), the second air (+ airy: a longer
//! ring); a body is the mean plus each knob × its direction's standard deviation × the direction.
use super::bands::{BANDS, grid};
use super::body::Body;

pub struct Basis {
    pub mean: Vec<f64>,
    /// unit vectors in the weighted space, by falling standard deviation
    pub directions: Vec<Vec<f64>>,
    /// the references' standard deviation along each direction
    pub sd: Vec<f64>,
}

/// Eigenvalues and eigenvectors (columns) of a symmetric matrix by cyclic Jacobi rotations.
fn jacobi(mut a: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut v: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| (i == j) as u8 as f64).collect()).collect();
    let norm: f64 = a.iter().flatten().map(|x| x * x).sum::<f64>().sqrt();
    for _ in 0..100 {
        let off: f64 = (0..n).flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j))).map(|(i, j)| a[i][j] * a[i][j]).sum::<f64>().sqrt();
        if off <= 1e-15 * norm {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() <= 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let (c, s) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
                for row in a.iter_mut() {
                    let (akp, akq) = (row[p], row[q]);
                    row[p] = c * akp - s * akq;
                    row[q] = s * akp + c * akq;
                }
                let (lo, hi) = a.split_at_mut(q);
                for (apk, aqk) in lo[p].iter_mut().zip(hi[0].iter_mut()) {
                    let (x, y) = (*apk, *aqk);
                    *apk = c * x - s * y;
                    *aqk = s * x + c * y;
                }
                for row in v.iter_mut() {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

impl Basis {
    /// The bodies' principal components through their N × N Gram matrix: at most N − 1 (the mean
    /// taken out), and directions with a standard deviation under 1e-9 of the largest dropped (the
    /// zero direction's rounding read 1e-9 absolute on nine refitted pianos); oriented (module doc; each further one so that its
    /// largest weight is positive).
    pub fn new(bodies: &[Body]) -> Basis {
        let n = bodies.len();
        let rows: Vec<Vec<f64>> = bodies.iter().map(Body::weighted).collect();
        let dim = rows[0].len();
        let mean: Vec<f64> = (0..dim).map(|j| rows.iter().map(|r| r[j]).sum::<f64>() / n as f64).collect();
        let x: Vec<Vec<f64>> = rows.iter().map(|r| r.iter().zip(&mean).map(|(a, m)| a - m).collect()).collect();
        let gram: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| dot(&x[i], &x[j])).collect()).collect();
        let (vals, vecs) = jacobi(gram);
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| vals[b].total_cmp(&vals[a]));
        let g = grid();
        let (mut directions, mut sd) = (Vec::new(), Vec::new());
        let s_max = vals[order[0]].max(0.0).sqrt();
        for (rank, &i) in order.iter().take(n.saturating_sub(1)).enumerate() {
            let s = vals[i].max(0.0).sqrt();
            if s < 1e-9 * s_max || s == 0.0 {
                continue;
            }
            let mut d: Vec<f64> = (0..dim).map(|j| (0..n).map(|r| x[r][j] * vecs[r][i]).sum::<f64>() / s).collect();
            let flip = match rank {
                0 => d[10] < 0.0,
                1 => (0..BANDS).filter(|&k| g[k] > 1000.0).map(|k| d[BANDS + k]).sum::<f64>() > 0.0,
                _ => d.iter().fold(0.0f64, |m, &v| if v.abs() > m.abs() { v } else { m }) < 0.0,
            };
            if flip {
                d.iter_mut().for_each(|v| *v = -*v);
            }
            directions.push(d);
            sd.push(s / (n as f64).sqrt());
        }
        Basis { mean, directions, sd }
    }

    /// The body these knobs make (missing knobs count 0).
    pub fn body(&self, knobs: &[f64]) -> Body {
        let mut v = self.mean.clone();
        for ((d, s), k) in self.directions.iter().zip(&self.sd).zip(knobs) {
            v.iter_mut().zip(d).for_each(|(a, b)| *a += k * s * b);
        }
        Body::unweighted(&v)
    }

    /// A body's knobs: its projection on each direction over that direction's standard deviation.
    pub fn knobs(&self, b: &Body) -> Vec<f64> {
        let w = b.weighted();
        let c: Vec<f64> = w.iter().zip(&self.mean).map(|(a, m)| a - m).collect();
        self.directions.iter().zip(&self.sd).map(|(d, s)| dot(&c, d) / s).collect()
    }
}
