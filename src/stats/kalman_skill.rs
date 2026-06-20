//! State-space "global skill" model for 3x3, replacing the scalar EWMA
//! (`skill_estimator.rs`) with a Kalman filter + RTS smoother in log-time.
//!
//! Per person, per ISO-week, we collapse all 3x3 solves into sufficient
//! statistics (count, mean log-time, sample variance) and run two coupled
//! linear-Gaussian filters:
//!
//!   * Filter A — latent skill, as a *damped local linear trend* `[level, slope]`.
//!     `level` = mean log solve-time; `slope` = improvement per week (damped by φ
//!     so improvement bends into a plateau). Robust Student-t observation noise
//!     down-weights outlier weeks; measurement variance is `exp(h)/n` (Filter B).
//!   * Filter B — per-person log within-week variance `h`, tracked from the
//!     bias-corrected log sample variance (Harvey-style, exact digamma correction).
//!
//! A forward pass (Kalman) then a backward pass (RTS smoother) give the full
//! smoothed trajectory + uncertainty — the exact linear-Gaussian equivalent of
//! TrueSkill-Through-Time. The smoothed state drives a Monte-Carlo ao5 simulator
//! for P(sub-X), expected ao5, and record probabilities.
//!
//! Global hyperparameters (σ²_η, σ²_ξ, φ) are fit by maximizing a speed-weighted
//! (`1/x²`) pooled one-step predictive log-likelihood over a person subsample.

use std::collections::HashMap;

use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;

use crate::db::WcaDb;

/// Speed events covered (all except blindfolded, FMC, feet, and clock).
const EVENTS: &[&str] =
    &["222", "333", "444", "555", "666", "777", "333oh", "pyram", "skewb", "sq1", "minx"];
const TOP_N: usize = 1000;
const N_TRACKS: usize = 200;
const MC_SAMPLES: usize = 20_000;
const NU: f64 = 5.0; // Student-t dof for robust observation noise.

/// sub-X thresholds (centiseconds) per event for the simulation summary.
fn subs_for(event: &str) -> &'static [i32] {
    match event {
        "222" => &[200, 150, 100],
        "333" => &[1000, 800, 700, 600, 500],
        "444" => &[4000, 3000, 2500],
        "555" => &[7000, 6000, 5000],
        "666" => &[13000, 11000],
        "777" => &[19000, 16000],
        "333oh" => &[1500, 1200, 1000, 900],
        "pyram" => &[400, 300, 200],
        "skewb" => &[400, 300, 200],
        "sq1" => &[900, 700, 500],
        "minx" => &[4000, 3500, 3000],
        _ => &[],
    }
}

// ── date helpers (shared shape with skill_estimator.rs) ─────────────────────
fn ymd_to_jdn(year: u16, month: u8, day: u8) -> i32 {
    let (y, m, d) = (year as i32, month as i32, day as i32);
    let a = (14 - m) / 12;
    let y2 = y + 4800 - a;
    let m2 = m + 12 * a - 3;
    d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045
}

fn jdn_to_iso(j: i32) -> String {
    let a = j + 32044;
    let b = (4 * a + 3) / 146097;
    let c = a - (146097 * b) / 4;
    let d2 = (4 * c + 3) / 1461;
    let e = c - (1461 * d2) / 4;
    let m = (5 * e + 2) / 153;
    let day = e - (153 * m + 2) / 5 + 1;
    let month = m + 3 - 12 * (m / 10);
    let year = 100 * b + d2 - 4800 + m / 10;
    format!("{year:04}-{month:02}-{day:02}")
}

/// Weekday name from a Julian Day Number. jdn % 7 == 3 is Thursday (1970-01-01),
/// so rem 0 = Mon, 1 = Tue, … 6 = Sun.
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
fn weekday(jdn: i32) -> usize {
    jdn.rem_euclid(7) as usize
}

// ── special functions ───────────────────────────────────────────────────────
fn digamma(mut x: f64) -> f64 {
    let mut r = 0.0;
    while x < 6.0 {
        r -= 1.0 / x;
        x += 1.0;
    }
    let inv = 1.0 / x;
    let inv2 = inv * inv;
    r + x.ln() - 0.5 * inv - inv2 * (1.0 / 12.0 - inv2 * (1.0 / 120.0 - inv2 / 252.0))
}

/// Standard normal CDF via erf (Abramowitz–Stegun 7.1.26).
fn normal_cdf(z: f64) -> f64 {
    let x = z / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    let erf = if x >= 0.0 { y } else { -y };
    0.5 * (1.0 + erf)
}

/// Inverse standard-normal CDF (Acklam's rational approximation, ~1e-9).
fn inv_norm(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02,
        1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02,
        6.680131188771972e+01, -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00,
        -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let pl = 0.02425;
    if p < pl {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - pl {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// sinh-arcsinh shape transform of a standard-normal draw (Jones–Pewsey).
/// `eps` = skew (>0 ⇒ right skew), `delta` = tail weight (<1 ⇒ heavier).
/// Empirically fit on 3x3 within-week log-residuals: eps≈0.15, delta≈0.72–0.82.
const SAS_EPS: f64 = 0.15;

fn sas(z: f64, eps: f64, delta: f64) -> f64 {
    ((z.asinh() + eps) / delta).sinh()
}

/// Tail-weight delta as a clamped linear ramp in log-skill (faster ⇒ lighter
/// tails). Fit to the per-tier MLE: ~0.82 at sub-6, ~0.72 at 25s.
fn sas_delta(level_logcs: f64) -> f64 {
    (0.82 - 0.066 * (level_logcs - 6.31)).clamp(0.70, 0.84)
}

/// Mean and variance of the standardized SAS shape S(Z), by deterministic
/// midpoint quadrature over the normal — so simulated solves can be rescaled to
/// keep the filter's log-mean and log-variance exactly.
fn sas_consts(eps: f64, delta: f64) -> (f64, f64) {
    const K: usize = 512;
    let mut m = 0.0;
    let mut m2 = 0.0;
    for k in 0..K {
        let z = inv_norm((k as f64 + 0.5) / K as f64);
        let s = sas(z, eps, delta);
        m += s;
        m2 += s * s;
    }
    m /= K as f64;
    m2 /= K as f64;
    (m, m2 - m * m)
}

fn trigamma(mut x: f64) -> f64 {
    let mut r = 0.0;
    while x < 6.0 {
        r += 1.0 / (x * x);
        x += 1.0;
    }
    let inv = 1.0 / x;
    let inv2 = inv * inv;
    r + inv * (1.0 + inv * (0.5 + inv * (1.0 / 6.0 - inv2 * (1.0 / 30.0 - inv2 / 42.0))))
}

/// Bias-correction constant so that `ln(s²) - C_n` is an unbiased estimate of
/// `ln(σ²)` for a sample of `n` normal draws (Harvey log-variance trick).
fn log_var_correction(n: usize) -> f64 {
    let k = (n - 1) as f64;
    std::f64::consts::LN_2 + digamma(k / 2.0) - k.ln()
}
/// Measurement-error variance of that estimator (known a priori from `n`).
fn log_var_noise(n: usize) -> f64 {
    trigamma((n - 1) as f64 / 2.0)
}

// ── tiny deterministic RNG (SplitMix64 + Box–Muller) ────────────────────────
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        // 53-bit uniform in (0,1).
        ((self.next_u64() >> 11) as f64 + 0.5) * (1.0 / (1u64 << 53) as f64)
    }
    fn normal(&mut self) -> f64 {
        let u1 = self.unit();
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

// ── data structures ─────────────────────────────────────────────────────────
/// One active week for one person.
struct WeekObs {
    wk: i32,
    n: usize,
    ybar: f64, // mean log-time
    s2: f64,   // sample variance of log-time (0 if n < 2)
}

struct PersonSeries {
    pid: String,
    name: String,
    country: String,
    weeks: Vec<WeekObs>, // sorted ascending by wk
    dnf: u32,
    attempts: u32,
}

/// Global model hyperparameters (log-time, per-week units).
/// Trend is **decay-to-floor**: state [level ℓ, floor f], ℓ'=(1−k)ℓ+k·f, f'=f.
/// (Field names retain `phi`/`q_eta`/`q_xi` for ABI minimalism but mean k / level
/// noise / floor noise respectively.)
#[derive(Clone, Copy)]
struct Hyper {
    q_eta: f64, // level process variance (q_ℓ)
    q_xi: f64,  // floor process variance (q_f) — floor drifts slowly
    phi: f64,   // mean-reversion rate k toward the floor (per week)
    // Filter B (volatility) — fixed sensible values with floors.
    q_h: f64,
    h0: f64, // prior mean log-variance
    r_min: f64,
    r_max: f64,
}

// ── 2x2 linear algebra for Filter A ─────────────────────────────────────────
type M2 = [[f64; 2]; 2];
type V2 = [f64; 2];

/// Effective transition `G^g` and accumulated process noise `Q_g` for a gap of
/// `g` weeks under the damped-trend dynamics. Loop is capped; the long tail
/// (where φ^i → 0) is added as a steady-state level term.
fn gap_transition(h: &Hyper, g: i32) -> (M2, M2) {
    // Decay-to-floor: single-step G = [[a, k],[0,1]], a = 1−k. Closed-form G^g and
    // accumulated Q_g = Σ_{i=0}^{g-1} G^i Q (G^i)^T, Q = diag(q_ℓ, q_f).
    let k = h.phi;
    let a = (1.0 - k).max(0.0);
    let g = g.max(1);
    let (ql, qf) = (h.q_eta, h.q_xi);
    let ag = a.powi(g);
    let g_eff: M2 = [[ag, 1.0 - ag], [0.0, 1.0]];

    let one_ma = (1.0 - a).max(1e-12);
    let one_ma2 = (1.0 - a * a).max(1e-12);
    let sa = (1.0 - a.powi(g)) / one_ma; // Σ a^i
    let sa2 = (1.0 - a.powi(2 * g)) / one_ma2; // Σ a^{2i}
    let gf = g as f64;
    // G^i = [[a^i, 1−a^i],[0,1]] ⇒ contribution [[a^{2i}qℓ+(1−a^i)²qf, (1−a^i)qf],[·,qf]].
    let q00 = ql * sa2 + qf * (gf - 2.0 * sa + sa2);
    let q01 = qf * (gf - sa);
    let q: M2 = [[q00, q01], [q01, qf * gf]];
    (g_eff, q)
}

fn mat_mul(a: M2, b: M2) -> M2 {
    [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0],
            a[0][0] * b[0][1] + a[0][1] * b[1][1],
        ],
        [
            a[1][0] * b[0][0] + a[1][1] * b[1][0],
            a[1][0] * b[0][1] + a[1][1] * b[1][1],
        ],
    ]
}
fn transpose(a: M2) -> M2 {
    [[a[0][0], a[1][0]], [a[0][1], a[1][1]]]
}
fn mat_add(a: M2, b: M2) -> M2 {
    [
        [a[0][0] + b[0][0], a[0][1] + b[0][1]],
        [a[1][0] + b[1][0], a[1][1] + b[1][1]],
    ]
}
fn mat_vec(a: M2, v: V2) -> V2 {
    [a[0][0] * v[0] + a[0][1] * v[1], a[1][0] * v[0] + a[1][1] * v[1]]
}
fn inv2(a: M2) -> M2 {
    let det = a[0][0] * a[1][1] - a[0][1] * a[1][0];
    let id = 1.0 / det;
    [[a[1][1] * id, -a[0][1] * id], [-a[1][0] * id, a[0][0] * id]]
}

// ── 3×3 helpers for the local-quadratic trend experiment ─────────────────────
type M3 = [[f64; 3]; 3];
type V3 = [f64; 3];
fn mm3(a: M3, b: M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    o
}
fn t3(a: M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = a[j][i];
        }
    }
    o
}
fn add3(a: M3, b: M3) -> M3 {
    let mut o = a;
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] += b[i][j];
        }
    }
    o
}
fn mv3(a: M3, v: V3) -> V3 {
    [
        a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2],
        a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2],
        a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2],
    ]
}

