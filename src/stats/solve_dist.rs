//! Empirical solve-distribution probe for 3x3.
//!
//! The Kalman skill model assumes within-week solve times are log-normal with a
//! roughly constant CV. This diagnostic checks that assumption against the data,
//! **model-free**: pool each competitor's valid solves into person-week buckets
//! (the same granularity the model treats as iid), remove that week's own raw
//! log-mean to isolate within-week spread (no Kalman level used, so nothing is
//! biased toward the model), pool the residuals by skill tier (week mean < 7s,
//! 7-10s, 10-20s, 20s+), and report shape (CV, skew, kurtosis), tail mass vs
//! Gaussian, quantiles, and the DNF rate. Linear ratio = exp(residual) shows
//! raw-time right-skew.
//!
//! Run with: `cargo run --release -- data out solve-dist`

use anyhow::Result;
use std::collections::HashMap;

use crate::db::WcaDb;

const EVENT: &str = "333";

/// Convert a calendar date to a Julian Day Number (proleptic Gregorian).
fn ymd_to_jdn(year: u16, month: u8, day: u8) -> i32 {
    let a = (14 - month as i32) / 12;
    let y2 = year as i32 + 4800 - a;
    let m2 = month as i32 + 12 * a - 3;
    let d = day as i32;
    d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045
}

/// Inclusive-lower / exclusive-upper bands on the week-mean time (centiseconds).
/// Fine at the fast end (where we care): sub-6 split out from sub-7 etc.
const TIERS: &[(&str, f64, f64)] = &[
    ("sub-6   ", 0.0, 600.0),
    ("6-7s    ", 600.0, 700.0),
    ("7-8s    ", 700.0, 800.0),
    ("8-10s   ", 800.0, 1000.0),
    ("10-15s  ", 1000.0, 1500.0),
    ("15-20s  ", 1500.0, 2000.0),
    ("20s+    ", 2000.0, f64::INFINITY),
];

#[derive(Default)]
struct Tier {
    resid: Vec<f32>, // log-space residuals about each person-week's own log-mean
    ss: f64,         // Σ residual²
    df: f64,         // Σ (n_valid − 1), for the unbiased pooled variance
    n_weeks: u64,
    n_solves: u64, // valid solves
    n_dnf: u64,
    n_attempts: u64, // valid + DNF (DNS excluded)
}

fn tier_index(week_mean_cs: f64) -> Option<usize> {
    TIERS.iter().position(|&(_, lo, hi)| week_mean_cs >= lo && week_mean_cs < hi)
}

/// Φ(x) via Abramowitz & Stegun 7.1.26 on erf.
fn normal_cdf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * (x / std::f64::consts::SQRT_2).abs());
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-(x * x) / 2.0).exp();
    if x >= 0.0 {
        0.5 + 0.5 * y
    } else {
        0.5 - 0.5 * y
    }
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

/// sinh-arcsinh shape transform of a standard-normal draw z.
/// eps = skew (eps>0 ⇒ right skew), delta = tail weight (delta<1 ⇒ heavier).
fn sas(z: f64, eps: f64, delta: f64) -> f64 {
    ((z.asinh() + eps) / delta).sinh()
}

const LN_SQRT_2PI: f64 = 0.918_938_533_204_672_7;

/// log standard-normal density.
fn norm_logpdf(z: f64) -> f64 {
    -0.5 * z * z - LN_SQRT_2PI
}

/// log density of W = sinh((asinh(Z)+eps)/delta), Z~N(0,1), evaluated at w.
/// Inverse: z = sinh(delta·asinh(w) − eps); f(w) = φ(z)·δ·cosh(δ·asinh(w)−eps)/√(1+w²).
fn sas_logpdf(w: f64, eps: f64, delta: f64) -> f64 {
    let a = delta * w.asinh() - eps;
    let z = a.sinh();
    norm_logpdf(z) + delta.ln() + a.cosh().ln() - 0.5 * (1.0 + w * w).ln()
}

/// Mean and variance of the standardized SAS shape S(Z), via a fixed deterministic
/// quadrature over the normal (so a location/scale fit can match sample moments).
fn sas_moments(eps: f64, delta: f64, zk: &[f64]) -> (f64, f64) {
    let k = zk.len() as f64;
    let m = zk.iter().map(|&z| sas(z, eps, delta)).sum::<f64>() / k;
    let v = zk.iter().map(|&z| (sas(z, eps, delta) - m).powi(2)).sum::<f64>() / k;
    (m, v)
}

fn quantile(sorted: &[f32], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (p * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)] as f64
}

