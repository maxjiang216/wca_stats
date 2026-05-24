use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

const EXCLUDED: &[&str] = &[
    "333bf", "444bf", "555bf", "333mbf", "333mbo", "333fm", "333ft", "magic", "mmagic",
];
const TOP_N: usize = 1000;

#[derive(Serialize)]
struct RankEntry {
    pid: String,
    name: String,
    cid: String,
    est: f64,   // centiseconds
    n_comps: usize,
    last_date: String,
}

#[derive(Serialize)]
struct EventSkill {
    lambda_per_day: f64,
    rankings: Vec<RankEntry>,
}

#[derive(Serialize)]
struct MethodComparison {
    n_disagree: u32,
    ewma_score: f64,
    pb_score:   f64,
    /// EWMA share of the disagreements it or PB won (%).
    ewma_pct: f64,
}

fn ymd_to_jdn(year: u16, month: u8, day: u8) -> i32 {
    let y = year as i32;
    let m = month as i32;
    let d = day as i32;
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

struct PersonHistory {
    person_id: String,
    name: String,
    country: String,
    /// (jdn, mean_cs, n_solves) sorted ascending by jdn.
    comps: Vec<(i32, f64, u32)>,
}

/// Total weighted squared prediction error across all people with ≥ 2 competitions.
/// Weight = n_solves / mean_cs — proportional to solve count (more data = higher weight)
/// and inversely proportional to time (corrects for scale across events).
fn compute_loss(histories: &[PersonHistory], lambda: f64) -> f64 {
    let mut loss = 0.0_f64;
    for h in histories {
        if h.comps.len() < 2 {
            continue;
        }
        let mut mu = h.comps[0].1;
        let mut w = 1.0_f64;
        let mut prev_jdn = h.comps[0].0;
        for &(jdn, mean_cs, n_solves) in &h.comps[1..] {
            let dt = (jdn - prev_jdn) as f64;
            let w_eff = w * (-lambda * dt).exp();
            let err = mean_cs - mu;
            loss += err * err * (n_solves as f64 / mean_cs);
            mu = (w_eff * mu + mean_cs) / (w_eff + 1.0);
            w = w_eff + 1.0;
            prev_jdn = jdn;
        }
    }
    loss
}

/// Compare EWMA vs PB method for predicting competition final winners.
///
/// Only competitions where the two methods predict different winners are counted.
/// Weight = 1 / winner_avg so faster (more elite) competitions matter more.
///
/// Returns (ewma_weighted_score, pb_weighted_score, n_disagreements).
fn compare_methods(
    indices: &[usize],
    db: &WcaDb,
    comp_day: &HashMap<&str, i32>,
    histories: &[PersonHistory],
    lambda: f64,
) -> (f64, f64, u32) {
    // ── EWMA going in: person_id → sorted Vec<(jdn, µ_before)> ──────────────
    // Entries start at each person's 2nd competition (no prediction for 1st).
    let mut ewma_before: HashMap<String, Vec<(i32, f64)>> = HashMap::new();
    for h in histories {
        if h.comps.len() < 2 { continue; }
        let mut mu = h.comps[0].1;
        let mut w  = 1.0_f64;
        let mut prev_jdn = h.comps[0].0;
        let mut v = Vec::with_capacity(h.comps.len() - 1);
        for &(jdn, mean_cs, _) in &h.comps[1..] {
            v.push((jdn, mu));
            let w_eff = w * (-lambda * (jdn - prev_jdn) as f64).exp();
            mu = (w_eff * mu + mean_cs) / (w_eff + 1.0);
            w = w_eff + 1.0;
            prev_jdn = jdn;
        }
        ewma_before.insert(h.person_id.clone(), v);
    }
    // Vecs are already sorted by jdn (PersonHistory.comps is sorted).

    // ── Historical PB (official average) going in ────────────────────────────
    // best official average per (person, jdn) across all rounds.
    let mut comp_avgs: HashMap<(String, i32), f64> = HashMap::new();
    for &i in indices {
        let r = &db.results[i];
        if r.average <= 0 { continue; }
        let Some(&jdn) = comp_day.get(r.competition_id.as_str()) else { continue; };
        let e = comp_avgs.entry((r.person_id.clone(), jdn)).or_insert(f64::INFINITY);
        if (r.average as f64) < *e { *e = r.average as f64; }
    }
    let mut sorted_avgs: Vec<((String, i32), f64)> = comp_avgs.into_iter().collect();
    sorted_avgs.sort_unstable_by(|a, b| a.0 .0.cmp(&b.0 .0).then(a.0 .1.cmp(&b.0 .1)));

    // pb_best: person_id → sorted Vec<(jdn, pb_after_this_comp)>
    // pb_going_in to comp J = pb_after the last comp with jdn < J.
    let mut pb_best: HashMap<String, Vec<(i32, f64)>> = HashMap::new();
    {
        let mut cur_pid = String::new();
        let mut cur_pb = f64::INFINITY;
        for ((pid, jdn), avg) in &sorted_avgs {
            if *pid != cur_pid { cur_pid = pid.clone(); cur_pb = f64::INFINITY; }
            if *avg < cur_pb { cur_pb = *avg; }
            pb_best.entry(pid.clone()).or_default().push((*jdn, cur_pb));
        }
    }

    // ── Identify finals ──────────────────────────────────────────────────────
    let final_ids: std::collections::HashSet<&str> = db.round_types.values()
        .filter(|rt| rt.is_final != 0)
        .map(|rt| rt.id.as_str())
        .collect();

    // comp_id → Vec<(person_id, official_avg)> for all finalists with valid avg
    let mut finals: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for &i in indices {
        let r = &db.results[i];
        if !final_ids.contains(r.round_type_id.as_str()) { continue; }
        if r.average <= 0 { continue; }
        finals.entry(r.competition_id.clone())
            .or_default()
            .push((r.person_id.clone(), r.average as f64));
    }

    // ── Compare predictions ──────────────────────────────────────────────────
    let mut ewma_score = 0.0_f64;
    let mut pb_score   = 0.0_f64;
    let mut n_disagree = 0u32;

    for (comp_id, results) in &finals {
        let Some(&jdn) = comp_day.get(comp_id.as_str()) else { continue; };

        let (actual_winner, winner_avg) = results
            .iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();

        // EWMA predicted winner: finalist with the lowest µ going into this comp.
        // Only considers finalists who have an EWMA estimate (≥2 prior comps in event).
        let ewma_pred = results.iter().filter_map(|(pid, _)| {
            let v = ewma_before.get(pid)?;
            // Binary search for exact jdn match (this is a comp they participated in).
            let idx = v.partition_point(|&(j, _)| j < jdn);
            if idx < v.len() && v[idx].0 == jdn { Some((pid.as_str(), v[idx].1)) } else { None }
        }).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(pid, _)| pid);

        // PB predicted winner: finalist with the lowest PB going into this comp.
        // pb_going_in = pb_after the most recent prior comp with valid official avg.
        let pb_pred = results.iter().filter_map(|(pid, _)| {
            let v = pb_best.get(pid)?;
            let idx = v.partition_point(|&(j, _)| j < jdn); // first entry with jdn ≥ J
            if idx == 0 { return None; } // no prior comp
            let pb = v[idx - 1].1;
            if pb < f64::INFINITY { Some((pid.as_str(), pb)) } else { None }
        }).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(pid, _)| pid);

        let (Some(ewma_w), Some(pb_w)) = (ewma_pred, pb_pred) else { continue; };
        if ewma_w == pb_w { continue; } // methods agree → skip

        n_disagree += 1;
        let weight = 1.0 / winner_avg;
        if ewma_w == actual_winner.as_str()      { ewma_score += weight; }
        else if pb_w == actual_winner.as_str()   { pb_score   += weight; }
    }

    (ewma_score, pb_score, n_disagree)
}