/// Local-quadratic trend hyperparameters: state [level, velocity, acceleration],
/// ℓ'=ℓ+v, v'=v+a, a'=phi_a·a; process noise diag(qe,qv,qa). Same obs/volatility
/// model as the 2-state (h0,q_h,r_min,r_max reused from the fitted `Hyper`).
#[derive(Clone, Copy)]
struct Hyper3 {
    qe: f64,
    qv: f64,
    qa: f64,
    phi_a: f64,
}

/// Forward 3-state filter, returns (weighted one-step loglik, innovation accumulators).
/// `(z_sum, z2_sum, z_n, age_zsum[4], age_z2[4], age_n[4])`.
type Z3 = (f64, f64, u64, [f64; 4], [f64; 4], [u64; 4]);
fn run3_eval(s: &PersonSeries, h3: &Hyper3, base: &Hyper, weight: f64) -> (f64, Z3) {
    let m = s.weeks.len();
    let mut hb = base.h0;
    let mut pb = 4.0;
    let g: M3 = [[1.0, 1.0, 0.0], [0.0, 1.0, 1.0], [0.0, 0.0, h3.phi_a]];
    let q: M3 = [[h3.qe, 0.0, 0.0], [0.0, h3.qv, 0.0], [0.0, 0.0, h3.qa]];
    let mut x: V3 = [s.weeks[0].ybar, 0.0, 0.0];
    let mut p: M3 = [[0.25, 0.0, 0.0], [0.0, 0.04, 0.0], [0.0, 0.0, 0.01]];
    let mut loglik = 0.0;
    let (mut z_sum, mut z2_sum, mut z_n) = (0.0f64, 0.0f64, 0u64);
    let (mut azs, mut az2, mut an) = ([0.0f64; 4], [0.0f64; 4], [0u64; 4]);
    let wk0 = s.weeks[0].wk;

    for t in 0..m {
        let w = &s.weeks[t];
        if t > 0 {
            let gap = (w.wk - s.weeks[t - 1].wk).max(1);
            for _ in 0..gap.min(156) {
                x = mv3(g, x);
                p = add3(mm3(mm3(g, p), t3(g)), q);
            }
            pb += base.q_h * gap as f64;
        }
        let sigma2 = hb.exp().clamp(base.r_min, base.r_max);
        let r_t = sigma2 / (w.n as f64);
        let innov = w.ybar - x[0];
        let f = p[0][0] + r_t;
        loglik += -0.5 * (f.ln() + innov * innov / f);
        if t > 0 {
            let z = innov / f.sqrt();
            z_sum += z;
            z2_sum += z * z;
            z_n += 1;
            let age = w.wk - wk0;
            let b = if age < 8 { 0 } else if age < 52 { 1 } else if age < 156 { 2 } else { 3 };
            azs[b] += z;
            az2[b] += z * z;
            an[b] += 1;
        }
        let lambda = (NU + 1.0) / (NU + innov * innov / f);
        let r_eff = r_t / lambda;
        let f_eff = p[0][0] + r_eff;
        let k: V3 = [p[0][0] / f_eff, p[1][0] / f_eff, p[2][0] / f_eff];
        x = [x[0] + k[0] * innov, x[1] + k[1] * innov, x[2] + k[2] * innov];
        // Joseph form, Z = [1,0,0].
        let ikz: M3 = [[1.0 - k[0], 0.0, 0.0], [-k[1], 1.0, 0.0], [-k[2], 0.0, 1.0]];
        let mut pn = mm3(mm3(ikz, p), t3(ikz));
        for i in 0..3 {
            for j in 0..3 {
                pn[i][j] += k[i] * k[j] * r_eff;
            }
        }
        p = pn;
        if w.n >= 2 && w.s2 > 0.0 {
            let yl = w.s2.ln() - log_var_correction(w.n);
            let rl = log_var_noise(w.n);
            let s_inn = pb + rl;
            let kb = pb / s_inn;
            hb += kb * (yl - hb);
            pb = (1.0 - kb) * pb;
            if pb < base.q_h {
                pb = base.q_h;
            }
        }
    }
    (loglik * weight, (z_sum, z2_sum, z_n, azs, az2, an))
}

fn pooled_loglik3(sample: &[(&PersonSeries, f64)], h3: &Hyper3, base: &Hyper) -> f64 {
    sample.par_iter().map(|(s, w)| run3_eval(s, h3, base, *w).0).sum()
}

/// Decay-to-floor trend: state [level ℓ, floor f], ℓ'=(1−k)ℓ+k·f, f'=f.
/// Velocity is derived (−k(ℓ−f)); the floor is a slowly-drifting per-person latent
/// (= projected potential). Empirically k≈0.0135/wk (1-yr gap half-life).
#[derive(Clone, Copy)]
struct HyperMR {
    k: f64,
    q_l: f64,
    q_f: f64,
}

fn runmr_eval(s: &PersonSeries, h: &HyperMR, base: &Hyper, weight: f64) -> (f64, Z3) {
    let m = s.weeks.len();
    let mut hb = base.h0;
    let mut pb = 4.0;
    let a = 1.0 - h.k;
    let g: M2 = [[a, h.k], [0.0, 1.0]];
    let q: M2 = [[h.q_l, 0.0], [0.0, h.q_f]];
    let mut x: V2 = [s.weeks[0].ybar, s.weeks[0].ybar];
    let mut p: M2 = [[0.25, 0.0], [0.0, 0.5]];
    let mut loglik = 0.0;
    let (mut z_sum, mut z2_sum, mut z_n) = (0.0f64, 0.0f64, 0u64);
    let (mut azs, mut az2, mut an) = ([0.0f64; 4], [0.0f64; 4], [0u64; 4]);
    let wk0 = s.weeks[0].wk;

    for t in 0..m {
        let w = &s.weeks[t];
        if t > 0 {
            let gap = (w.wk - s.weeks[t - 1].wk).max(1);
            for _ in 0..gap.min(156) {
                x = mat_vec(g, x);
                p = mat_add(mat_mul(mat_mul(g, p), transpose(g)), q);
            }
            pb += base.q_h * gap as f64;
        }
        let sigma2 = hb.exp().clamp(base.r_min, base.r_max);
        let r_t = sigma2 / (w.n as f64);
        let innov = w.ybar - x[0];
        let f = p[0][0] + r_t;
        loglik += -0.5 * (f.ln() + innov * innov / f);
        if t > 0 {
            let z = innov / f.sqrt();
            z_sum += z;
            z2_sum += z * z;
            z_n += 1;
            let age = w.wk - wk0;
            let b = if age < 8 { 0 } else if age < 52 { 1 } else if age < 156 { 2 } else { 3 };
            azs[b] += z;
            az2[b] += z * z;
            an[b] += 1;
        }
        let lambda = (NU + 1.0) / (NU + innov * innov / f);
        let r_eff = r_t / lambda;
        let f_eff = p[0][0] + r_eff;
        let kg: V2 = [p[0][0] / f_eff, p[1][0] / f_eff];
        x = [x[0] + kg[0] * innov, x[1] + kg[1] * innov];
        let ikz: M2 = [[1.0 - kg[0], 0.0], [-kg[1], 1.0]];
        let mut pn = mat_mul(mat_mul(ikz, p), transpose(ikz));
        pn[0][0] += kg[0] * kg[0] * r_eff;
        pn[0][1] += kg[0] * kg[1] * r_eff;
        pn[1][0] += kg[1] * kg[0] * r_eff;
        pn[1][1] += kg[1] * kg[1] * r_eff;
        p = pn;
        if w.n >= 2 && w.s2 > 0.0 {
            let yl = w.s2.ln() - log_var_correction(w.n);
            let rl = log_var_noise(w.n);
            let s_inn = pb + rl;
            let kb = pb / s_inn;
            hb += kb * (yl - hb);
            pb = (1.0 - kb) * pb;
            if pb < base.q_h {
                pb = base.q_h;
            }
        }
    }
    (loglik * weight, (z_sum, z2_sum, z_n, azs, az2, an))
}

fn pooled_loglikmr(sample: &[(&PersonSeries, f64)], h: &HyperMR, base: &Hyper) -> f64 {
    sample.par_iter().map(|(s, w)| runmr_eval(s, h, base, *w).0).sum()
}

fn fit_hypermr(sample: &[(&PersonSeries, f64)], base: &Hyper, init: HyperMR) -> HyperMR {
    let mut h = init;
    let mut best = pooled_loglikmr(sample, &h, base);
    let factors = [0.5, 0.7, 1.4, 2.0];
    for _pass in 0..3 {
        for which in 0..3 {
            for &f in &factors {
                let mut c = h;
                match which {
                    0 => c.k = (h.k * f).clamp(0.001, 0.2),
                    1 => c.q_l *= f,
                    _ => c.q_f *= f,
                }
                let ll = pooled_loglikmr(sample, &c, base);
                if ll > best {
                    best = ll;
                    h = c;
                }
            }
        }
    }
    h
}

fn fit_hyper3(sample: &[(&PersonSeries, f64)], base: &Hyper, init: Hyper3) -> Hyper3 {
    let mut h = init;
    let mut best = pooled_loglik3(sample, &h, base);
    let factors = [0.5, 0.7, 1.4, 2.0];
    for _pass in 0..3 {
        for which in 0..4 {
            for &f in &factors {
                let mut c = h;
                match which {
                    0 => c.qe *= f,
                    1 => c.qv *= f,
                    2 => c.qa *= f,
                    _ => c.phi_a = (1.0 - (1.0 - h.phi_a) * f).clamp(0.50, 0.999),
                }
                let ll = pooled_loglik3(sample, &c, base);
                if ll > best {
                    best = ll;
                    h = c;
                }
            }
        }
    }
    h
}

/// Smoothed output of the coupled filters for one person.
struct Smoothed {
    /// (wk, level, slope, level_var) at each active week (smoothed).
    nodes: Vec<(i32, f64, f64, f64)>,
    h_last: f64,        // smoothed log within-week variance at last week
    loglik_weighted: f64,
    n_weeks: usize,
    /// One-step innovation calibration accumulators (skip t=0 prior; Gaussian F).
    /// z = (ybar − predicted level)/√F. Calibrated ⇒ Var(z)=1, mean z=0.
    z_sum: f64,
    z2_sum: f64,
    z_n: u64,
    epi_frac_sum: f64, // Σ p00/F: epistemic (level) share of predictive variance
    /// Same z's split by career age (weeks since first comp): <8, 8–52, 52–156, >156.
    age_zsum: [f64; 4],
    age_z2: [f64; 4],
    age_n: [u64; 4],
}