pub fn run(db: &WcaDb) -> Result<()> {
    // Competition start date → JDN, for weekly bucketing.
    let comp_jdn: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    // Pool valid log-solves + DNF counts per (person, week). Week = floor(jdn/7);
    // exact boundary is immaterial here, this is purely descriptive.
    let mut buckets: HashMap<(&str, i32), (Vec<f32>, u32)> = HashMap::new();
    for r in &db.results {
        if r.event_id != EVENT {
            continue;
        }
        let Some(&jdn) = comp_jdn.get(r.competition_id.as_str()) else {
            continue;
        };
        let Some(times) = db.attempts.get(&r.id) else {
            continue;
        };
        let wk = jdn.div_euclid(7);
        let entry = buckets.entry((r.person_id.as_str(), wk)).or_default();
        for &v in times {
            if v > 0 {
                entry.0.push((v as f64).ln() as f32);
            } else if v == -1 {
                entry.1 += 1; // DNF (DNS / v == -2 ignored)
            }
        }
    }

    let mut tiers: HashMap<usize, Tier> = HashMap::new();
    for ((_, _), (logs, dnf)) in &buckets {
        let dnf = *dnf as u64;
        // Tier on the week's own raw geomean (model-free).
        let nv = logs.len();
        if nv == 0 {
            continue;
        }
        let mean = logs.iter().map(|&y| y as f64).sum::<f64>() / nv as f64;
        let Some(ti) = tier_index(mean.exp()) else { continue };
        let t = tiers.entry(ti).or_default();
        t.n_solves += nv as u64;
        t.n_dnf += dnf;
        t.n_attempts += dnf + nv as u64;
        if nv < 2 {
            continue; // need ≥2 valid solves to de-mean
        }
        for &y in logs {
            let res = y as f64 - mean;
            t.resid.push(res as f32);
            t.ss += res * res;
        }
        t.df += nv as f64 - 1.0;
        t.n_weeks += 1;
    }

    // Deterministic normal quadrature nodes for SAS moment matching.
    let zk: Vec<f64> = (0..4000).map(|k| inv_norm((k as f64 + 0.5) / 4000.0)).collect();

    eprintln!();
    eprintln!("=== Empirical 3x3 within-week solve distribution (log space) ===");
    eprintln!("Model-free: de-meaned per person-week, pooled by week-mean tier. CV ≈ sd of log time.");
    eprintln!();

    for (ti, &(label, _, _)) in TIERS.iter().enumerate() {
        let Some(t) = tiers.get(&ti) else { continue };
        if t.resid.len() < 100 {
            continue;
        }

        // Unbiased pooled within-round sd (the canonical CV estimate).
        let var = t.ss / t.df.max(1.0);
        let sd = var.sqrt();

        // Central moments of the de-meaned residuals (skew/kurtosis are scale-free,
        // so the slight within-round shrinkage doesn't bias these).
        let n = t.resid.len() as f64;
        let (mut m2, mut m3, mut m4) = (0.0, 0.0, 0.0);
        for &r in &t.resid {
            let r = r as f64;
            let r2 = r * r;
            m2 += r2;
            m3 += r2 * r;
            m4 += r2 * r2;
        }
        m2 /= n;
        m3 /= n;
        m4 /= n;
        let skew = m3 / m2.powf(1.5);
        let exkurt = m4 / (m2 * m2) - 3.0;

        // Tail mass in sd units vs Gaussian expectation.
        let mut sorted = t.resid.clone();
        sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let frac = |pred: &dyn Fn(f64) -> bool| {
            sorted.iter().filter(|&&r| pred(r as f64)).count() as f64 / n
        };
        let r2sd = frac(&|r| r > 2.0 * sd);
        let r3sd = frac(&|r| r > 3.0 * sd);
        let l2sd = frac(&|r| r < -2.0 * sd);
        let l3sd = frac(&|r| r < -3.0 * sd);

        // Linear-space (raw time) skew: ratio to round geomean = exp(residual).
        let ratios: Vec<f64> = t.resid.iter().map(|&r| (r as f64).exp()).collect();
        let rm = ratios.iter().sum::<f64>() / n;
        let (mut rm2, mut rm3) = (0.0, 0.0);
        for &x in &ratios {
            let d = x - rm;
            rm2 += d * d;
            rm3 += d * d * d;
        }
        let lin_skew = (rm3 / n) / (rm2 / n).powf(1.5);

        let dnf_rate = t.n_dnf as f64 / t.n_attempts.max(1) as f64;

        eprintln!("Tier {label}  weeks={:>8}  solves={:>9}", t.n_weeks, t.n_solves);
        eprintln!(
            "  CV(sd_log)={:.4}   skew={:+.3}  exkurt={:+.3}   lin-skew(raw)={:+.3}   DNF={:.2}%",
            sd, skew, exkurt, lin_skew, dnf_rate * 100.0
        );
        eprintln!(
            "  right tail  >2sd {:.3}% (N {:.3}%)  >3sd {:.3}% (N {:.3}%)",
            r2sd * 100.0,
            (1.0 - normal_cdf(2.0)) * 100.0,
            r3sd * 100.0,
            (1.0 - normal_cdf(3.0)) * 100.0
        );
        eprintln!(
            "  left  tail  <2sd {:.3}%             <3sd {:.3}%",
            l2sd * 100.0,
            l3sd * 100.0
        );
        // Standardized quantiles vs the standard-normal value at each p.
        let qs = [0.01, 0.05, 0.25, 0.50, 0.75, 0.95, 0.99];
        let zn = [-2.326, -1.645, -0.674, 0.0, 0.674, 1.645, 2.326];
        let line: String = qs
            .iter()
            .zip(zn)
            .map(|(&p, z)| format!("p{:02}={:+.2}/{:+.2}", (p * 100.0) as i32, quantile(&sorted, p) / sd, z))
            .collect::<Vec<_>>()
            .join("  ");
        eprintln!("  std quantiles (emp/normal): {line}");

        // --- Fit candidate shapes by least-squares on quantiles (log units) ---
        // Probe grid, tail-weighted. Empirical quantile e_i vs model shape S(z_p).
        let probes: &[f64] = &[
            0.005, 0.01, 0.02, 0.05, 0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90,
            0.95, 0.98, 0.99, 0.995,
        ];
        let emp: Vec<f64> = probes.iter().map(|&p| quantile(&sorted, p)).collect();
        let zp: Vec<f64> = probes.iter().map(|&p| inv_norm(p)).collect();

        // Least-squares fit of e ≈ μ + σ·S over the probe points; returns RMSE.
        let ls_rmse = |shape: &[f64]| -> f64 {
            let m = shape.len() as f64;
            let (sm, em) = (shape.iter().sum::<f64>() / m, emp.iter().sum::<f64>() / m);
            let (mut cov, mut var) = (0.0, 0.0);
            for (s, e) in shape.iter().zip(&emp) {
                cov += (s - sm) * (e - em);
                var += (s - sm) * (s - sm);
            }
            let sigma = cov / var;
            let mu = em - sigma * sm;
            (shape
                .iter()
                .zip(&emp)
                .map(|(s, e)| (e - mu - sigma * s).powi(2))
                .sum::<f64>()
                / m)
                .sqrt()
        };

        // Gaussian baseline: S(z) = z.
        let gauss_rmse = ls_rmse(&zp);

        // sinh-arcsinh: grid over (eps, delta).
        let mut best = (0.0f64, 1.0f64, f64::INFINITY);
        let mut eps = -1.20;
        while eps <= 1.201 {
            let mut delta = 0.40;
            while delta <= 1.601 {
                let shape: Vec<f64> = zp.iter().map(|&z| sas(z, eps, delta)).collect();
                let rmse = ls_rmse(&shape);
                if rmse < best.2 {
                    best = (eps, delta, rmse);
                }
                delta += 0.02;
            }
            eps += 0.02;
        }
        eprintln!(
            "  fit RMSE (log units, lower=better): normal {:.4}   sinh-arcsinh {:.4}  (eps={:+.2} delta={:.2}, {:.0}% better)",
            gauss_rmse,
            best.2,
            best.0,
            best.1,
            (1.0 - best.2 / gauss_rmse) * 100.0
        );

        // --- Likelihood / information-criterion model selection ---
        // Both models matched to the sample variance (mean ≈ 0 by construction);
        // SAS spends 2 extra global params (eps, delta) on shape.
        let var0 = m2; // second central moment ≈ variance (residual mean ≈ 0)
        let sig0 = var0.sqrt();
        // Total log-likelihood of the residuals under a model defined by (eps,delta).
        // Normal is the eps=0, delta=1 limit.
        let total_ll = |eps: f64, delta: f64, data: &[f32]| -> f64 {
            let (ms, vs) = sas_moments(eps, delta, &zk);
            let sigma = (var0 / vs).sqrt();
            let mu = -sigma * ms;
            let ln_sig = sigma.ln();
            data.iter()
                .map(|&r| {
                    let w = (r as f64 - mu) / sigma;
                    sas_logpdf(w, eps, delta) - ln_sig
                })
                .sum::<f64>()
        };

        // Local MLE refine of (eps,delta) around the quantile fit, on a subsample.
        let stride = (sorted.len() / 100_000).max(1);
        let sub: Vec<f32> = sorted.iter().copied().step_by(stride).collect();
        let scale = sorted.len() as f64 / sub.len() as f64; // extrapolate subsample LL
        let (mut be, mut bd, mut bll) = (best.0, best.1, f64::NEG_INFINITY);
        for i in -4..=4 {
            for j in -4..=4 {
                let e = best.0 + i as f64 * 0.02;
                let d = (best.1 + j as f64 * 0.02).max(0.3);
                let ll = total_ll(e, d, &sub);
                if ll > bll {
                    (be, bd, bll) = (e, d, ll);
                }
            }
        }
        // Full-sample LL at the chosen params vs the Gaussian baseline.
        let ll_sas = total_ll(be, bd, &sorted);
        let ll_norm: f64 = sorted
            .iter()
            .map(|&r| norm_logpdf(r as f64 / sig0) - sig0.ln())
            .sum();
        let _ = (bll, scale);
        let nn = sorted.len() as f64;
        let d_ll = ll_sas - ll_norm; // SAS adds 2 params
        let d_aic = -2.0 * d_ll + 2.0 * 2.0;
        let d_bic = -2.0 * d_ll + 2.0 * nn.ln();
        let bits = d_ll / nn / std::f64::consts::LN_2;
        eprintln!(
            "  model LL: ΔAIC={:+.0}  ΔBIC={:+.0}  (neg ⇒ SAS wins)   Δ={:+.4} nats/solve = {:+.4} bits/solve   MLE eps={:+.2} delta={:.2}",
            d_aic, d_bic, d_ll / nn, bits, be, bd
        );
        eprintln!();
    }

    Ok(())
}

