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
#[derive(Clone, Copy)]
struct Hyper {
    q_eta: f64, // level process variance
    q_xi: f64,  // slope process variance
    phi: f64,   // trend damping
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
    let phi = h.phi;
    let g = g.max(1);
    // G^g = [[1, c_g],[0, phi^g]] with c_g = sum_{i=1}^{g} phi^i.
    let phi_g = phi.powi(g);
    let c_g = if (1.0 - phi).abs() < 1e-12 {
        g as f64
    } else {
        phi * (1.0 - phi_g) / (1.0 - phi)
    };
    let g_eff: M2 = [[1.0, c_g], [0.0, phi_g]];

    // Q_g = sum_{i=0}^{g-1} G^i Q (G^i)^T, Q = diag(q_eta, q_xi).
    // G^i = [[1, c_i],[0, phi^i]] with c_i = sum_{j=1}^{i} phi^j (c_0 = 0).
    let cap = g.min(300);
    let mut q: M2 = [[0.0; 2]; 2];
    for i in 0..cap {
        let phi_i = phi.powi(i);
        let c_i = if (1.0 - phi).abs() < 1e-12 {
            i as f64
        } else {
            phi * (1.0 - phi_i) / (1.0 - phi)
        };
        q[0][0] += h.q_eta + c_i * c_i * h.q_xi;
        let off = c_i * phi_i * h.q_xi;
        q[0][1] += off;
        q[1][0] += off;
        q[1][1] += phi_i * phi_i * h.q_xi;
    }
    if g > cap {
        // Tail: phi^i ≈ 0, c_i ≈ phi/(1-phi); each step adds ~ q_eta + c∞² q_xi to level.
        let c_inf = if (1.0 - phi).abs() < 1e-12 {
            cap as f64
        } else {
            phi / (1.0 - phi)
        };
        q[0][0] += (g - cap) as f64 * (h.q_eta + c_inf * c_inf * h.q_xi);
    }
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

/// Smoothed output of the coupled filters for one person.
struct Smoothed {
    /// (wk, level, slope, level_var) at each active week (smoothed).
    nodes: Vec<(i32, f64, f64, f64)>,
    h_last: f64,        // smoothed log within-week variance at last week
    loglik_weighted: f64,
    n_weeks: usize,
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

    let mut x: V2 = [s.weeks[0].ybar, 0.0];
    let mut p: M2 = [[0.25, 0.0], [0.0, 0.04]]; // moderately diffuse start
    let mut loglik = 0.0;

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