/// Run Filter A + Filter B forward, then the RTS smoother for Filter A.
/// `weight` scales the returned one-step log-likelihood (for hyperparameter fit).
/// `excl` (if set) marks a week as *unobserved* (predict-only, no update) so the
/// smoothed estimate there is a leave-one-out skill from surrounding competitions.
fn run_person(s: &PersonSeries, h: &Hyper, weight: f64, excl: Option<i32>) -> Smoothed {
    let m = s.weeks.len();
    // Filter B state (scalar): log within-week variance.
    let mut hb = h.h0;
    let mut pb = 4.0; // diffuse prior on h
    let mut h_pred_hist = vec![h.h0; m]; // ĥ_{t|t-1} per week (one-step delayed)

    // Filter A storage for the smoother.
    let mut x_filt = vec![[0.0f64; 2]; m];
    let mut p_filt = vec![[[0.0f64; 2]; 2]; m];
    let mut x_pred = vec![[0.0f64; 2]; m];
    let mut p_pred = vec![[[0.0f64; 2]; 2]; m];
    let mut g_used = vec![[[1.0f64, 0.0], [0.0, 1.0]]; m];

    let mut x: V2 = [s.weeks[0].ybar, s.weeks[0].ybar]; // floor init = level
    let mut p: M2 = [[0.25, 0.0], [0.0, 0.5]]; // floor diffuse
    let mut loglik = 0.0;
    let (mut z_sum, mut z2_sum, mut z_n, mut epi_frac_sum) = (0.0f64, 0.0f64, 0u64, 0.0f64);
    let mut age_zsum = [0.0f64; 4];
    let mut age_z2 = [0.0f64; 4];
    let mut age_n = [0u64; 4];
    let wk0 = s.weeks[0].wk;

    for t in 0..m {
        let w = &s.weeks[t];
        // ── predict (Filter A) ──
        if t == 0 {
            x_pred[t] = x;
            p_pred[t] = p;
        } else {
            let gap = w.wk - s.weeks[t - 1].wk;
            let (g_eff, q_g) = gap_transition(h, gap);
            g_used[t] = g_eff;
            x = mat_vec(g_eff, x);
            p = mat_add(mat_mul(mat_mul(g_eff, p), transpose(g_eff)), q_g);
            x_pred[t] = x;
            p_pred[t] = p;
            // Filter B predict (one step; gap inflates with floor).
            pb += h.q_h.max(h.q_h) * gap.max(1) as f64;
        }
        h_pred_hist[t] = hb;

        let observe = excl != Some(w.wk);

        // ── measurement noise from Filter B (one-step delayed, saturated) ──
        let sigma2 = hb.exp().clamp(h.r_min, h.r_max);
        let r_t = sigma2 / (w.n as f64);

        // ── update (Filter A), robust Student-t (skipped for an excluded week) ──
        if observe {
            let innov = w.ybar - x[0]; // Z = [1,0]
            let f = p[0][0] + r_t;
            // Gaussian predictive log-lik for hyperparameter fitting.
            loglik += -0.5 * (f.ln() + innov * innov / f);
            // Innovation calibration (skip t=0: that's the diffuse prior, not a prediction).
            if t > 0 {
                let z = innov / f.sqrt();
                z_sum += z;
                z2_sum += z * z;
                z_n += 1;
                epi_frac_sum += p[0][0] / f;
                let age = w.wk - wk0;
                let b = if age < 8 { 0 } else if age < 52 { 1 } else if age < 156 { 2 } else { 3 };
                age_zsum[b] += z;
                age_z2[b] += z * z;
                age_n[b] += 1;
            }
            let lambda = (NU + 1.0) / (NU + innov * innov / f);
            let r_eff = r_t / lambda;
            let f_eff = p[0][0] + r_eff;
            let k: V2 = [p[0][0] / f_eff, p[1][0] / f_eff];
            x = [x[0] + k[0] * innov, x[1] + k[1] * innov];
            // Joseph form: (I-KZ) P (I-KZ)^T + K R K^T, Z=[1,0].
            let ikz: M2 = [[1.0 - k[0], 0.0], [-k[1], 1.0]];
            let mut p_new = mat_mul(mat_mul(ikz, p), transpose(ikz));
            p_new[0][0] += k[0] * k[0] * r_eff;
            p_new[0][1] += k[0] * k[1] * r_eff;
            p_new[1][0] += k[1] * k[0] * r_eff;
            p_new[1][1] += k[1] * k[1] * r_eff;
            p = p_new;
        }
        x_filt[t] = x;
        p_filt[t] = p;

        // ── Filter B update (only if observed and a within-week variance exists) ──
        if observe && w.n >= 2 && w.s2 > 0.0 {
            let yl = w.s2.ln() - log_var_correction(w.n);
            let rl = log_var_noise(w.n);
            let s_inn = pb + rl;
            let kb = pb / s_inn;
            hb += kb * (yl - hb);
            pb = (1.0 - kb) * pb;
            if pb < h.q_h {
                pb = h.q_h; // floor keeps the volatility filter responsive
            }
        }
    }

    // ── RTS smoother (Filter A) ──
    let mut xs = x_filt.clone();
    let mut ps = p_filt.clone();
    for t in (0..m - 1).rev() {
        let a = mat_mul(mat_mul(p_filt[t], transpose(g_used[t + 1])), inv2(p_pred[t + 1]));
        let dx = [xs[t + 1][0] - x_pred[t + 1][0], xs[t + 1][1] - x_pred[t + 1][1]];
        xs[t] = [x_filt[t][0] + a[0][0] * dx[0] + a[0][1] * dx[1],
                 x_filt[t][1] + a[1][0] * dx[0] + a[1][1] * dx[1]];
        let dp = mat_add(ps[t + 1], [[-p_pred[t + 1][0][0], -p_pred[t + 1][0][1]],
                                     [-p_pred[t + 1][1][0], -p_pred[t + 1][1][1]]]);
        ps[t] = mat_add(p_filt[t], mat_mul(mat_mul(a, dp), transpose(a)));
    }

    let nodes: Vec<(i32, f64, f64, f64)> = (0..m)
        .map(|t| (s.weeks[t].wk, xs[t][0], xs[t][1], ps[t][0][0].max(0.0)))
        .collect();

    Smoothed {
        nodes,
        h_last: hb,
        loglik_weighted: loglik * weight,
        n_weeks: m,
        z_sum,
        z2_sum,
        z_n,
        epi_frac_sum,
        age_zsum,
        age_z2,
        age_n,
    }
}

// ── output structs ──────────────────────────────────────────────────────────
#[derive(Serialize)]
struct RankEntry {
    rank: usize,
    pid: String,
    name: String,
    cid: String,
    est: f64,             // exp(level) projected to current week, centiseconds
    potential: f64,       // exp(floor) — estimated personal asymptote, centiseconds
    level_sd: f64,        // sd of level, log units
    velocity_pct_wk: f64, // (exp(velocity)-1)*100, %/week (negative = improving)
    cv: f64,              // per-solve coefficient of variation (≈ sqrt(exp(h)))
    dnf_rate: f64,
    n_weeks: usize,
    last_date: String,
    e_ao5: f64,           // expected ao5, centiseconds
    p_sub: HashMap<String, f64>,
    p_wr_single: f64,
    p_wr_ao5: f64,
}

#[derive(Serialize)]
struct TrackPoint {
    date: String,
    est: f64,
    lo: f64,
    hi: f64,
}
#[derive(Serialize)]
struct Track {
    pid: String,
    name: String,
    points: Vec<TrackPoint>,
}

#[derive(Serialize)]
struct Output {
    event: String,
    week_start: String,
    phi: f64,
    q_eta: f64,
    q_xi: f64,
    wr_single: i32,
    wr_ao5: i32,
    rankings: Vec<RankEntry>,
    tracks: Vec<Track>,
}

// ── simulation ──────────────────────────────────────────────────────────────
struct SimResult {
    e_ao5: f64,
    p_sub: Vec<f64>,
    p_wr_single: f64,
    p_wr_ao5: f64,
}

/// Precomputed per-competitor sim parameters (skew shape resolved once).
#[derive(Clone, Copy)]
struct SimParams {
    level: f64,
    lvl_sd: f64,
    sd: f64,
    dnf: f64,
    eps: f64,
    delta: f64,
    sas_m: f64,
    sas_scale: f64,
}

impl SimParams {
    fn new(level: f64, level_var: f64, sigma2: f64, dnf: f64) -> Self {
        let eps = SAS_EPS;
        let delta = sas_delta(level);
        let (sas_m, sas_v) = sas_consts(eps, delta);
        SimParams {
            level,
            lvl_sd: level_var.sqrt(),
            sd: sigma2.sqrt(),
            dnf,
            eps,
            delta,
            sas_m,
            sas_scale: 1.0 / sas_v.sqrt(),
        }
    }
}