/// Shared-scramble correlation probe (3x3 finals).
///
/// Finalists solve the **same 5 scrambles**, so an easy scramble is easy for
/// everyone — a shared per-scramble effect that the iid competition sim ignores.
/// Decompose each final's 5×m log-time matrix two-way:
///   log t[i,k] = μ_i (solver) + s_k (scramble) + ε[i,k]
/// pool MS_scramble & MS_error across finals, back out Var(s), and report
///   ρ = Var(s)/(Var(s)+Var(ε))   and the gap-variance factor (1−ρ),
/// i.e. how much smaller Var(time_A − time_B) is than the uncorrelated prediction.
///
/// Run with: `cargo run --release -- data out scramble-corr`
pub fn scramble_corr(db: &WcaDb) -> Result<()> {
    let final_ids: std::collections::HashSet<&str> = db
        .round_types
        .values()
        .filter(|rt| rt.is_final != 0)
        .map(|rt| rt.id.as_str())
        .collect();

    // Group 333 final results by competition: (pid, official avg, 5 log-times).
    type Row = (f64, [f64; 5]);
    let mut by_comp: HashMap<&str, Vec<Row>> = HashMap::new();
    for r in &db.results {
        if r.event_id != EVENT || !final_ids.contains(r.round_type_id.as_str()) {
            continue;
        }
        let Some(times) = db.attempts.get(&r.id) else { continue };
        if times.len() != 5 || times.iter().any(|&v| v <= 0) {
            continue; // need all 5 valid (balanced ANOVA)
        }
        let logs: [f64; 5] = std::array::from_fn(|k| (times[k] as f64).ln());
        by_comp
            .entry(r.competition_id.as_str())
            .or_default()
            .push((r.average as f64, logs));
    }

    // Strong-final band thresholds on the 3rd-place official average (centiseconds).
    let bands: &[(&str, f64)] = &[("3rd sub-8 ", 800.0), ("3rd sub-12", 1200.0), ("all finals", f64::INFINITY)];
    eprintln!();
    eprintln!("=== 3x3 finals: shared-scramble correlation (two-way log ANOVA) ===");
    eprintln!("Same group ⇒ same 5 scrambles. rho = cross-solver corr on a scramble.");
    eprintln!();

    for &(label, thr) in bands {
        // Pooled sums across qualifying finals.
        let (mut ss_scr, mut df_scr) = (0.0f64, 0.0f64);
        let (mut ss_err, mut df_err) = (0.0f64, 0.0f64);
        let (mut sum_m, mut n_rounds) = (0.0f64, 0u64);
        for rows in by_comp.values() {
            let m = rows.len();
            if m < 4 {
                continue;
            }
            let mut avgs: Vec<f64> = rows.iter().map(|r| r.0).collect();
            avgs.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
            if avgs.get(2).map_or(true, |&a| a >= thr) {
                continue;
            }
            let mf = m as f64;
            let nf = 5.0;
            // Grand mean.
            let grand = rows.iter().map(|r| r.1.iter().sum::<f64>()).sum::<f64>() / (mf * nf);
            // Solver means (rows) and scramble means (cols).
            let solver_mean: Vec<f64> = rows.iter().map(|r| r.1.iter().sum::<f64>() / nf).collect();
            let mut col_mean = [0.0f64; 5];
            for r in rows {
                for k in 0..5 {
                    col_mean[k] += r.1[k] / mf;
                }
            }
            // SS_scramble (cols) and SS_error.
            let ss_c = mf * col_mean.iter().map(|&c| (c - grand).powi(2)).sum::<f64>();
            let mut ss_e = 0.0;
            for (i, r) in rows.iter().enumerate() {
                for k in 0..5 {
                    let fitted = solver_mean[i] + col_mean[k] - grand;
                    ss_e += (r.1[k] - fitted).powi(2);
                }
            }
            ss_scr += ss_c;
            df_scr += nf - 1.0; // 4 per round
            ss_err += ss_e;
            df_err += (mf - 1.0) * (nf - 1.0);
            sum_m += mf;
            n_rounds += 1;
        }
        if n_rounds == 0 {
            continue;
        }
        let ms_scr = ss_scr / df_scr;
        let ms_err = ss_err / df_err;
        let m_bar = sum_m / n_rounds as f64;
        // E[MS_scramble] = Var(eps) + m·Var(s) ⇒ back out Var(s) at the mean field size.
        let var_s = ((ms_scr - ms_err) / m_bar).max(0.0);
        let var_e = ms_err;
        let rho = var_s / (var_s + var_e);
        let sd_e = var_e.sqrt();
        let sd_tot = (var_s + var_e).sqrt();
        eprintln!(
            "{label}: rounds={:>5}  m̄={:.1}  rho={:.3}  Var(scramble)={:.5} Var(idio)={:.5}",
            n_rounds, m_bar, rho, var_s, var_e
        );
        eprintln!(
            "            single CV: total {:.4}  idiosyncratic {:.4}   gap-var factor (1−rho)={:.3}  ⇒ Var(A−B) is {:.0}% of iid",
            sd_tot, sd_e, 1.0 - rho, 100.0 * (1.0 - rho)
        );
    }
    Ok(())
}

