//! MBLD (Multi-Blind) derived rankings.
//!
//! mbld_mean    – mean points across 3 valid Bo3 attempts (333mbf only, format '3').
//! mbld_perfect – best single attempt where missed == 0 (all cubes solved).
//! mbld_solved  – best single attempt ranked by cubes solved only (no miss penalty).

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::{models::RawResult, WcaDb};

/// Decode a WCA MBLD encoded attempt.
/// Returns (points, time_s, missed, solved, attempted) or None if DNF/DNS/invalid.
#[inline]
fn decode(value: i32) -> Option<(i32, u32, u32, u32, u32)> {
    if value <= 0 {
        return None;
    }
    let missed = (value % 100) as u32;
    let time_s = ((value / 100) % 100000) as u32;
    let points = 99 - (value / 10_000_000);
    if points <= 0 {
        return None;
    }
    let solved = points as u32 + missed;
    let attempted = solved + missed;
    Some((points, time_s, missed, solved, attempted))
}

fn end_date(db: &WcaDb, r: &RawResult) -> u32 {
    db.competitions
        .get(r.competition_id.as_str())
        .map(|c| {
            (c.end_year as u32) * 10_000
                + (c.end_month as u32) * 100
                + c.end_day as u32
        })
        .unwrap_or(u32::MAX)
}

fn person_name<'a>(db: &'a WcaDb, r: &'a RawResult) -> &'a str {
    db.persons
        .get(r.person_id.as_str())
        .map(|p| p.name.as_str())
        .unwrap_or(r.person_name.as_str())
}

#[derive(Debug, Serialize)]
pub struct SingleEntry {
    pub rank: usize,
    pub person_id: String,
    pub person_name: String,
    pub country_id: String,
    pub competition_id: String,
    pub solved: u32,
    pub attempted: u32,
    pub time_s: u32,
}

#[derive(Debug, Serialize)]
pub struct MeanEntry {
    pub rank: usize,
    pub person_id: String,
    pub person_name: String,
    pub country_id: String,
    pub competition_id: String,
    /// Sum of points across 3 attempts; mean = points_sum / 3.
    pub points_sum: i32,
    pub time_total_s: u32,
}