/// One Monte-Carlo ao5 draw (centiseconds; +∞ = DNF average) for a competitor,
/// using the skew (sinh-arcsinh) emission. Draws the latent mean once, then 5 solves.
/// `shared` adds a per-scramble log offset common to all finalists (scramble luck).
fn sim_one_ao5(p: &SimParams, shared: &[f64; 5], rng: &mut Rng) -> f64 {
    let mean = p.level + p.lvl_sd * rng.normal();
    let mut v = [0.0f64; 5];
    let mut nd = 0;
    for (k, s) in v.iter_mut().enumerate() {
        if rng.unit() < p.dnf {
            *s = f64::INFINITY;
            nd += 1;
        } else {
            let w = (sas(rng.normal(), p.eps, p.delta) - p.sas_m) * p.sas_scale;
            *s = (mean + p.sd * w + shared[k]).exp();
        }
    }
    if nd >= 2 {
        return f64::INFINITY;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (v[1] + v[2] + v[3]) / 3.0
}

/// Monte-Carlo an ao5 distribution from the projected state.
fn simulate(level: f64, level_var: f64, sigma2: f64, dnf_p: f64, wr_single: i32, wr_ao5: i32,
            subs: &[i32], seed: u64) -> SimResult {
    let mut rng = Rng(seed | 1);
    let sd = sigma2.sqrt();
    let lvl_sd = level_var.sqrt();
    // sinh-arcsinh shape: skew + fat right tail, normalized to keep mean=level,
    // var=sigma2. delta tracks skill (lighter tails for faster solvers).
    let eps = SAS_EPS;
    let delta = sas_delta(level);
    let (sas_m, sas_v) = sas_consts(eps, delta);
    let sas_scale = 1.0 / sas_v.sqrt();
    let mut sum_ao5 = 0.0;
    let mut cnt_ao5 = 0u64;
    let mut sub = vec![0u64; subs.len()];
    let mut wr_s = 0u64;
    let mut wr_a = 0u64;
    for _ in 0..MC_SAMPLES {
        let mean = level + lvl_sd * rng.normal();
        let mut solves = [0.0f64; 5]; // centiseconds; f64::INFINITY = DNF
        let mut n_dnf = 0;
        let mut best = f64::INFINITY;
        for s in solves.iter_mut() {
            if rng.unit() < dnf_p {
                *s = f64::INFINITY;
                n_dnf += 1;
            } else {
                let w = (sas(rng.normal(), eps, delta) - sas_m) * sas_scale;
                let t = (mean + sd * w).exp();
                *s = t;
                if t < best {
                    best = t;
                }
            }
        }
        if best.is_finite() && (best as i32) < wr_single {
            wr_s += 1;
        }
        // WCA ao5: ≥2 DNF → DNF; else drop best+worst, mean middle 3.
        if n_dnf >= 2 {
            continue;
        }
        let mut v = solves;
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let ao5 = (v[1] + v[2] + v[3]) / 3.0; // drop v[0] (best) and v[4] (worst/DNF)
        sum_ao5 += ao5;
        cnt_ao5 += 1;
        for (i, &thr) in subs.iter().enumerate() {
            if ao5 < thr as f64 {
                sub[i] += 1;
            }
        }
        if (ao5 as i32) < wr_ao5 {
            wr_a += 1;
        }
    }
    let denom = MC_SAMPLES as f64;
    SimResult {
        e_ao5: if cnt_ao5 > 0 { sum_ao5 / cnt_ao5 as f64 } else { f64::NAN },
        p_sub: sub.iter().map(|&c| c as f64 / denom).collect(),
        p_wr_single: wr_s as f64 / denom,
        p_wr_ao5: wr_a as f64 / denom,
    }
}

// ── hyperparameter fit ──────────────────────────────────────────────────────
/// Speed-weighted pooled one-step log-likelihood over the subsample.
fn pooled_loglik(sample: &[(&PersonSeries, f64)], h: &Hyper) -> f64 {
    sample
        .par_iter()
        .map(|(s, w)| run_person(s, h, *w, None).loglik_weighted)
        .sum()
}

fn fit_hyper(sample: &[(&PersonSeries, f64)], base: Hyper) -> Hyper {
    let mut h = base;
    let mut best = pooled_loglik(sample, &h);
    // Coordinate ascent over multiplicative grids for q_ℓ (q_eta), q_f (q_xi),
    // and the mean-reversion rate k (phi).
    let factors = [0.5, 0.7, 1.4, 2.0];
    for _pass in 0..3 {
        for which in 0..3 {
            for &f in &factors {
                let mut cand = h;
                match which {
                    0 => cand.q_eta *= f,
                    1 => cand.q_xi *= f,
                    _ => cand.phi = (h.phi * f).clamp(0.001, 0.2),
                }
                let ll = pooled_loglik(sample, &cand);
                if ll > best {
                    best = ll;
                    h = cand;
                }
            }
        }
    }
    h
}

/// Pooled one-step innovation mean z̄ over the sample, restricted to career age
/// ≥ 8 weeks (the diffuse-prior weeks <8 are not meaningful predictions).
fn pooled_zbar(sample: &[(&PersonSeries, f64)], h: &Hyper) -> f64 {
    let (zs, zn) = sample
        .par_iter()
        .map(|(s, _)| {
            let sm = run_person(s, h, 1.0, None);
            (
                sm.age_zsum[1] + sm.age_zsum[2] + sm.age_zsum[3],
                sm.age_n[1] + sm.age_n[2] + sm.age_n[3],
            )
        })
        .reduce(|| (0.0, 0u64), |a, b| (a.0 + b.0, a.1 + b.1));
    if zn == 0 { 0.0 } else { zs / zn as f64 }
}

/// Stage 1 (mean): calibrate the trend-damping φ so the pooled lag z̄ ≈ 0.
/// z̄ increases monotonically with φ (more slope projected ⇒ less under-prediction),
/// so bisect. Clamps to [0.80, 0.995]; if even φ=0.995 still lags, returns the cap.
fn calibrate_phi(sample: &[(&PersonSeries, f64)], h: &Hyper) -> f64 {
    let (mut lo, mut hi) = (0.80f64, 0.995f64);
    for _ in 0..20 {
        let mid = 0.5 * (lo + hi);
        let mut hm = *h;
        hm.phi = mid;
        if pooled_zbar(sample, &hm) < 0.0 {
            lo = mid; // still under-predicting ⇒ need more persistence
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

// ── main entry ──────────────────────────────────────────────────────────────
pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    // Competition start/end JDN.
    let comp_start: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();
    let comp_end: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.end_year, c.end_month, c.end_day)))
        .collect();

    // ── Step 1: week-boundary probe — weekday minimizing straddled competitions ──
    let mut straddle = [0u64; 7];
    for (id, &s) in &comp_start {
        let e = *comp_end.get(id).unwrap_or(&s);
        for b in 0..7 {
            // Boundary at weekday b: weeks start when weekday == b.
            let ws = (s - b as i32).div_euclid(7);
            let we = (e - b as i32).div_euclid(7);
            if ws != we {
                straddle[b] += 1;
            }
        }
    }
    let boundary = (0..7).min_by_key(|&b| straddle[b]).unwrap();
    eprintln!(
        "  kalman_skill: week boundary = {} (straddled comps: {:?})",
        WEEKDAYS[weekday(boundary as i32)], straddle
    );
    // week index for a competition (use its start date).
    let week_of = |jdn: i32| (jdn - boundary as i32).div_euclid(7);

    // Per-event outputs, each with its own hyperparameters (BTreeMap = stable order).
    let mut outputs: std::collections::BTreeMap<String, Output> = std::collections::BTreeMap::new();
    let mut lucks: std::collections::BTreeMap<String, Vec<LuckEntry>> =
        std::collections::BTreeMap::new();

    for &event in EVENTS {
        eprintln!("  kalman_skill: ───── {event} ─────");

    // ── Step 2: per-person weekly sufficient stats for this event ──
    // (pid, week) -> running log-time stats + dnf/attempt counts.
    struct Acc {
        n: usize,
        sum: f64,
        sumsq: f64,
    }
    let mut pw: HashMap<(&str, i32), Acc> = HashMap::new();
    let mut dnf_cnt: HashMap<&str, (u32, u32)> = HashMap::new(); // pid -> (dnf, attempts)
    let mut wr_single = i32::MAX;
    let mut wr_ao5 = i32::MAX;

    for r in &db.results {
        if r.event_id != event {
            continue;
        }
        if r.best > 0 && r.best < wr_single {
            wr_single = r.best;
        }
        if r.average > 0 && r.average < wr_ao5 {
            wr_ao5 = r.average;
        }
        let Some(&jdn) = comp_start.get(r.competition_id.as_str()) else {
            continue;
        };
        let wk = week_of(jdn);
        let entry = dnf_cnt.entry(r.person_id.as_str()).or_insert((0, 0));
        if let Some(times) = db.attempts.get(&r.id) {
            let acc = pw.entry((r.person_id.as_str(), wk)).or_insert(Acc { n: 0, sum: 0.0, sumsq: 0.0 });
            for &v in times {
                if v > 0 {
                    let y = (v as f64).ln();
                    acc.n += 1;
                    acc.sum += y;
                    acc.sumsq += y * y;
                    entry.1 += 1;
                } else if v == -1 {
                    entry.0 += 1; // DNF
                    entry.1 += 1;
                }
                // v == -2 (DNS) ignored entirely.
            }
        } else if r.average > 0 {
            // Fallback when attempt rows are missing: one pseudo-solve.
            let acc = pw.entry((r.person_id.as_str(), wk)).or_insert(Acc { n: 0, sum: 0.0, sumsq: 0.0 });
            let y = (r.average as f64).ln();
            acc.n += 1;
            acc.sum += y;
            acc.sumsq += y * y;
            entry.1 += 1;
        }
    }

    // Assemble PersonSeries.
    let mut by_person: HashMap<&str, Vec<WeekObs>> = HashMap::new();
    for ((pid, wk), acc) in pw {
        if acc.n == 0 {
            continue;
        }
        let ybar = acc.sum / acc.n as f64;
        let s2 = if acc.n >= 2 {
            ((acc.sumsq - acc.n as f64 * ybar * ybar) / (acc.n as f64 - 1.0)).max(0.0)
        } else {
            0.0
        };
        by_person.entry(pid).or_default().push(WeekObs { wk, n: acc.n, ybar, s2 });
    }

    let mut series: Vec<PersonSeries> = by_person
        .into_iter()
        .filter(|(_, w)| !w.is_empty())
        .map(|(pid, mut weeks)| {
            weeks.sort_unstable_by_key(|w| w.wk);
            let (name, country) = db
                .persons
                .get(pid)
                .map(|p| (p.name.clone(), p.country_id.clone()))
                .unwrap_or_else(|| (pid.to_string(), String::new()));
            let (dnf, attempts) = dnf_cnt.get(pid).copied().unwrap_or((0, 0));
            PersonSeries { pid: pid.to_string(), name, country, weeks, dnf, attempts }
        })
        .collect();
    // Deterministic order (HashMap iteration is randomized) so the hyperparameter
    // subsample, rankings, and luck list are reproducible run to run.
    series.sort_unstable_by(|a, b| a.pid.cmp(&b.pid));
    if series.len() < 50 {
        eprintln!("  kalman_skill: {event}: only {} people — skipping", series.len());
        continue;
    }
    eprintln!("  kalman_skill: {event}: {} people with weekly series", series.len());

    // Global prior for Filter B: mean of bias-corrected log sample variances.
    let mut hsum = 0.0;
    let mut hn = 0u64;
    for s in &series {
        for w in &s.weeks {
            if w.n >= 2 && w.s2 > 0.0 {
                hsum += w.s2.ln() - log_var_correction(w.n);
                hn += 1;
            }
        }
    }
    let h0 = if hn > 0 { hsum / hn as f64 } else { (0.08f64).ln() };
    let global_cv2 = h0.exp(); // ≈ variance of log-time per solve

    // ── Step 3: hyperparameter fit on a speed-weighted subsample ──
    let base = Hyper {
        q_eta: 0.00015, // q_ℓ: level process variance
        q_xi: 0.00018,  // q_f: floor drift variance
        phi: 0.0265,    // k: mean-reversion rate (≈6-month gap half-life)
        q_h: 0.02,
        h0,
        r_min: global_cv2 * 0.02,
        r_max: global_cv2 * 50.0,
    };
    // Subsample: people with enough weeks; weight 1/x² (x = mean time, floored at 4s).
    let mut sample: Vec<(&PersonSeries, f64)> = series
        .iter()
        .filter(|s| s.weeks.len() >= 8)
        .map(|s| {
            let med = s.weeks[s.weeks.len() / 2].ybar.exp().max(400.0);
            (s, 1.0 / (med * med))
        })
        .collect();
    // Cap subsample size for speed (deterministic stride).
    if sample.len() > 6000 {
        let stride = sample.len() / 6000;
        sample = sample.into_iter().step_by(stride.max(1)).collect();
    }
    eprintln!("  kalman_skill: fitting hyperparameters on {} series", sample.len());
    let hyper = fit_hyper(&sample, base);
    eprintln!(
        "  kalman_skill: decay-to-floor k={:.4}/wk (gap half-life {:.0} wk), q_ℓ={:.5}/wk, q_f={:.5}/wk",
        hyper.phi,
        std::f64::consts::LN_2 / hyper.phi.max(1e-6),
        hyper.q_eta.sqrt(),
        hyper.q_xi.sqrt(),
    );

    // ── Is per-person specificity warranted? (333) Held-out forward-prediction
    //    test at ~9-month horizon: A=no change, B=global rate-by-level curve,
    //    C=per-person floor projection. Compare by out-of-sample RMSE. ──
    if event == "333" {
        let k = hyper.phi;
        const H: f64 = 39.0; // horizon weeks (center of [+26,+52])
        const NB: usize = 14;
        let (lo, hi) = (450.0_f64.ln(), 4000.0_f64.ln());
        let lbin = |l: f64| (((l - lo) / (hi - lo) * NB as f64) as i64).clamp(0, NB as i64 - 1) as usize;
        // Anchors: (filtered level, floor, actual future level).
        let anchors: Vec<(f64, f64, f64)> = series
            .par_iter()
            .flat_map(|s| {
                let lf = forward_lf(s, &hyper);
                let mut out = Vec::new();
                for (i, &(wk, level, floor)) in lf.iter().enumerate() {
                    let (mut fs, mut fn_) = (0.0f64, 0usize);
                    for w in &s.weeks[i + 1..] {
                        if w.wk <= wk + 26 { continue; }
                        if w.wk > wk + 52 { break; }
                        fs += w.ybar * w.n as f64;
                        fn_ += w.n;
                    }
                    if fn_ >= 10 {
                        out.push((level, floor, fs / fn_ as f64));
                    }
                }
                out
            })
            .collect();
        // Global rate-by-level curve μ_v (mean actual future rate per level bin).
        let mut rs = vec![0.0f64; NB];
        let mut rn = vec![0u64; NB];
        for &(level, _, fut) in &anchors {
            let b = lbin(level);
            rs[b] += (fut - level) / H;
            rn[b] += 1;
        }
        let muv: Vec<f64> = (0..NB).map(|b| if rn[b] > 0 { rs[b] / rn[b] as f64 } else { 0.0 }).collect();
        // Score A/B/C; report RMSE overall and for fast (level < ln 1000).
        let mut e = [[0.0f64; 3]; 2]; // [all, fast] × [A,B,C] sum sq err
        let mut n = [0u64; 2];
        for &(level, floor, fut) in &anchors {
            let a = level;
            let b = level + muv[lbin(level)] * H;
            let c = floor + (level - floor) * (1.0 - k).powi(H as i32);
            let (ea, eb, ec) = ((a - fut).powi(2), (b - fut).powi(2), (c - fut).powi(2));
            e[0][0] += ea; e[0][1] += eb; e[0][2] += ec; n[0] += 1;
            if level < 1000.0_f64.ln() {
                e[1][0] += ea; e[1][1] += eb; e[1][2] += ec; n[1] += 1;
            }
        }
        let rmse = |ss: f64, cnt: u64| (ss / cnt.max(1) as f64).sqrt();
        eprintln!("  kalman_skill: FLOOR-VALUE TEST (held-out {:.0}wk, n={}):", H, n[0]);
        eprintln!(
            "    RMSE (log) — all:  A(no-change)={:.4}  B(global curve)={:.4}  C(per-person floor)={:.4}",
            rmse(e[0][0], n[0]), rmse(e[0][1], n[0]), rmse(e[0][2], n[0])
        );
        eprintln!(
            "    RMSE (log) — fast: A={:.4}  B={:.4}  C={:.4}  (n={})",
            rmse(e[1][0], n[1]), rmse(e[1][1], n[1]), rmse(e[1][2], n[1]), n[1]
        );
    }

    // ── Step 4: smooth everyone, project to current week, simulate top-N ──
    let target_wk = series.iter().flat_map(|s| s.weeks.last()).map(|w| w.wk).max().unwrap_or(0);

    struct Computed {
        idx: usize,
        est: f64,
        level: f64,
        level_var: f64,
        floor: f64,    // estimated personal floor (potential), log-cs
        velocity: f64, // derived weekly improvement (log/wk, negative = faster)
        h_last: f64,
        dnf_rate: f64,
        last_wk: i32,
    }
    let computed: Vec<Computed> = series
        .par_iter()
        .enumerate()
        .map(|(idx, s)| {
            let sm = run_person(s, &hyper, 1.0, None);
            let (last_wk, level, floor, lvar) = *sm.nodes.last().unwrap();
            let k = hyper.phi;
            // Derived velocity = mean-reversion toward floor: −k·(level − floor).
            let velocity = -k * (level - floor);
            // Project to the current week. Cap the improvement horizon (stale
            // solvers aren't assumed to keep training toward their floor) but let
            // variance grow over the full gap. ℓ' = floor + (ℓ−floor)·(1−k)^g.
            let g_full = (target_wk - last_wk).max(0);
            let g_proj = g_full.min(26);
            let level_p = floor + (level - floor) * (1.0 - k).powi(g_proj);
            let lvar_p = if g_full == 0 {
                lvar
            } else {
                let (g_eff, q_g) = gap_transition(&hyper, g_full);
                mat_add(
                    mat_mul(mat_mul(g_eff, [[lvar, 0.0], [0.0, lvar]]), transpose(g_eff)),
                    q_g,
                )[0][0]
            };
            // DNF rate shrunk toward global (pseudo-count 50 attempts).
            let global_dnf = 0.02;
            let dnf_rate = (s.dnf as f64 + 50.0 * global_dnf) / (s.attempts as f64 + 50.0);
            Computed {
                idx,
                est: level_p.exp(),
                level: level_p,
                level_var: lvar_p,
                floor,
                velocity,
                h_last: sm.h_last,
                dnf_rate,
                last_wk,
            }
        })
        .collect();

    // Rank by estimate ascending. Require enough data to be ranked, else a
    // 2-week newcomer with a diffuse prior can rocket to the top.
    let mut order: Vec<&Computed> = computed
        .iter()
        .filter(|c| {
            let s = &series[c.idx];
            s.weeks.len() >= 5 && s.attempts >= 25
        })
        .collect();
    order.sort_unstable_by(|a, b| a.est.partial_cmp(&b.est).unwrap());

    let n_rank = order.len().min(TOP_N);
    let ranked = &order[..n_rank];

    // Simulate top-N in parallel.
    let sims: Vec<SimResult> = ranked
        .par_iter()
        .enumerate()
        .map(|(i, c)| {
            let sigma2 = c.h_last.exp().clamp(hyper.r_min, hyper.r_max);
            simulate(
                c.level,
                c.level_var.min(0.09), // cap level uncertainty (sd ≤ 0.3 log) for stable sims
                sigma2,
                c.dnf_rate,
                wr_single,
                wr_ao5,
                subs_for(event),
                0xC0FFEE ^ (i as u64).wrapping_mul(0x9E3779B1),
            )
        })
        .collect();

    let rankings: Vec<RankEntry> = ranked
        .iter()
        .zip(sims.iter())
        .enumerate()
        .map(|(i, (c, sim))| {
            let s = &series[c.idx];
            let p_sub: HashMap<String, f64> = subs_for(event)
                .iter()
                .zip(sim.p_sub.iter())
                .map(|(&thr, &p)| (format!("{}", thr / 100), p))
                .collect();
            RankEntry {
                rank: i + 1,
                pid: s.pid.clone(),
                name: s.name.clone(),
                cid: s.country.clone(),
                est: c.est,
                potential: c.floor.exp(),
                level_sd: c.level_var.sqrt(),
                velocity_pct_wk: (c.velocity.exp() - 1.0) * 100.0,
                cv: c.h_last.exp().sqrt(),
                dnf_rate: c.dnf_rate,
                n_weeks: s.weeks.len(),
                last_date: jdn_to_iso(c.last_wk * 7 + boundary as i32),
                e_ao5: sim.e_ao5,
                p_sub,
                p_wr_single: sim.p_wr_single,
                p_wr_ao5: sim.p_wr_ao5,
            }
        })
        .collect();

    // Tracks for the top-K (smoothed level ± 2σ over time).
    let tracks: Vec<Track> = ranked
        .iter()
        .take(N_TRACKS)
        .map(|c| {
            let s = &series[c.idx];
            let sm = run_person(s, &hyper, 1.0, None);
            let points = sm
                .nodes
                .iter()
                .map(|&(wk, level, _slope, lvar)| {
                    let sd = lvar.sqrt();
                    TrackPoint {
                        date: jdn_to_iso(wk * 7 + boundary as i32),
                        est: level.exp(),
                        lo: (level - 2.0 * sd).exp(),
                        hi: (level + 2.0 * sd).exp(),
                    }
                })
                .collect();
            Track { pid: s.pid.clone(), name: s.name.clone(), points }
        })
        .collect();

    // ── Diagnostics + validation (only for 3x3, to keep the run light) ──
    if event == "333" {
        diagnostics(&series, &hyper);
        validate(db, event, &series, &hyper, &comp_start, boundary as i32, wr_single, wr_ao5);
    }

    // ── Luck stat: top-100 ao5 leaderboard with career luck ──
    let luck = build_luck(db, event, &series, &hyper, &comp_start, boundary as i32, event == "333");

    eprintln!("  kalman_skill: {event}: {} ranked, {} tracks, {} luck", rankings.len(), tracks.len(), luck.len());
    outputs.insert(
        event.to_string(),
        Output {
            event: event.to_string(),
            week_start: WEEKDAYS[weekday(boundary as i32)].to_string(),
            phi: hyper.phi,
            q_eta: hyper.q_eta,
            q_xi: hyper.q_xi,
            wr_single,
            wr_ao5,
            rankings,
            tracks,
        },
    );
    lucks.insert(event.to_string(), luck);
    } // end per-event loop

    serde_json::to_writer(
        std::fs::File::create(format!("{out_dir}/kalman_skill.json"))?,
        &outputs,
    )?;
    serde_json::to_writer(std::fs::File::create(format!("{out_dir}/luck.json"))?, &lucks)?;
    eprintln!("  kalman_skill: wrote {} events", outputs.len());
    Ok(())
}