/// Probe: does within-week spread / "lucky-fast reach" predict future improvement,
/// controlling for skill level and career age?
///
/// Per person-week (≥5 valid solves) compute level = mean log-time, sd, and
/// reach = (mean − min)/sd (how many sd the best solve sits below the mean — the
/// left-tail "lucky solve" capability). Future improvement Δ = mean log-time over
/// weeks [wk+13, wk+65] minus the current level (negative = got faster). Bin anchors
/// by (level × career-age) and split each cell at its median sd and median reach, so
/// the comparison is within-confounder. If the high-reach / high-sd halves improve
/// more, the hypothesis holds.
///
/// Run with: `cargo run --release -- data out improve-signal`
pub fn improve_signal(db: &WcaDb) -> Result<()> {
    let comp_jdn: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    // (pid, week) → (n, sum_log, sumsq_log, min_log)
    let mut pw: HashMap<(&str, i32), (u32, f64, f64, f64)> = HashMap::new();
    for r in &db.results {
        if r.event_id != EVENT {
            continue;
        }
        let Some(&jdn) = comp_jdn.get(r.competition_id.as_str()) else { continue };
        let Some(times) = db.attempts.get(&r.id) else { continue };
        let wk = jdn.div_euclid(7);
        let e = pw.entry((r.person_id.as_str(), wk)).or_insert((0, 0.0, 0.0, f64::INFINITY));
        for &v in times {
            if v > 0 {
                let y = (v as f64).ln();
                e.0 += 1;
                e.1 += y;
                e.2 += y * y;
                if y < e.3 { e.3 = y; }
            }
        }
    }
    // Per person → sorted weeks with (wk, n, mean, sd, min).
    let mut by_person: HashMap<&str, Vec<(i32, u32, f64, f64, f64)>> = HashMap::new();
    for ((pid, wk), (n, s, ss, mn)) in &pw {
        if *n < 1 {
            continue;
        }
        let nf = *n as f64;
        let mean = s / nf;
        let sd = if *n >= 2 { ((ss - nf * mean * mean) / (nf - 1.0)).max(0.0).sqrt() } else { 0.0 };
        by_person.entry(pid).or_default().push((*wk, *n, mean, sd, *mn));
    }

    // Build anchors: (cell, sd, reach, delta).
    let lvl_edges = [700.0_f64.ln(), 1000.0_f64.ln(), 1500.0_f64.ln(), 2500.0_f64.ln()];
    let age_edges = [26, 104, 260];
    let lvl_bin = |m: f64| lvl_edges.iter().position(|&e| m < e).unwrap_or(lvl_edges.len());
    let age_bin = |a: i32| age_edges.iter().position(|&e| a < e).unwrap_or(age_edges.len());
    let ncell = (lvl_edges.len() + 1) * (age_edges.len() + 1);

    let mut anchors: Vec<(usize, f64, f64, f64)> = Vec::new();
    for weeks in by_person.values() {
        let mut w = weeks.clone();
        w.sort_unstable_by_key(|x| x.0);
        let first = w[0].0;
        for i in 0..w.len() {
            let (wk, n, mean, sd, mn) = w[i];
            if n < 5 || sd <= 0.0 {
                continue;
            }
            // Future window [wk+13, wk+65], solve-weighted mean log-time.
            let (mut fs, mut fn_) = (0.0f64, 0u32);
            for j in (i + 1)..w.len() {
                let fwk = w[j].0;
                if fwk <= wk + 13 { continue; }
                if fwk > wk + 65 { break; }
                fs += w[j].2 * w[j].1 as f64;
                fn_ += w[j].1;
            }
            if fn_ < 10 {
                continue;
            }
            let future_mean = fs / fn_ as f64;
            let delta = future_mean - mean; // negative = improvement
            let reach = (mean - mn) / sd;
            let cell = lvl_bin(mean) * (age_edges.len() + 1) + age_bin(wk - first);
            anchors.push((cell, sd, reach, delta));
        }
    }

    // Per-cell medians of sd and reach.
    let med = |vals: &mut Vec<f64>| -> f64 {
        if vals.is_empty() { return f64::NAN; }
        vals.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        vals[vals.len() / 2]
    };
    let mut sd_med = vec![f64::NAN; ncell];
    let mut reach_med = vec![f64::NAN; ncell];
    for c in 0..ncell {
        let mut sds: Vec<f64> = anchors.iter().filter(|a| a.0 == c).map(|a| a.1).collect();
        let mut rs: Vec<f64> = anchors.iter().filter(|a| a.0 == c).map(|a| a.2).collect();
        sd_med[c] = med(&mut sds);
        reach_med[c] = med(&mut rs);
    }

    // Within-cell median split; aggregate future Δ for each half.
    let mut acc = [[(0.0f64, 0u64); 2]; 2]; // [signal: sd,reach][half: lo,hi]
    let (mut tot, mut tn) = (0.0f64, 0u64);
    for &(c, sd, reach, d) in &anchors {
        tot += d; tn += 1;
        let hs = (sd > sd_med[c]) as usize;
        acc[0][hs].0 += d; acc[0][hs].1 += 1;
        let hr = (reach > reach_med[c]) as usize;
        acc[1][hr].0 += d; acc[1][hr].1 += 1;
    }
    let mean = |t: (f64, u64)| if t.1 > 0 { t.0 / t.1 as f64 } else { f64::NAN };

    eprintln!();
    eprintln!("=== Does spread / lucky-fast reach predict future improvement? (3x3) ===");
    eprintln!("Δ = future mean log-time − current (negative = improved). Within (level×age) cells.");
    eprintln!("anchors n={tn}  overall mean Δ = {:+.4} (people improve on average)", mean((tot, tn)));
    eprintln!(
        "  within-cell sd split:    low-sd  Δ={:+.4} (n{})   high-sd  Δ={:+.4} (n{})",
        mean(acc[0][0]), acc[0][0].1, mean(acc[0][1]), acc[0][1].1
    );
    eprintln!(
        "  within-cell reach split: low-rch Δ={:+.4} (n{})   high-rch Δ={:+.4} (n{})",
        mean(acc[1][0]), acc[1][0].1, mean(acc[1][1]), acc[1][1].1
    );
    eprintln!("  (more-negative high half ⇒ the signal predicts improvement)");
    Ok(())
}