fn ternary_search_lambda(histories: &[PersonHistory]) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 0.05_f64; // ~14-day half-life at the high end
    for _ in 0..100 {
        let m1 = lo + (hi - lo) / 3.0;
        let m2 = hi - (hi - lo) / 3.0;
        if compute_loss(histories, m1) <= compute_loss(histories, m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    (lo + hi) / 2.0
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let excluded: std::collections::HashSet<&str> = EXCLUDED.iter().copied().collect();
    let active: std::collections::HashSet<&str> = db.events.keys().map(String::as_str).collect();

    let comp_day: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    // Group result indices by event_id (single pass).
    let mut event_results: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in db.results.iter().enumerate() {
        let eid = r.event_id.as_str();
        if excluded.contains(eid) || !active.contains(eid) {
            continue;
        }
        event_results.entry(eid).or_default().push(i);
    }

    let mut all_events: HashMap<String, EventSkill> = HashMap::new();
    let mut comparisons: HashMap<String, MethodComparison> = HashMap::new();

    for (event_id, indices) in &event_results {
        // (person_id, comp_id) -> (sum_cs, count, name, country)
        let mut pc_map: HashMap<(String, String), (f64, u32, String, String)> = HashMap::new();

        for &i in indices {
            let r = &db.results[i];
            if comp_day.get(r.competition_id.as_str()).is_none() {
                continue;
            }

            // Mean of non-DNF individual solve times.
            let mut sum = 0.0_f64;
            let mut cnt = 0u32;
            if let Some(times) = db.attempts.get(&r.id) {
                for &v in times {
                    if v > 0 {
                        sum += v as f64;
                        cnt += 1;
                    }
                }
            }
            if cnt == 0 {
                // Fallback when attempt-level data is missing.
                if r.average > 0 {
                    sum = r.average as f64;
                    cnt = 1;
                } else if r.best > 0 {
                    sum = r.best as f64;
                    cnt = 1;
                } else {
                    continue;
                }
            }

            let entry = pc_map
                .entry((r.person_id.clone(), r.competition_id.clone()))
                .or_insert_with(|| (0.0, 0, r.person_name.clone(), r.person_country_id.clone()));
            entry.0 += sum;
            entry.1 += cnt;
        }

        // Group by person_id -> PersonHistory.
        let mut person_map: HashMap<String, PersonHistory> = HashMap::new();
        for ((person_id, comp_id), (sum, cnt, name, country)) in pc_map {
            let Some(&jdn) = comp_day.get(comp_id.as_str()) else {
                continue;
            };
            let mean_cs = sum / cnt as f64;
            let h = person_map.entry(person_id.clone()).or_insert_with(|| {
                // Prefer current name/country from db.persons if available.
                let (n, c) = db
                    .persons
                    .get(&person_id)
                    .map(|p| (p.name.clone(), p.country_id.clone()))
                    .unwrap_or_else(|| (name, country));
                PersonHistory { person_id: person_id.clone(), name: n, country: c, comps: Vec::new() }
            });
            h.comps.push((jdn, mean_cs, cnt));
        }

        let mut histories: Vec<PersonHistory> = person_map.into_values().collect();
        for h in &mut histories {
            h.comps.sort_unstable_by_key(|&(jdn, _, _)| jdn);
        }

        let lambda = ternary_search_lambda(&histories);

        // Compute final EWMA estimate for each person.
        let mut entries: Vec<RankEntry> = Vec::with_capacity(histories.len());
        for h in &histories {
            if h.comps.is_empty() {
                continue;
            }
            let mut mu = h.comps[0].1;
            let mut w = 1.0_f64;
            let mut prev_jdn = h.comps[0].0;
            for &(jdn, mean_cs, _) in &h.comps[1..] {
                let dt = (jdn - prev_jdn) as f64;
                let w_eff = w * (-lambda * dt).exp();
                mu = (w_eff * mu + mean_cs) / (w_eff + 1.0);
                w = w_eff + 1.0;
                prev_jdn = jdn;
            }
            entries.push(RankEntry {
                pid: h.person_id.clone(),
                name: h.name.clone(),
                cid: h.country.clone(),
                est: mu,
                n_comps: h.comps.len(),
                last_date: jdn_to_iso(h.comps.last().unwrap().0),
            });
        }

        entries.sort_unstable_by(|a, b| a.est.partial_cmp(&b.est).unwrap());
        entries.truncate(TOP_N);

        let half_life = if lambda > 0.0 { 2f64.ln() / lambda } else { f64::INFINITY };
        eprintln!(
            "  skill_estimator {event_id}: {} people, lambda={:.6}/day ({:.0}-day half-life)",
            entries.len(), lambda, half_life,
        );

        // ── Bias analysis ────────────────────────────────────────────────────
        // Re-simulate to collect every prediction and its outcome.
        // pred: the EWMA estimate going into the competition (the "prediction")
        // actual: observed mean_cs at the competition
        // career_days: jdn of this competition minus jdn of person's first competition
        struct PredPoint { pred: f64, actual: f64, career_days: i32 }
        let mut all_preds: Vec<PredPoint> = Vec::new();
        for h in &histories {
            if h.comps.len() < 2 { continue; }
            let first_jdn = h.comps[0].0;
            let mut mu = h.comps[0].1;
            let mut w  = 1.0_f64;
            let mut prev_jdn = h.comps[0].0;
            for &(jdn, mean_cs, _) in &h.comps[1..] {
                all_preds.push(PredPoint { pred: mu, actual: mean_cs, career_days: jdn - first_jdn });
                let w_eff = w * (-lambda * (jdn - prev_jdn) as f64).exp();
                mu = (w_eff * mu + mean_cs) / (w_eff + 1.0);
                w  = w_eff + 1.0;
                prev_jdn = jdn;
            }
        }

        // Relative error: (actual - pred) / pred
        // Positive = model predicted too fast (person actually slower than expected)
        // Negative = model predicted too slow (person actually faster = improved)

        // By speed: sort by pred, split into quintiles.
        all_preds.sort_unstable_by(|a, b| a.pred.partial_cmp(&b.pred).unwrap());
        let np = all_preds.len();
        eprintln!("    bias by speed ({np} total predictions):");
        let n_q = 5usize;
        for qi in 0..n_q {
            let lo = qi * np / n_q;
            let hi = (qi + 1) * np / n_q;
            let slice = &all_preds[lo..hi];
            let mean_pred = slice.iter().map(|p| p.pred).sum::<f64>() / slice.len() as f64;
            let mean_rel  = slice.iter().map(|p| (p.actual - p.pred) / p.pred).sum::<f64>() / slice.len() as f64;
            eprintln!("      Q{} (~{:.2}s): bias = {:+.2}%", qi + 1, mean_pred / 100.0, mean_rel * 100.0);
        }

        // By career age: fixed breakpoints in days.
        let age_breaks: &[(i32, &str)] = &[
            (90,   "<3 mo"),
            (180,  "3–6 mo"),
            (365,  "6–12 mo"),
            (730,  "1–2 yr"),
            (1825, "2–5 yr"),
            (i32::MAX, "5+ yr"),
        ];
        eprintln!("    bias by career age:");
        let mut age_lo = 0i32;
        for &(age_hi, label) in age_breaks {
            let slice: Vec<_> = all_preds.iter()
                .filter(|p| p.career_days >= age_lo && p.career_days < age_hi)
                .collect();
            if slice.is_empty() { age_lo = age_hi; continue; }
            let mean_rel = slice.iter().map(|p| (p.actual - p.pred) / p.pred).sum::<f64>() / slice.len() as f64;
            eprintln!("      {label:8} (n={:6}): bias = {:+.2}%", slice.len(), mean_rel * 100.0);
            age_lo = age_hi;
        }

        // ── Method comparison ─────────────────────────────────────────────────
        let (ewma_sc, pb_sc, n_dis) = compare_methods(indices, db, &comp_day, &histories, lambda);
        let total_scored = ewma_sc + pb_sc;
        let ewma_pct = if total_scored > 0.0 { ewma_sc / total_scored * 100.0 } else { 50.0 };
        eprintln!(
            "    method comparison: {n_dis} disagreements — EWMA {ewma_pct:.1}% vs PB {:.1}%  \
             (weighted scores: EWMA {ewma_sc:.4}, PB {pb_sc:.4})",
            100.0 - ewma_pct,
        );
        comparisons.insert(
            event_id.to_string(),
            MethodComparison { n_disagree: n_dis, ewma_score: ewma_sc, pb_score: pb_sc, ewma_pct },
        );

        all_events.insert(
            event_id.to_string(),
            EventSkill { lambda_per_day: lambda, rankings: entries },
        );
    }

    let path = format!("{out_dir}/skill_estimator.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &all_events)?;
    let cmp_path = format!("{out_dir}/skill_estimator_comparison.json");
    serde_json::to_writer(std::fs::File::create(&cmp_path)?, &comparisons)?;
    Ok(())
}