/// One-step predictive relative error by career age — comparable to the EWMA
/// bias analysis in `skill_estimator.rs`. Uses the *forward* filter prediction.
fn diagnostics(series: &[PersonSeries], h: &Hyper) {
    let breaks: &[(i32, &str)] = &[
        (13, "<3 mo"),
        (26, "3–6 mo"),
        (52, "6–12 mo"),
        (104, "1–2 yr"),
        (260, "2–5 yr"),
        (i32::MAX, "5+ yr"),
    ];
    // (career_weeks, relative_error)
    let preds: Vec<(i32, f64)> = series
        .par_iter()
        .filter(|s| s.weeks.len() >= 2)
        .flat_map_iter(|s| {
            let mut out = Vec::new();
            let first = s.weeks[0].wk;
            // Re-run forward filter, recording one-step predictions on level.
            let mut x: V2 = [s.weeks[0].ybar, s.weeks[0].ybar]; // floor init = level
            let mut p: M2 = [[0.25, 0.0], [0.0, 0.5]]; // floor diffuse
            for t in 1..s.weeks.len() {
                let gap = s.weeks[t].wk - s.weeks[t - 1].wk;
                let (g_eff, q_g) = gap_transition(h, gap);
                x = mat_vec(g_eff, x);
                p = mat_add(mat_mul(mat_mul(g_eff, p), transpose(g_eff)), q_g);
                let pred = x[0];
                let actual = s.weeks[t].ybar;
                // relative error in real time: exp(actual-pred)-1
                out.push((s.weeks[t].wk - first, (actual - pred).exp() - 1.0));
                // crude update (Gaussian, fixed R) just to advance the state.
                let r = 0.02_f64;
                let f = p[0][0] + r;
                let innov = actual - x[0];
                let k: V2 = [p[0][0] / f, p[1][0] / f];
                x = [x[0] + k[0] * innov, x[1] + k[1] * innov];
                let ikz: M2 = [[1.0 - k[0], 0.0], [-k[1], 1.0]];
                p = mat_mul(mat_mul(ikz, p), transpose(ikz));
            }
            out
        })
        .collect();

    eprintln!("    one-step bias by career age (n={}):", preds.len());
    let mut lo = 0i32;
    for &(hi, label) in breaks {
        let slice: Vec<f64> = preds.iter().filter(|p| p.0 >= lo && p.0 < hi).map(|p| p.1).collect();
        if !slice.is_empty() {
            let mean = slice.iter().sum::<f64>() / slice.len() as f64;
            eprintln!("      {label:8} (n={:7}): bias = {:+.2}%", slice.len(), mean * 100.0);
        }
        lo = hi;
    }
}

// ── validation ──────────────────────────────────────────────────────────────
/// Per-active-week one-step predictions entering week t (t ≥ 1):
/// (wk, predicted level, level variance, per-solve variance σ²).
fn forward_levels(s: &PersonSeries, h: &Hyper) -> Vec<(i32, f64, f64, f64)> {
    let m = s.weeks.len();
    let mut out = Vec::with_capacity(m.saturating_sub(1));
    let mut hb = h.h0;
    let mut pb = 4.0;
    let mut x: V2 = [s.weeks[0].ybar, s.weeks[0].ybar]; // floor init = level
    let mut p: M2 = [[0.25, 0.0], [0.0, 0.5]]; // floor diffuse
    for t in 0..m {
        let w = &s.weeks[t];
        if t > 0 {
            let gap = w.wk - s.weeks[t - 1].wk;
            let (g_eff, q_g) = gap_transition(h, gap);
            x = mat_vec(g_eff, x);
            p = mat_add(mat_mul(mat_mul(g_eff, p), transpose(g_eff)), q_g);
            pb += h.q_h * gap.max(1) as f64;
            let sigma2 = hb.exp().clamp(h.r_min, h.r_max);
            out.push((w.wk, x[0], p[0][0], sigma2)); // entering week t
        }
        // Filter A update (robust), to advance the state.
        let sigma2 = hb.exp().clamp(h.r_min, h.r_max);
        let r_t = sigma2 / (w.n as f64);
        let innov = w.ybar - x[0];
        let f = p[0][0] + r_t;
        let lambda = (NU + 1.0) / (NU + innov * innov / f);
        let r_eff = r_t / lambda;
        let f_eff = p[0][0] + r_eff;
        let k: V2 = [p[0][0] / f_eff, p[1][0] / f_eff];
        x = [x[0] + k[0] * innov, x[1] + k[1] * innov];
        let ikz: M2 = [[1.0 - k[0], 0.0], [-k[1], 1.0]];
        let mut pn = mat_mul(mat_mul(ikz, p), transpose(ikz));
        pn[0][0] += k[0] * k[0] * r_eff;
        pn[0][1] += k[0] * k[1] * r_eff;
        pn[1][0] += k[1] * k[0] * r_eff;
        pn[1][1] += k[1] * k[1] * r_eff;
        p = pn;
        if w.n >= 2 && w.s2 > 0.0 {
            let yl = w.s2.ln() - log_var_correction(w.n);
            let rl = log_var_noise(w.n);
            let kb = pb / (pb + rl);
            hb += kb * (yl - hb);
            pb = ((1.0 - kb) * pb).max(h.q_h);
        }
    }
    out
}