/// Empirical improvement-curve study (3x3): improvement rate vs level, vs calendar
/// era, and the aligned average trajectory of cohorts grouped by eventual best.
/// Anchors use trailing/forward multi-week windows (≥10 solves each) to avoid the
/// single-week mean-reversion artifact.
///
/// Run with: `cargo run --release -- data out improve-curve`
pub fn improve_curve(db: &WcaDb) -> Result<()> {
    let comp_jdn: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    // (pid, week) → (n, sum_log)
    let mut pw: HashMap<(&str, i32), (u32, f64)> = HashMap::new();
    for r in &db.results {
        if r.event_id != EVENT {
            continue;
        }
        let Some(&jdn) = comp_jdn.get(r.competition_id.as_str()) else { continue };
        let Some(times) = db.attempts.get(&r.id) else { continue };
        let wk = jdn.div_euclid(7);
        let e = pw.entry((r.person_id.as_str(), wk)).or_insert((0, 0.0));
        for &v in times {
            if v > 0 {
                e.0 += 1;
                e.1 += (v as f64).ln();
            }
        }
    }
    let mut by_person: HashMap<&str, Vec<(i32, u32, f64)>> = HashMap::new();
    for ((pid, wk), (n, s)) in &pw {
        if *n == 0 {
            continue; // all-DNF week: no valid mean
        }
        by_person.entry(pid).or_default().push((*wk, *n, s / *n as f64));
    }

    // Solve-weighted mean log-time over weeks in [lo, hi]; returns (mean, total n).
    let window = |w: &[(i32, u32, f64)], lo: i32, hi: i32| -> (f64, u32) {
        let (mut s, mut n) = (0.0f64, 0u32);
        for &(wk, c, m) in w {
            if wk >= lo && wk <= hi {
                s += m * c as f64;
                n += c;
            }
        }
        if n > 0 { (s / n as f64, n) } else { (f64::NAN, 0) }
    };
    let year_of = |wk: i32| 1970.0 + ((wk * 7 - 2440588) as f64) / 365.25;

    // ── Tables A & B: improvement rate (log/week) anchored at level L0 ──
    let lvl_edges = [600.0, 800.0, 1000.0, 1300.0, 1700.0, 2200.0, 3000.0]; // cs
    let lvl_lab = ["<6s", "6-8", "8-10", "10-13", "13-17", "17-22", "22-30", "30s+"];
    let lvl_bin = |cs: f64| lvl_edges.iter().position(|&e| cs < e).unwrap_or(lvl_edges.len());
    let era_edges = [2012.0, 2016.0, 2020.0];
    let era_lab = ["<2012", "12-16", "16-20", "20+"];
    let era_bin = |y: f64| era_edges.iter().position(|&e| y < e).unwrap_or(era_edges.len());
    let nlvl = lvl_edges.len() + 1;
    let nera = era_edges.len() + 1;

    let mut a_sum = vec![0.0f64; nlvl];
    let mut a_n = vec![0u64; nlvl];
    let mut b_sum = vec![vec![0.0f64; nera]; nlvl];
    let mut b_n = vec![vec![0u64; nera]; nlvl];

    for w in by_person.values() {
        let mut w = w.clone();
        w.sort_unstable_by_key(|x| x.0);
        for &(wk, _, _) in &w {
            let (l0, n0) = window(&w, wk - 8, wk);
            if n0 < 10 {
                continue;
            }
            let (l1, n1) = window(&w, wk + 26, wk + 52);
            if n1 < 10 {
                continue;
            }
            let rate = (l1 - l0) / 39.0; // log per week (negative = improving)
            let lb = lvl_bin(l0.exp());
            let eb = era_bin(year_of(wk));
            a_sum[lb] += rate;
            a_n[lb] += 1;
            b_sum[lb][eb] += rate;
            b_n[lb][eb] += 1;
        }
    }

    eprintln!();
    eprintln!("=== 3x3 improvement rate vs level (anchor L0, forward 26–52wk) ===");
    eprintln!("rate = %/week change in mean log-time (negative = improving):");
    for i in 0..nlvl {
        if a_n[i] > 50 {
            eprintln!(
                "  {:>6}: {:+.3}%/wk  (n={})   ≈ {:+.1}%/yr",
                lvl_lab[i], 100.0 * a_sum[i] / a_n[i] as f64, a_n[i],
                100.0 * a_sum[i] / a_n[i] as f64 * 52.0
            );
        }
    }
    eprintln!();
    eprintln!("=== improvement rate (%/wk) by level × calendar era ===");
    eprint!("  {:>6}", "level");
    for e in 0..nera { eprint!("  {:>8}", era_lab[e]); }
    eprintln!();
    for i in 0..nlvl {
        if a_n[i] <= 50 { continue; }
        eprint!("  {:>6}", lvl_lab[i]);
        for e in 0..nera {
            if b_n[i][e] > 30 {
                eprint!("  {:>+8.3}", 100.0 * b_sum[i][e] / b_n[i][e] as f64);
            } else {
                eprint!("  {:>8}", "·");
            }
        }
        eprintln!();
    }

    // ── Table D: aligned mean trajectory by eventual-best cohort ──
    let coh_edges = [700.0, 1000.0, 1500.0, 2500.0];
    let coh_lab = ["eventual sub-7", "sub-10", "sub-15", "sub-25", "25s+"];
    let age_pts = [0, 13, 26, 52, 104, 156, 260, 364]; // weeks since first comp
    let ncoh = coh_edges.len() + 1;
    let np = age_pts.len();
    let mut d_sum = vec![vec![0.0f64; np]; ncoh];
    let mut d_n = vec![vec![0u64; np]; ncoh];

    for w in by_person.values() {
        let mut w = w.clone();
        w.sort_unstable_by_key(|x| x.0);
        if w.len() < 5 {
            continue;
        }
        let best = w.iter().map(|x| x.2).fold(f64::INFINITY, f64::min).exp();
        let coh = coh_edges.iter().position(|&e| best < e).unwrap_or(coh_edges.len());
        let first = w[0].0;
        for (pi, &ap) in age_pts.iter().enumerate() {
            // Level around career age `ap` (±8wk window).
            let (l, n) = window(&w, first + ap - 8, first + ap + 8);
            if n >= 5 {
                d_sum[coh][pi] += l.exp();
                d_n[coh][pi] += 1;
            }
        }
    }
    eprintln!();
    eprintln!("=== aligned mean level (seconds) by career age, grouped by eventual best ===");
    eprint!("  {:>14}", "cohort\\age(wk)");
    for &ap in &age_pts { eprint!("  {:>6}", ap); }
    eprintln!();
    for c in 0..ncoh {
        eprint!("  {:>14}", coh_lab[c]);
        for pi in 0..np {
            if d_n[c][pi] > 20 {
                eprint!("  {:>6.2}", d_sum[c][pi] / d_n[c][pi] as f64 / 100.0);
            } else {
                eprint!("  {:>6}", "·");
            }
        }
        eprintln!();
    }
    Ok(())
}