/// First index ≥ 1000 where the value changes, or rows_len if no such index.
fn cutoff_at_1000(rows_len: usize, is_same_rank: impl Fn(usize) -> bool) -> usize {
    if rows_len <= 1000 {
        return rows_len;
    }
    let mut i = 1000;
    while i < rows_len && is_same_rank(i) {
        i += 1;
    }
    i
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    // event → person → best value
    // perfect: (encoded_value, result)  — lower encoded = better (higher pts, lower time)
    // solved:  (solved, time_s, attempted, result)
    // mean:    (points_sum, time_total_s, result)
    let mut perfect_slot: HashMap<&str, HashMap<&str, (i32, &RawResult)>> = HashMap::new();
    let mut solved_slot: HashMap<&str, HashMap<&str, (u32, u32, u32, &RawResult)>> =
        HashMap::new();
    let mut mean_slot: HashMap<&str, HashMap<&str, (i32, u32, &RawResult)>> = HashMap::new();

    for result in &db.results {
        let is_mbld = matches!(result.event_id.as_str(), "333mbf" | "333mbo");
        if !is_mbld {
            continue;
        }

        let Some(attempts) = db.attempts.get(&result.id) else {
            continue;
        };

        // perfect and solved: inspect each individual attempt
        for &v in attempts {
            let Some((_, time_s, missed, solved, attempted)) = decode(v) else {
                continue;
            };

            if missed == 0 {
                let e = perfect_slot
                    .entry(result.event_id.as_str())
                    .or_default()
                    .entry(result.person_id.as_str())
                    .or_insert((i32::MAX, result));
                if v < e.0 {
                    *e = (v, result);
                }
            }

            {
                let e = solved_slot
                    .entry(result.event_id.as_str())
                    .or_default()
                    .entry(result.person_id.as_str())
                    .or_insert((0, u32::MAX, 0, result));
                if solved > e.0 || (solved == e.0 && time_s < e.1) {
                    *e = (solved, time_s, attempted, result);
                }
            }
        }

        // mean: only 333mbf Bo3, all 3 attempts must be valid
        if result.event_id == "333mbf"
            && result.format_id == "3"
            && attempts.len() == 3
        {
            let decoded: Option<Vec<_>> =
                attempts.iter().map(|&v| decode(v)).collect();
            if let Some(d) = decoded {
                let points_sum: i32 = d.iter().map(|(p, ..)| p).sum();
                let time_total_s: u32 = d.iter().map(|(_, t, ..)| t).sum();

                let e = mean_slot
                    .entry(result.event_id.as_str())
                    .or_default()
                    .entry(result.person_id.as_str())
                    .or_insert((i32::MIN, u32::MAX, result));
                if points_sum > e.0 || (points_sum == e.0 && time_total_s < e.1) {
                    *e = (points_sum, time_total_s, result);
                }
            }
        }
    }

    // perfect rankings
    {
        let mut rankings: HashMap<String, Vec<SingleEntry>> = HashMap::new();
        for (event_id, person_map) in perfect_slot {
            let mut rows: Vec<(i32, &RawResult)> =
                person_map.into_values().collect();
            rows.sort_unstable_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| end_date(db, a.1).cmp(&end_date(db, b.1)))
                    .then_with(|| a.1.person_id.cmp(&b.1.person_id))
            });
            let n = cutoff_at_1000(rows.len(), |i| rows[i].0 == rows[i - 1].0);
            let rows = &rows[..n];
            let mut entries = Vec::with_capacity(rows.len());
            let mut rank = 1;
            for (i, (v, r)) in rows.iter().enumerate() {
                if i > 0 && *v != rows[i - 1].0 {
                    rank = i + 1;
                }
                let (_, time_s, _, solved, attempted) = decode(*v).unwrap();
                entries.push(SingleEntry {
                    rank,
                    person_id: r.person_id.clone(),
                    person_name: person_name(db, r).to_owned(),
                    country_id: r.person_country_id.clone(),
                    competition_id: r.competition_id.clone(),
                    solved,
                    attempted,
                    time_s,
                });
            }
            rankings.insert(event_id.to_owned(), entries);
        }
        print_events("mbld_perfect", &rankings);
        serde_json::to_writer(
            std::fs::File::create(format!("{out_dir}/mbld_perfect.json"))?,
            &rankings,
        )?;
    }

    // solved rankings
    {
        let mut rankings: HashMap<String, Vec<SingleEntry>> = HashMap::new();
        for (event_id, person_map) in solved_slot {
            let mut rows: Vec<(u32, u32, u32, &RawResult)> =
                person_map.into_values().collect();
            rows.sort_unstable_by(|a, b| {
                // higher solved first, then lower time_s
                b.0.cmp(&a.0)
                    .then_with(|| a.1.cmp(&b.1))
                    .then_with(|| end_date(db, a.3).cmp(&end_date(db, b.3)))
                    .then_with(|| a.3.person_id.cmp(&b.3.person_id))
            });
            let n = cutoff_at_1000(rows.len(), |i| {
                rows[i].0 == rows[i - 1].0 && rows[i].1 == rows[i - 1].1
            });
            let rows = &rows[..n];
            let mut entries = Vec::with_capacity(rows.len());
            let mut rank = 1;
            for (i, (sv, tv, av, r)) in rows.iter().enumerate() {
                if i > 0
                    && !(rows[i - 1].0 == *sv && rows[i - 1].1 == *tv)
                {
                    rank = i + 1;
                }
                entries.push(SingleEntry {
                    rank,
                    person_id: r.person_id.clone(),
                    person_name: person_name(db, r).to_owned(),
                    country_id: r.person_country_id.clone(),
                    competition_id: r.competition_id.clone(),
                    solved: *sv,
                    attempted: *av,
                    time_s: *tv,
                });
            }
            rankings.insert(event_id.to_owned(), entries);
        }
        print_events("mbld_solved", &rankings);
        serde_json::to_writer(
            std::fs::File::create(format!("{out_dir}/mbld_solved.json"))?,
            &rankings,
        )?;
    }

    // mean rankings
    {
        let mut rankings: HashMap<String, Vec<MeanEntry>> = HashMap::new();
        for (event_id, person_map) in mean_slot {
            let mut rows: Vec<(i32, u32, &RawResult)> =
                person_map.into_values().collect();
            rows.sort_unstable_by(|a, b| {
                // higher points_sum first, then lower time_total_s
                b.0.cmp(&a.0)
                    .then_with(|| a.1.cmp(&b.1))
                    .then_with(|| end_date(db, a.2).cmp(&end_date(db, b.2)))
                    .then_with(|| a.2.person_id.cmp(&b.2.person_id))
            });
            let n = cutoff_at_1000(rows.len(), |i| {
                rows[i].0 == rows[i - 1].0 && rows[i].1 == rows[i - 1].1
            });
            let rows = &rows[..n];
            let mut entries = Vec::with_capacity(rows.len());
            let mut rank = 1;
            for (i, (ps, ts, r)) in rows.iter().enumerate() {
                if i > 0
                    && !(rows[i - 1].0 == *ps && rows[i - 1].1 == *ts)
                {
                    rank = i + 1;
                }
                entries.push(MeanEntry {
                    rank,
                    person_id: r.person_id.clone(),
                    person_name: person_name(db, r).to_owned(),
                    country_id: r.person_country_id.clone(),
                    competition_id: r.competition_id.clone(),
                    points_sum: *ps,
                    time_total_s: *ts,
                });
            }
            rankings.insert(event_id.to_owned(), entries);
        }
        print_events("mbld_mean", &rankings);
        serde_json::to_writer(
            std::fs::File::create(format!("{out_dir}/mbld_mean.json"))?,
            &rankings,
        )?;
    }

    Ok(())
}

fn print_events<T>(name: &str, rankings: &HashMap<String, Vec<T>>) {
    let mut events: Vec<&String> = rankings.keys().collect();
    events.sort();
    eprint!("  {name} — ");
    for eid in events {
        eprint!("{eid}  ");
    }
    eprintln!();
}