/// Causal forward filter returning the post-update filtered (wk, level, floor)
/// at each observed week — for held-out forward-prediction tests.
fn forward_lf(s: &PersonSeries, h: &Hyper) -> Vec<(i32, f64, f64)> {
    let m = s.weeks.len();
    let mut out = Vec::with_capacity(m);
    let mut hb = h.h0;
    let mut pb = 4.0;
    let mut x: V2 = [s.weeks[0].ybar, s.weeks[0].ybar];
    let mut p: M2 = [[0.25, 0.0], [0.0, 0.5]];
    for t in 0..m {
        let w = &s.weeks[t];
        if t > 0 {
            let gap = w.wk - s.weeks[t - 1].wk;
            let (g_eff, q_g) = gap_transition(h, gap);
            x = mat_vec(g_eff, x);
            p = mat_add(mat_mul(mat_mul(g_eff, p), transpose(g_eff)), q_g);
            pb += h.q_h * gap.max(1) as f64;
        }
        let sigma2 = hb.exp().clamp(h.r_min, h.r_max);
        let r_t = sigma2 / (w.n as f64);
        let innov = w.ybar - x[0];
        let f = p[0][0] + r_t;
        let lambda = (NU + 1.0) / (NU + innov * innov / f);
        let r_eff = r_t / lambda;
        let f_eff = p[0][0] + r_eff;
        let kg: V2 = [p[0][0] / f_eff, p[1][0] / f_eff];
        x = [x[0] + kg[0] * innov, x[1] + kg[1] * innov];
        let ikz: M2 = [[1.0 - kg[0], 0.0], [-kg[1], 1.0]];
        let mut pn = mat_mul(mat_mul(ikz, p), transpose(ikz));
        pn[0][0] += kg[0] * kg[0] * r_eff;
        pn[0][1] += kg[0] * kg[1] * r_eff;
        pn[1][0] += kg[1] * kg[0] * r_eff;
        pn[1][1] += kg[1] * kg[1] * r_eff;
        p = pn;
        if w.n >= 2 && w.s2 > 0.0 {
            let yl = w.s2.ln() - log_var_correction(w.n);
            let rl = log_var_noise(w.n);
            let kb = pb / (pb + rl);
            hb += kb * (yl - hb);
            pb = ((1.0 - kb) * pb).max(h.q_h);
        }
        out.push((w.wk, x[0], x[1]));
    }
    out
}

/// EWMA on the same weekly mean-log-time series, with ~14-day half-life — a
/// like-for-like stand-in for the existing scalar `skill_estimator`.
fn ewma_lite(s: &PersonSeries) -> Vec<(i32, f64)> {
    let mut out = Vec::with_capacity(s.weeks.len().saturating_sub(1));
    let mut mu = s.weeks[0].ybar;
    let mut w = 1.0_f64;
    let mut prev = s.weeks[0].wk;
    for t in 1..s.weeks.len() {
        let wk = s.weeks[t].wk;
        out.push((wk, mu));
        let dec = (-0.35 * (wk - prev) as f64).exp(); // 0.05/day · 7 days/wk
        let weff = w * dec;
        mu = (weff * mu + s.weeks[t].ybar) / (weff + 1.0);
        w = weff + 1.0;
        prev = wk;
    }
    out
}

fn look4(v: &[(i32, f64, f64, f64)], wk: i32) -> Option<&(i32, f64, f64, f64)> {
    let i = v.partition_point(|e| e.0 < wk);
    v.get(i).filter(|e| e.0 == wk)
}
fn look2(v: &[(i32, f64)], wk: i32) -> Option<f64> {
    let i = v.partition_point(|e| e.0 < wk);
    v.get(i).filter(|e| e.0 == wk).map(|e| e.1)
}