/// Fit the decay-to-floor improvement form  dℓ/dt = −k·(ℓ − floor)  (3x3).
/// Per person, floor = best sustained trailing-window level (≥20 solves). For each
/// anchor, d = L0 − floor (log units) and the forward rate; if the form holds,
/// rate ≈ −k·d, i.e. k = −rate/d is constant across d. Reports the per-d-bin rate
/// and implied k (linearity check), the global through-origin k + half-life, the
/// floor distribution, and k by calendar era.
///
/// Run with: `cargo run --release -- data out floor-fit`
pub fn floor_fit(db: &WcaDb) -> Result<()> {
    let comp_jdn: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();
    let mut pw: HashMap<(&str, i32), (u32, f64)> = HashMap::new();
    for r in &db.results {
        if r.event_id != EVENT {
            continue;
        }
        let Some(&jdn) = comp_jdn.get(r.competition_id.as_str()) else { continue };
        let Some(times) = db.attempts.get(&r.id) else { continue };
        let wk = jdn.div_euclid(7);
        let e = pw.entry((r.person_id.as_str(), wk)).or_insert((0, 0.0));
        for &v in times {
            if v > 0 {
                e.0 += 1;
                e.1 += (v as f64).ln();
            }
        }
    }
    let mut by_person: HashMap<&str, Vec<(i32, u32, f64)>> = HashMap::new();
    for ((pid, wk), (n, s)) in &pw {
        if *n == 0 {
            continue;
        }
        by_person.entry(pid).or_default().push((*wk, *n, s / *n as f64));
    }
    let window = |w: &[(i32, u32, f64)], lo: i32, hi: i32| -> (f64, u32) {
        let (mut s, mut n) = (0.0f64, 0u32);
        for &(wk, c, m) in w {
            if wk >= lo && wk <= hi {
                s += m * c as f64;
                n += c;
            }
        }
        if n > 0 { (s / n as f64, n) } else { (f64::NAN, 0) }
    };
    let year_of = |wk: i32| 1970.0 + ((wk * 7 - 2440588) as f64) / 365.25;
    let era_edges = [2012.0, 2016.0, 2020.0];
    let era_bin = |y: f64| era_edges.iter().position(|&e| y < e).unwrap_or(era_edges.len());

    // d-bins (log units, distance above floor).
    let d_edges = [0.05, 0.10, 0.15, 0.22, 0.32, 0.45, 0.62, 0.85];
    let nd = d_edges.len() + 1;
    let d_bin = |d: f64| d_edges.iter().position(|&e| d < e).unwrap_or(d_edges.len());
    let mut d_sum = vec![0.0f64; nd];
    let mut d_rate = vec![0.0f64; nd];
    let mut d_n = vec![0u64; nd];
    // Through-origin accumulators, overall and per era.
    let (mut sdr, mut sdd) = (0.0f64, 0.0f64);
    let mut era_dr = [0.0f64; 4];
    let mut era_dd = [0.0f64; 4];
    let mut floors: Vec<f64> = Vec::new();

    for w in by_person.values() {
        let mut w = w.clone();
        w.sort_unstable_by_key(|x| x.0);
        // Floor = min sustained trailing-window (≥20 solves) level.
        let mut floor = f64::INFINITY;
        for &(wk, _, _) in &w {
            let (m, n) = window(&w, wk - 12, wk);
            if n >= 20 && m < floor {
                floor = m;
            }
        }
        if !floor.is_finite() {
            continue;
        }
        floors.push(floor.exp());
        for &(wk, _, _) in &w {
            let (l0, n0) = window(&w, wk - 8, wk);
            if n0 < 10 {
                continue;
            }
            let (l1, n1) = window(&w, wk + 26, wk + 52);
            if n1 < 10 {
                continue;
            }
            let d = l0 - floor; // ≥ ~0
            if d < 0.0 {
                continue;
            }
            let rate = (l1 - l0) / 39.0;
            let b = d_bin(d);
            d_sum[b] += d;
            d_rate[b] += rate;
            d_n[b] += 1;
            sdr += d * rate;
            sdd += d * d;
            let e = era_bin(year_of(wk));
            era_dr[e] += d * rate;
            era_dd[e] += d * d;
        }
    }

    let k = -sdr / sdd; // through-origin slope of rate on d
    eprintln!();
    eprintln!("=== decay-to-floor fit: dℓ/dt = −k·(ℓ − floor)  (3x3) ===");
    eprintln!("d = level − personal floor (log). If form holds, k = −rate/d is flat across d.");
    eprintln!("  {:>10}  {:>10}  {:>10}  {:>8}", "d (mid)", "rate %/wk", "k=-r/d /wk", "n");
    for b in 0..nd {
        if d_n[b] < 50 {
            continue;
        }
        let dm = d_sum[b] / d_n[b] as f64;
        let rm = d_rate[b] / d_n[b] as f64;
        eprintln!(
            "  {:>10.3}  {:>+10.3}  {:>10.4}  {:>8}",
            dm, 100.0 * rm, -rm / dm, d_n[b]
        );
    }
    eprintln!(
        "  global k = {:.4}/wk  ⇒ improvement half-life = {:.0} wk ({:.1} yr); floor reached ~{:.0} wk",
        k, std::f64::consts::LN_2 / k, std::f64::consts::LN_2 / k / 52.0, 3.0 / k
    );
    let era_lab = ["<2012", "12-16", "16-20", "20+"];
    eprint!("  k by era:");
    for e in 0..4 {
        if era_dd[e] > 0.0 {
            eprint!("  {}={:.4}", era_lab[e], -era_dr[e] / era_dd[e]);
        }
    }
    eprintln!();
    floors.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let pc = |p: f64| floors[((p * (floors.len() as f64 - 1.0)) as usize).min(floors.len() - 1)] / 100.0;
    eprintln!(
        "  personal floor distribution (s, n={}): p5={:.2} p25={:.2} p50={:.2} p75={:.2} p95={:.2}",
        floors.len(), pc(0.05), pc(0.25), pc(0.50), pc(0.75), pc(0.95)
    );
    Ok(())
}