    Smoothed { nodes, h_last: hb, loglik_weighted: loglik * weight, n_weeks: m }
}

// ── output structs ──────────────────────────────────────────────────────────
#[derive(Serialize)]
struct RankEntry {
    rank: usize,
    pid: String,
    name: String,
    cid: String,
    est: f64,             // exp(level) projected to current week, centiseconds
    level_sd: f64,        // sd of level, log units
    velocity_pct_wk: f64, // (exp(slope)-1)*100, %/week (negative = improving)
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

/// Monte-Carlo an ao5 distribution from the projected state.
fn simulate(level: f64, level_var: f64, sigma2: f64, dnf_p: f64, wr_single: i32, wr_ao5: i32,
            subs: &[i32], seed: u64) -> SimResult {
    let mut rng = Rng(seed | 1);
    let sd = sigma2.sqrt();
    let lvl_sd = level_var.sqrt();
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
                let t = (mean + sd * rng.normal()).exp();
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
    // Coordinate ascent over multiplicative grids for q_eta, q_xi, and φ.
    let factors = [0.5, 0.7, 1.4, 2.0];
    for _pass in 0..3 {
        for which in 0..3 {
            for &f in &factors {
                let mut cand = h;
                match which {
                    0 => cand.q_eta *= f,
                    1 => cand.q_xi *= f,
                    _ => {
                        cand.phi = (1.0 - (1.0 - h.phi) * f).clamp(0.50, 0.999);
                    }
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
        q_eta: 0.0009, // (0.03 log/wk)²
        q_xi: 9e-6,    // (0.003 log/wk)²
        phi: 0.95,
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
        "  kalman_skill: φ={:.4}, σ_η={:.4}/wk, σ_ξ={:.5}/wk (half-life {:.0} wk)",
        hyper.phi,
        hyper.q_eta.sqrt(),
        hyper.q_xi.sqrt(),
        std::f64::consts::LN_2 / (1.0 - hyper.phi).max(1e-6),
    );

    // ── Step 4: smooth everyone, project to current week, simulate top-N ──
    let target_wk = series.iter().flat_map(|s| s.weeks.last()).map(|w| w.wk).max().unwrap_or(0);

    struct Computed {
        idx: usize,
        est: f64,
        level: f64,
        level_var: f64,
        slope: f64,
        h_last: f64,
        dnf_rate: f64,
        last_wk: i32,
    }
    let computed: Vec<Computed> = series
        .par_iter()
        .enumerate()
        .map(|(idx, s)| {
            let sm = run_person(s, &hyper, 1.0, None);
            let (last_wk, level, slope_raw, lvar) = *sm.nodes.last().unwrap();
            // Shrink the trend toward 0 for short histories (few weeks → noisy
            // slope) and clamp it, so we never extrapolate a wild improvement.
            let rel = ((s.weeks.len() as f64 - 2.0) / 10.0).clamp(0.0, 1.0);
            let slope = (slope_raw * rel).clamp(-0.05, 0.05); // ≤ ~5%/week
            // Project to the current week: cap the slope horizon (stale solvers
            // aren't assumed to keep improving) but let variance grow over the
            // full gap.
            let g_full = (target_wk - last_wk).max(0);
            let g_slope = g_full.min(26);
            let phi = hyper.phi;
            let c_slope = if g_slope == 0 {
                0.0
            } else if (1.0 - phi).abs() < 1e-12 {
                g_slope as f64
            } else {
                phi * (1.0 - phi.powi(g_slope)) / (1.0 - phi)
            };
            let level_p = level + c_slope * slope;
            let lvar_p = if g_full == 0 {
                lvar
            } else {
                let (g_eff, q_g) = gap_transition(&hyper, g_full);
                mat_add(
                    mat_mul(mat_mul(g_eff, [[lvar, 0.0], [0.0, hyper.q_xi.max(1e-9)]]), transpose(g_eff)),
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
                slope,
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
                level_sd: c.level_var.sqrt(),
                velocity_pct_wk: (c.slope.exp() - 1.0) * 100.0,
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
            let mut x: V2 = [s.weeks[0].ybar, 0.0];
            let mut p: M2 = [[0.25, 0.0], [0.0, 0.04]];
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
    let mut x: V2 = [s.weeks[0].ybar, 0.0];
    let mut p: M2 = [[0.25, 0.0], [0.0, 0.04]];
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

    // Std of a WCA ao5 (trimmed mean of 5) relative to a single-solve sd.
    let ao5_factor = {
        let mut rng = Rng(12345);
        let (mut sum, mut sumsq, n) = (0.0, 0.0, 200_000u64);
        for _ in 0..n {
            let mut v: [f64; 5] = std::array::from_fn(|_| rng.normal());
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let a = (v[1] + v[2] + v[3]) / 3.0;
            sum += a;
            sumsq += a * a;
        }
        (sumsq / n as f64 - (sum / n as f64).powi(2)).sqrt()
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
                let p = normal_cdf((record.ln() - level) / sigma_ao5).min(1.0 - 1e-12);
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
        let lucks: Vec<f64> = elig
            .par_iter()
            .filter_map(|&(idx, avg, wk)| career_luck(idx, avg, wk).map(|r| r.0))
            .collect();
        if !lucks.is_empty() {
            let mean = lucks.iter().sum::<f64>() / lucks.len() as f64;
            let mut dec = [0u32; 10];
            for &l in &lucks {
                dec[((l * 10.0) as usize).min(9)] += 1;
            }
            let pct: Vec<String> =
                dec.iter().map(|&c| format!("{:.1}", 100.0 * c as f64 / lucks.len() as f64)).collect();
            eprintln!(
                "    career-luck calibration (n={}): mean = {:.3} (ideal 0.500)",
                lucks.len(), mean
            );
            eprintln!("      decile % (ideal ~10 each): [{}]", pct.join(", "));
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

    let _ = ao5_factor;
    entries
}