fn validate(
    db: &WcaDb,
    event: &str,
    series: &[PersonSeries],
    h: &Hyper,
    comp_start: &HashMap<&str, i32>,
    boundary: i32,
    wr_single: i32,
    wr_ao5: i32,
) {
    let week_of = |jdn: i32| (jdn - boundary).div_euclid(7);
    let pid2idx: HashMap<&str, usize> =
        series.iter().enumerate().map(|(i, s)| (s.pid.as_str(), i)).collect();

    // Causal predictions per person.
    let kal: Vec<Vec<(i32, f64, f64, f64)>> =
        series.par_iter().map(|s| forward_levels(s, h)).collect();
    let ewma: Vec<Vec<(i32, f64)>> = series.par_iter().map(ewma_lite).collect();

    // Per-person DNF rate (shrunk toward 2%).
    let dnf_rate: Vec<f64> = series
        .iter()
        .map(|s| (s.dnf as f64 + 1.0) / (s.attempts as f64 + 50.0))
        .collect();

    // PB (best prior official 333 average) per (pid, wk).
    let mut by_pw: HashMap<(&str, i32), f64> = HashMap::new();
    for r in &db.results {
        if r.event_id != event || r.average <= 0 {
            continue;
        }
        let Some(&jdn) = comp_start.get(r.competition_id.as_str()) else { continue };
        let wk = week_of(jdn);
        let e = by_pw.entry((r.person_id.as_str(), wk)).or_insert(f64::INFINITY);
        *e = e.min(r.average as f64);
    }
    let mut pb_rows: Vec<(&str, i32, f64)> = by_pw.iter().map(|(&(p, w), &a)| (p, w, a)).collect();
    pb_rows.sort_unstable_by(|a, b| a.0.cmp(b.0).then(a.1.cmp(&b.1)));
    // pid -> sorted (wk, pb_before_this_wk)
    let mut pb_before: HashMap<&str, Vec<(i32, f64)>> = HashMap::new();
    {
        let mut cur = "";
        let mut best = f64::INFINITY;
        for (p, w, a) in &pb_rows {
            if *p != cur {
                cur = p;
                best = f64::INFINITY;
            }
            pb_before.entry(p).or_default().push((*w, best)); // best BEFORE this comp
            best = best.min(*a);
        }
    }

    // Finals for 333.
    let final_ids: std::collections::HashSet<&str> = db
        .round_types
        .values()
        .filter(|rt| rt.is_final != 0)
        .map(|rt| rt.id.as_str())
        .collect();
    // comp -> finalists (pid, official avg)
    let mut finals: HashMap<&str, Vec<(&str, f64)>> = HashMap::new();
    for r in &db.results {
        if r.event_id != event || r.average <= 0 {
            continue;
        }
        if !final_ids.contains(r.round_type_id.as_str()) {
            continue;
        }
        finals
            .entry(r.competition_id.as_str())
            .or_default()
            .push((r.person_id.as_str(), r.average as f64));
    }

    // Tally: (hits, total) per method, over finals where all three pick. Plus head-to-head.
    let mut acc = [[0u32; 2]; 3]; // [kal, ewma, pb] x [hits, total]
    let mut acc_d = [[0u32; 2]; 3]; // deep-field subset (3rd place sub-7)
    let (mut kw, mut ew, mut n_ke) = (0u32, 0u32, 0u32); // kalman vs ewma disagreements
    let (mut kw2, mut pw, mut n_kp) = (0u32, 0u32, 0u32); // kalman vs pb

    for (cid, fl) in &finals {
        if fl.len() < 4 {
            continue;
        }
        let Some(&jdn) = comp_start.get(*cid) else { continue };
        let wk = week_of(jdn);
        let winner = fl.iter().min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap().0;

        let pick_kal = fl
            .iter()
            .filter_map(|(p, _)| {
                let idx = *pid2idx.get(p)?;
                look4(&kal[idx], wk).map(|e| (*p, e.1))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|x| x.0);
        let pick_ewma = fl
            .iter()
            .filter_map(|(p, _)| {
                let idx = *pid2idx.get(p)?;
                look2(&ewma[idx], wk).map(|v| (*p, v))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|x| x.0);
        let pick_pb = fl
            .iter()
            .filter_map(|(p, _)| {
                let v = pb_before.get(p)?;
                let i = v.partition_point(|e| e.0 < wk);
                let e = v.get(i).filter(|e| e.0 == wk)?;
                if e.1.is_finite() { Some((*p, e.1)) } else { None }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|x| x.0);

        let (Some(k), Some(e), Some(pbp)) = (pick_kal, pick_ewma, pick_pb) else { continue };
        // Deep field: 3rd-fastest official average at this final is sub-7.
        let mut avgs: Vec<f64> = fl.iter().map(|x| x.1).collect();
        avgs.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let is_deep = avgs.get(2).map_or(false, |&a| a < 700.0);
        for (mi, pick) in [k, e, pbp].into_iter().enumerate() {
            acc[mi][1] += 1;
            if pick == winner {
                acc[mi][0] += 1;
            }
            if is_deep {
                acc_d[mi][1] += 1;
                if pick == winner {
                    acc_d[mi][0] += 1;
                }
            }
        }
        if k != e {
            n_ke += 1;
            if k == winner {
                kw += 1;
            } else if e == winner {
                ew += 1;
            }
        }
        if k != pbp {
            n_kp += 1;
            if k == winner {
                kw2 += 1;
            } else if pbp == winner {
                pw += 1;
            }
        }
    }

    let rate = |a: [u32; 2]| if a[1] > 0 { 100.0 * a[0] as f64 / a[1] as f64 } else { 0.0 };
    eprintln!("    winner prediction (333 finals, n={}):", acc[0][1]);
    eprintln!("      Kalman {:.1}%  EWMA {:.1}%  PB {:.1}%", rate(acc[0]), rate(acc[1]), rate(acc[2]));
    eprintln!(
        "      deep finals (3rd place sub-7, n={}): Kalman {:.1}%  EWMA {:.1}%  PB {:.1}%",
        acc_d[0][1], rate(acc_d[0]), rate(acc_d[1]), rate(acc_d[2])
    );
    let sh = |a: u32, b: u32| if a + b > 0 { 100.0 * a as f64 / (a + b) as f64 } else { 50.0 };
    eprintln!(
        "      head-to-head on disagreements: Kalman {:.1}% vs EWMA {:.1}% (n={n_ke}); Kalman {:.1}% vs PB {:.1}% (n={n_kp})",
        sh(kw, ew), sh(ew, kw), sh(kw2, pw), sh(pw, kw2)
    );

    // ── MC competition calibration: win/podium reliability on strong finals ──
    // For each strong final (3rd-place sub-8), Monte-Carlo the whole field with the
    // skew emission, get each finalist's P(win) and P(podium), and check those
    // probabilities against realized outcomes in 10 prediction buckets (reliability
    // diagram) plus a Brier score. Tests whether the simulator's odds are honest.
    const R_MC: usize = 3000;
    // Reliability accumulators for one sim variant.
    struct Rel {
        win_pred: [f64; 10],
        win_act: [f64; 10],
        win_n: [u32; 10],
        pod_pred: [f64; 10],
        pod_act: [f64; 10],
        pod_n: [u32; 10],
        brier_w: f64,
        brier_p: f64,
        n_pred: u64,
        n_finals: u64,
    }
    // Run the win/podium MC over strong finals under (lvl_scale, sd_scale, var_scr):
    // lvl_scale scales the skill-posterior sd, sd_scale the per-solve sd, var_scr is
    // the shared per-scramble log-variance (0 = iid scrambles).
    let run_variant = |lvl_scale: f64, sd_scale: f64, var_scr: f64| -> Rel {
        let scr_sd = var_scr.sqrt();
        let per_final: Vec<Vec<(f64, bool, f64, bool)>> = finals
            .par_iter()
            .filter_map(|(cid, fl)| {
                let &jdn = comp_start.get(*cid)?;
                let wk = week_of(jdn);
                let mut avgs: Vec<f64> = fl.iter().map(|x| x.1).collect();
                avgs.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
                if avgs.get(2).map_or(true, |&a| a >= 800.0) {
                    return None;
                }
                let cands: Vec<(f64, SimParams)> = fl
                    .iter()
                    .filter_map(|(p, avg)| {
                        let idx = *pid2idx.get(p)?;
                        let e = look4(&kal[idx], wk)?;
                        let mut sp = SimParams::new(e.1, e.2, e.3, dnf_rate[idx]);
                        sp.lvl_sd *= lvl_scale;
                        sp.sd *= sd_scale;
                        Some((*avg, sp))
                    })
                    .collect();
                let m = cands.len();
                if m < 4 {
                    return None;
                }
                let mut order: Vec<usize> = (0..m).collect();
                order.sort_unstable_by(|&a, &b| cands[a].0.partial_cmp(&cands[b].0).unwrap());
                let actual_win = order[0];
                let actual_pod: std::collections::HashSet<usize> =
                    order.iter().take(3).copied().collect();

                let mut wins = vec![0u32; m];
                let mut pods = vec![0u32; m];
                let mut rng = Rng((wk as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ cid.len() as u64 ^ 0x55);
                let mut sample = vec![0.0f64; m];
                for _ in 0..R_MC {
                    // Shared per-scramble log offsets (same 5 scrambles for everyone).
                    let scr: [f64; 5] = if var_scr > 0.0 {
                        std::array::from_fn(|_| scr_sd * rng.normal())
                    } else {
                        [0.0; 5]
                    };
                    for (i, c) in cands.iter().enumerate() {
                        sample[i] = sim_one_ao5(&c.1, &scr, &mut rng);
                    }
                    let mut idx: Vec<usize> = (0..m).collect();
                    idx.sort_unstable_by(|&a, &b| sample[a].partial_cmp(&sample[b]).unwrap());
                    wins[idx[0]] += 1;
                    for &k in idx.iter().take(3) {
                        pods[k] += 1;
                    }
                }
                Some(
                    (0..m)
                        .map(|i| {
                            (
                                wins[i] as f64 / R_MC as f64,
                                i == actual_win,
                                pods[i] as f64 / R_MC as f64,
                                actual_pod.contains(&i),
                            )
                        })
                        .collect(),
                )
            })
            .collect();

        let mut r = Rel {
            win_pred: [0.0; 10], win_act: [0.0; 10], win_n: [0; 10],
            pod_pred: [0.0; 10], pod_act: [0.0; 10], pod_n: [0; 10],
            brier_w: 0.0, brier_p: 0.0, n_pred: 0, n_finals: 0,
        };
        for f in &per_final {
            r.n_finals += 1;
            for &(pw, won, pp, pod) in f {
                let bw = ((pw * 10.0) as usize).min(9);
                r.win_pred[bw] += pw;
                r.win_act[bw] += won as u32 as f64;
                r.win_n[bw] += 1;
                let bp = ((pp * 10.0) as usize).min(9);
                r.pod_pred[bp] += pp;
                r.pod_act[bp] += pod as u32 as f64;
                r.pod_n[bp] += 1;
                r.brier_w += (pw - won as u32 as f64).powi(2);
                r.brier_p += (pp - pod as u32 as f64).powi(2);
                r.n_pred += 1;
            }
        }
        r
    };

    let scr_var = 0.0009; // shared per-scramble log-variance from the ANOVA probe
    let variants: &[(&str, f64, f64, f64)] = &[
        ("baseline       ", 1.0, 1.0, 0.0),
        ("lvl_sd=0        ", 0.0, 1.0, 0.0),
        ("sigma x0.85     ", 1.0, 0.85, 0.0),
        ("+scramble corr  ", 1.0, 1.0, scr_var),
        ("lvl=0 sig x0.9  ", 0.0, 0.9, 0.0),
    ];
    let row = |label: &str, pred: &[f64; 10], act: &[f64; 10], cnt: &[u32; 10]| {
        let cells: Vec<String> = (0..10)
            .filter(|&b| cnt[b] > 0)
            .map(|b| format!("[{:.2}→{:.2} n{}]", pred[b] / cnt[b] as f64, act[b] / cnt[b] as f64, cnt[b]))
            .collect();
        eprintln!("        {label} (pred→actual): {}", cells.join(" "));
    };
    for &(label, ls, ss, vs) in variants {
        let r = run_variant(ls, ss, vs);
        if r.n_pred == 0 {
            continue;
        }
        eprintln!(
            "    MC competition calibration [{}] (strong finals n={}, finalists n={}, R={R_MC}):  Brier win {:.4} podium {:.4}",
            label.trim(), r.n_finals, r.n_pred, r.brier_w / r.n_pred as f64, r.brier_p / r.n_pred as f64
        );
        row("win   ", &r.win_pred, &r.win_act, &r.win_n);
        row("podium", &r.pod_pred, &r.pod_act, &r.pod_n);
    }

    // ── PIT calibration on a subsample of 333 ao5 results ──
    // For each result with an official average, locate the causal prediction at
    // its week, Monte-Carlo the predicted ao5, and record the actual's percentile.
    let mut samples: Vec<(usize, i32, f64)> = Vec::new(); // (person idx, wk, actual_ao5)
    for r in &db.results {
        if r.event_id != event || r.average <= 0 {
            continue;
        }
        let Some(&jdn) = comp_start.get(r.competition_id.as_str()) else { continue };
        let Some(&idx) = pid2idx.get(r.person_id.as_str()) else { continue };
        samples.push((idx, week_of(jdn), r.average as f64));
    }
    let stride = (samples.len() / 60_000).max(1);
    let pit_in: Vec<(usize, i32, f64)> = samples.into_iter().step_by(stride).collect();
    let pits: Vec<f64> = pit_in
        .par_iter()
        .filter_map(|&(idx, wk, actual)| {
            let e = look4(&kal[idx], wk)?;
            let (_w, level, lvar, sig2) = *e;
            let sd = sig2.sqrt();
            let lsd = lvar.min(0.09).sqrt();
            let dnf = dnf_rate[idx];
            let mut rng = Rng((idx as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ wk as u64 ^ 7);
            let draws = 800usize;
            let mut below = 0u32;
            let mut valid = 0u32;
            for _ in 0..draws {
                let mean = level + lsd * rng.normal();
                let mut v = [0.0f64; 5];
                let mut ndnf = 0;
                for s in v.iter_mut() {
                    if rng.unit() < dnf {
                        *s = f64::INFINITY;
                        ndnf += 1;
                    } else {
                        *s = (mean + sd * rng.normal()).exp();
                    }
                }
                if ndnf >= 2 {
                    continue;
                }
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let ao5 = (v[1] + v[2] + v[3]) / 3.0;
                valid += 1;
                if ao5 < actual {
                    below += 1;
                }
            }
            if valid as usize >= draws / 2 {
                Some(below as f64 / valid as f64)
            } else {
                None
            }
        })
        .collect();

    let n = pits.len();
    if n > 0 {
        let mean = pits.iter().sum::<f64>() / n as f64;
        let mut deciles = [0u32; 10];
        for &p in &pits {
            deciles[((p * 10.0) as usize).min(9)] += 1;
        }
        eprintln!("    PIT calibration (ao5, n={n}): mean = {:.3} (ideal 0.500)", mean);
        let pct: Vec<String> =
            deciles.iter().map(|&c| format!("{:.1}", 100.0 * c as f64 / n as f64)).collect();
        eprintln!("      decile % (ideal ~10 each): [{}]", pct.join(", "));
    }
    // ── Innovation-variance calibration (model-internal, no outcome data) ──
    // z = (ybar − one-step predicted level)/√F. Honest variances ⇒ Var(z)=1.
    // Var(z)<1 ⇒ predictive F inflated by 1/Var(z) (posterior too wide).
    {
        let stride = (series.len() / 50_000).max(1);
        // (z_sum, z2, zn, epi, age_zsum[4], age_z2[4], age_n[4])
        type Acc = (f64, f64, u64, f64, [f64; 4], [f64; 4], [u64; 4]);
        let fold = |a: Acc, b: Acc| {
            let mut zs = a.4; let mut z2 = a.5; let mut n = a.6;
            for i in 0..4 { zs[i] += b.4[i]; z2[i] += b.5[i]; n[i] += b.6[i]; }
            (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3 + b.3, zs, z2, n)
        };
        let ages = ["<8wk", "8-52", "52-156", ">156"];
        let report = |label: &str, acc: Acc| {
            let (zs, z2, zn, epi, azs, az2, an) = acc;
            if zn == 0 {
                return;
            }
            let nf = zn as f64;
            let mean = zs / nf;
            let varz = z2 / nf - mean * mean;
            eprintln!(
                "    innovation calibration [{label}] (n={zn}): Var(z) = {:.3} (ideal 1.000)  mean z = {:+.3}  (sd ×{:.2}, epistemic {:.0}% of F)",
                varz, mean, 1.0 / varz.sqrt(), 100.0 * epi / nf
            );
            let cells: Vec<String> = (0..4)
                .filter(|&i| an[i] > 0)
                .map(|i| {
                    let n = an[i] as f64;
                    let m = azs[i] / n;
                    format!("{}: z̄={:+.2} Var={:.2} (n{})", ages[i], m, az2[i] / n - m * m, an[i])
                })
                .collect();
            eprintln!("      by career age — {}", cells.join("  "));
        };
        let zero: Acc = (0.0, 0.0, 0, 0.0, [0.0; 4], [0.0; 4], [0; 4]);
        let pull = |s: &PersonSeries| {
            let sm = run_person(s, h, 1.0, None);
            (sm.z_sum, sm.z2_sum, sm.z_n, sm.epi_frac_sum, sm.age_zsum, sm.age_z2, sm.age_n)
        };
        let acc_all: Acc = series.par_iter().step_by(stride).map(pull).reduce(|| zero, fold);
        report("all   ", acc_all);
        // Fast solvers only (best week sub-8), where prediction accuracy matters most.
        let acc_fast: Acc = series
            .par_iter()
            .step_by(stride)
            .filter(|s| s.weeks.iter().map(|w| w.ybar).fold(f64::INFINITY, f64::min) < 800.0_f64.ln())
            .map(pull)
            .reduce(|| zero, fold);
        report("sub-8 ", acc_fast);

        // Sweep φ and q_eta: which knob drives improve-phase z̄→0 and veteran Var(z)→1?
        let base = *h;
        let phis = [base.phi, 0.92, 0.96, 0.99];
        let qscales = [0.5, 1.0, 2.0];
        eprintln!("    hyperparameter sweep (improve = 8–156wk, vet = >156wk):");
        for &phi in &phis {
            for &qs in &qscales {
                let mut h2 = base;
                h2.phi = phi;
                h2.q_eta = base.q_eta * qs;
                let (_, _, _, _, azs, az2, an): Acc =
                    series.par_iter().step_by(stride).map(|s| {
                        let sm = run_person(s, &h2, 1.0, None);
                        (sm.z_sum, sm.z2_sum, sm.z_n, sm.epi_frac_sum, sm.age_zsum, sm.age_z2, sm.age_n)
                    }).reduce(|| zero, fold);
                let imp_n = (an[1] + an[2]).max(1) as f64;
                let imp_z = (azs[1] + azs[2]) / imp_n;
                let vn = an[3].max(1) as f64;
                let vz = azs[3] / vn;
                let vvar = az2[3] / vn - vz * vz;
                eprintln!(
                    "      φ={:.2} q_eta×{:.1}: improve z̄={:+.2} | vet z̄={:+.2} Var(z)={:.2}",
                    phi, qs, imp_z, vz, vvar
                );
            }
        }
    }

    let _ = (wr_single, wr_ao5);
}

// ── luck stat ───────────────────────────────────────────────────────────────
#[derive(Serialize)]
struct LuckEntry {
    ao5_rank: usize,
    skill_rank: usize, // rank by leave-one-out skill within this top-100
    pid: String,
    name: String,
    cid: String,
    comp_id: String,
    date: String,
    ao5: f64,       // the record official average, centiseconds
    skill: f64,     // leave-one-out skill estimate at that comp, centiseconds
    sigmas: f64,    // ao5-σ below skill (negative = lucky/over its skill)
    luck_prob: f64, // P(an ao5 this good or better | LOO skill) via MC
}

/// The top-100 official 333 averages (best per person), each annotated with how
/// lucky it was: the competitor's **leave-one-out** skill at that competition
/// (estimated from their surrounding comps, excluding the record week) and the
/// Monte-Carlo probability of matching-or-beating that average at that skill.
/// Low probability / negative σ = the average outran their skill (lucky); the
/// `ao5_rank` vs `skill_rank` gap shows who is propped up by one great result.
fn build_luck(
    db: &WcaDb,
    event: &str,
    series: &[PersonSeries],
    h: &Hyper,
    comp_start: &HashMap<&str, i32>,
    boundary: i32,
    calibrate: bool,
) -> Vec<LuckEntry> {
    let week_of = |jdn: i32| (jdn - boundary).div_euclid(7);
    let pid2idx: HashMap<&str, usize> =
        series.iter().enumerate().map(|(i, s)| (s.pid.as_str(), i)).collect();

    // Standardized WCA ao5 statistic under the fitted skew shape (sinh-arcsinh):
    // q = ln(trimmed-mean of 5 skewed solves) / sd, in single-solve-sd units,
    // level = 0. Tabulated once; its empirical CDF gives P(a round's ao5 ≤ target)
    // including the averaging variance reduction, the trim, the skew, and the fat
    // right tail — replacing the Gaussian Φ. Built at the fast-solver shape and a
    // representative sd (records come from fast solvers; the standardized statistic
    // is ~sd-invariant to first order).
    // Family of standardized ao5 tables, one per tail-weight delta, so each
    // person's tail shape is matched to their skill (faster ⇒ lighter tails).
    // delta grid spans the fitted range; index = round((delta-D0)/DSTEP).
    const D0: f64 = 0.70;
    const DSTEP: f64 = 0.02;
    const NDELTA: usize = 8; // 0.70 .. 0.84
    let build_table = |delta: f64| -> Vec<f32> {
        let eps = SAS_EPS;
        let sd0 = 0.15;
        let (sm, sv) = sas_consts(eps, delta);
        let scale = 1.0 / sv.sqrt();
        let mut rng = Rng(12345);
        let mut q: Vec<f32> = Vec::with_capacity(2_000_000);
        for _ in 0..2_000_000 {
            let mut v: [f64; 5] = std::array::from_fn(|_| {
                let w = (sas(rng.normal(), eps, delta) - sm) * scale; // mean 0, var 1
                (sd0 * w).exp() // linear time, baseline 1.0
            });
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let a = (v[1] + v[2] + v[3]) / 3.0;
            q.push((a.ln() / sd0) as f32);
        }
        q.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        q
    };
    let ao5_tables: Vec<Vec<f32>> =
        (0..NDELTA).into_par_iter().map(|i| build_table(D0 + i as f64 * DSTEP)).collect();
    // Per-level table selection from the skill level (log centiseconds).
    let table_for = |level: f64| -> &Vec<f32> {
        let i = (((sas_delta(level) - D0) / DSTEP).round() as i64).clamp(0, NDELTA as i64 - 1);
        &ao5_tables[i as usize]
    };
    // P(standardized ao5 ≤ x) under the level-matched table.
    let ao5_cdf = |level: f64, x: f64| -> f64 {
        let t = table_for(level);
        t.partition_point(|&q| (q as f64) < x) as f64 / t.len() as f64
    };
    // Std of the standardized ao5 (for the σ-below display) at the fast-solver shape.
    let ao5_factor = {
        let t = table_for((600.0_f64).ln());
        let n = t.len() as f64;
        let m = t.iter().map(|&x| x as f64).sum::<f64>() / n;
        (t.iter().map(|&x| (x as f64 - m).powi(2)).sum::<f64>() / n).sqrt()
    };

    // Best official ao5 per person (the ao5 ranking), with the comp it was set at.
    let mut best: HashMap<&str, (f64, &str)> = HashMap::new();
    // Number of ao5 opportunities (rounds with a valid average) per (person, week).
    let mut rounds: HashMap<(&str, i32), u32> = HashMap::new();
    for r in &db.results {
        if r.event_id != event || r.average <= 0 {
            continue;
        }
        let e = best.entry(r.person_id.as_str()).or_insert((f64::INFINITY, ""));
        if (r.average as f64) < e.0 {
            *e = (r.average as f64, r.competition_id.as_str());
        }
        if let Some(&jdn) = comp_start.get(r.competition_id.as_str()) {
            *rounds.entry((r.person_id.as_str(), week_of(jdn))).or_insert(0) += 1;
        }
    }
    let mut rounds_by_person: HashMap<&str, Vec<(i32, u32)>> = HashMap::new();
    for ((pid, wk), nr) in &rounds {
        rounds_by_person.entry(pid).or_default().push((*wk, *nr));
    }

    // Career luck of a person's record ao5: P(at least one of all their ao5
    // attempts, at their skill each week, lands ≤ their record). The record
    // week's skill is leave-one-out so the record doesn't inflate its own odds.
    // Returns (career_luck, loo_skill_level, sd, σ-below). Cheap analytic combine.
    let career_luck = |idx: usize, record: f64, rec_wk: i32| -> Option<(f64, f64, f64, f64)> {
        let s = &series[idx];
        let sm = run_person(s, h, 1.0, None);
        let mut level_at: HashMap<i32, f64> =
            sm.nodes.iter().map(|&(wk, lvl, _, _)| (wk, lvl)).collect();
        let loo = run_person(s, h, 1.0, Some(rec_wk));
        let j = loo.nodes.partition_point(|e| e.0 < rec_wk);
        let loo_level = loo.nodes.get(j).filter(|e| e.0 == rec_wk).map(|e| e.1)?;
        level_at.insert(rec_wk, loo_level); // record week: leave-one-out skill
        let (mut sw, mut wn) = (0.0, 0.0);
        for w in &s.weeks {
            if w.n >= 2 && w.wk != rec_wk {
                sw += w.s2 * w.n as f64;
                wn += w.n as f64;
            }
        }
        let sd = if wn > 0.0 { (sw / wn).sqrt() } else { h.h0.exp().sqrt() };
        let sigma_ao5 = (sd * ao5_factor).max(1e-4);
        // log P(no attempt beats the record) = Σ_weeks n_rounds · ln(1 − p_week).
        let mut log_surv = 0.0;
        if let Some(weeks) = rounds_by_person.get(s.pid.as_str()) {
            for &(wk, nr) in weeks {
                let level = *level_at.get(&wk).unwrap_or(&loo_level);
                // Argument in single-solve-sd units; the level-matched table
                // encodes the ao5 variance reduction + skew, so divide by sd.
                let p = ao5_cdf(level, (record.ln() - level) / sd).min(1.0 - 1e-12);
                log_surv += nr as f64 * (1.0 - p).ln();
            }
        }
        let luck = 1.0 - log_surv.exp();
        let sigmas = (record.ln() - loo_level) / sigma_ao5;
        Some((luck, loo_level, sd, sigmas))
    };

    // ── Aggregate calibration: mean career luck should be ≈ 0.5 ──
    if calibrate {
        let elig: Vec<(usize, f64, i32)> = best
            .iter()
            .filter_map(|(&pid, &(avg, comp_id))| {
                let idx = *pid2idx.get(pid)?;
                if series[idx].weeks.len() < 8 {
                    return None;
                }
                let wk = week_of(*comp_start.get(comp_id)?);
                Some((idx, avg, wk))
            })
            .collect();
        // Keep (record ao5, luck) so calibration can be split by speed bin.
        let lucks: Vec<(f64, f64)> = elig
            .par_iter()
            .filter_map(|&(idx, avg, wk)| career_luck(idx, avg, wk).map(|r| (avg, r.0)))
            .collect();
        if !lucks.is_empty() {
            let mean = lucks.iter().map(|&(_, l)| l).sum::<f64>() / lucks.len() as f64;
            let mut dec = [0u32; 10];
            for &(_, l) in &lucks {
                dec[((l * 10.0) as usize).min(9)] += 1;
            }
            let pct: Vec<String> =
                dec.iter().map(|&c| format!("{:.1}", 100.0 * c as f64 / lucks.len() as f64)).collect();
            eprintln!(
                "    career-luck calibration (n={}): mean = {:.3} (ideal 0.500)",
                lucks.len(), mean
            );
            eprintln!("      decile % (ideal ~10 each): [{}]", pct.join(", "));
            // Per-speed-bin mean (ideal 0.500 in every bin).
            let bins: &[(&str, f64, f64)] = &[
                ("sub-8 ", 0.0, 800.0),
                ("8-12s ", 800.0, 1200.0),
                ("12-20s", 1200.0, 2000.0),
                ("20s+  ", 2000.0, f64::INFINITY),
            ];
            for &(lbl, lo, hi) in bins {
                let v: Vec<f64> =
                    lucks.iter().filter(|&&(a, _)| a >= lo && a < hi).map(|&(_, l)| l).collect();
                if v.len() >= 2 {
                    let nb = v.len() as f64;
                    let mb = v.iter().sum::<f64>() / nb;
                    let var = v.iter().map(|&l| (l - mb).powi(2)).sum::<f64>() / (nb - 1.0);
                    let se = (var / nb).sqrt();
                    let z = (mb - 0.5) / se;
                    let p = 2.0 * (1.0 - normal_cdf(z.abs())); // H0: mean = 0.500
                    eprintln!(
                        "      bin {lbl}: n={:>6}  mean = {:.3}  (z={:+.1}, p={:.1e} vs 0.500)",
                        v.len(), mb, z, p
                    );
                }
            }
        }
    }

    // ── Top 100 fastest averages, each with career luck ──
    let mut board: Vec<(&str, f64, &str)> = best
        .iter()
        .filter(|(pid, _)| {
            pid2idx.get(**pid).map_or(false, |&i| series[i].weeks.len() >= 8)
        })
        .map(|(&pid, &(avg, cid))| (pid, avg, cid))
        .collect();
    board.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    board.truncate(100);

    let mut rows: Vec<(usize, &str, f64, &str, f64, f64, f64)> = board
        .par_iter()
        .enumerate()
        .filter_map(|(i, &(pid, avg, comp_id))| {
            let idx = *pid2idx.get(pid)?;
            let wk = week_of(*comp_start.get(comp_id)?);
            let (luck, level, _sd, sigmas) = career_luck(idx, avg, wk)?;
            // (ao5_rank index, pid, avg, comp_id, level, sigmas, career_luck)
            Some((i, pid, avg, comp_id, level, sigmas, luck))
        })
        .collect();
    rows.sort_unstable_by_key(|r| r.0); // restore ao5 order

    // Skill rank within the board (fastest LOO skill = 1).
    let mut by_skill: Vec<usize> = (0..rows.len()).collect();
    by_skill.sort_unstable_by(|&a, &b| rows[a].4.partial_cmp(&rows[b].4).unwrap());
    let mut skill_rank = vec![0usize; rows.len()];
    for (rank, &ri) in by_skill.iter().enumerate() {
        skill_rank[ri] = rank + 1;
    }

    let entries: Vec<LuckEntry> = rows
        .iter()
        .enumerate()
        .map(|(pos, &(_i, pid, avg, comp_id, level, sigmas, luck))| {
            let s = &series[*pid2idx.get(pid).unwrap()];
            LuckEntry {
                ao5_rank: pos + 1,
                skill_rank: skill_rank[pos],
                pid: pid.to_string(),
                name: s.name.clone(),
                cid: s.country.clone(),
                comp_id: comp_id.to_string(),
                date: jdn_to_iso(*comp_start.get(comp_id).unwrap()),
                ao5: avg,
                skill: level.exp(),
                sigmas,
                luck_prob: luck,
            }
        })
        .collect();

    entries
}
