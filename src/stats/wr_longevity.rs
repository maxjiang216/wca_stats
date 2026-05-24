use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

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

#[derive(Serialize)]
pub struct WrEntry {
    pub name: String,
    pub pid: String,
    pub value: i32,
    pub date: String,
    pub comp: String,
    /// Days this result was in the top-k (up to today if still there).
    pub top10_days: i32,
    /// True if still in top 10 as of the latest competition in the database.
    pub top10_current: bool,
    pub top100_days: i32,
    pub top100_current: bool,
}

/// For each value, track how many persons have that as their personal best.
fn get_kth(sorted_vals: &BTreeMap<i32, u32>, k: usize) -> Option<i32> {
    let mut cnt = 0usize;
    for (&v, &c) in sorted_vals.iter() {
        cnt += c as usize;
        if cnt >= k {
            return Some(v);
        }
    }
    None
}

/// Build a timeline of when the k-th best personal best changed.
/// Input must be sorted by jdn.
/// Returns Vec<(jdn, kth_value)> — one entry per change in the kth-best value.
fn kth_best_timeline(all_results: &[(i32, String, i32)], k: usize) -> Vec<(i32, i32)> {
    let mut person_best: HashMap<String, i32> = HashMap::new();
    let mut sorted_vals: BTreeMap<i32, u32> = BTreeMap::new();
    let mut timeline: Vec<(i32, i32)> = Vec::new();
    let mut prev_kth: Option<i32> = None;

    for (jdn, pid, value) in all_results {
        let (jdn, value) = (*jdn, *value);
        let old_best = person_best.get(pid).copied();
        if old_best.map_or(true, |ob| value < ob) {
            if let Some(ob) = old_best {
                let cnt = sorted_vals.get_mut(&ob).unwrap();
                if *cnt == 1 {
                    sorted_vals.remove(&ob);
                } else {
                    *cnt -= 1;
                }
            }
            *sorted_vals.entry(value).or_insert(0) += 1;
            person_best.insert(pid.clone(), value);

            let new_kth = get_kth(&sorted_vals, k);
            if new_kth != prev_kth {
                if let Some(kth) = new_kth {
                    // Overwrite if same day (multiple updates on one day → keep last).
                    match timeline.last_mut() {
                        Some(last) if last.0 == jdn => {
                            last.1 = kth;
                        }
                        _ => {
                            timeline.push((jdn, kth));
                        }
                    }
                }
                prev_kth = new_kth;
            }
        }
    }
    timeline
}

/// Returns (days_in_top_k, is_still_current).
/// `is_still_current` = true if the value is still in top-k as of today_jdn.
fn find_days(timeline: &[(i32, i32)], wr_jdn: i32, wr_value: i32, today_jdn: i32) -> (i32, bool) {
    // Skip all entries up to and including the WR date — start scanning after.
    let start = timeline.partition_point(|&(j, _)| j <= wr_jdn);
    for &(jdn, kth_val) in &timeline[start..] {
        if kth_val < wr_value {
            return (jdn - wr_jdn, false);
        }
    }
    (today_jdn - wr_jdn, true)
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let comp_day: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    // Use the actual current date, not max(comp_day) — the WCA database contains
    // future competitions that are pre-registered, which would inflate durations.
    let today_jdn = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        2440588 + (secs / 86400) as i32 // JDN of 1970-01-01 + days since epoch
    };

    // (event_id) → Vec<(jdn, person_id, value)>
    let mut event_singles: HashMap<String, Vec<(i32, String, i32)>> = HashMap::new();
    let mut event_avgs: HashMap<String, Vec<(i32, String, i32)>> = HashMap::new();
    // (event_id) → Vec<(jdn, value, name, pid, comp_id)>
    let mut event_wr_singles: HashMap<String, Vec<(i32, i32, String, String, String)>> =
        HashMap::new();
    let mut event_wr_avgs: HashMap<String, Vec<(i32, i32, String, String, String)>> =
        HashMap::new();

    for r in &db.results {
        let Some(&jdn) = comp_day.get(r.competition_id.as_str()) else {
            continue;
        };

        if r.best > 0 {
            event_singles
                .entry(r.event_id.clone())
                .or_default()
                .push((jdn, r.person_id.clone(), r.best));
            if r.regional_single_record.as_deref() == Some("WR") {
                event_wr_singles
                    .entry(r.event_id.clone())
                    .or_default()
                    .push((jdn, r.best, r.person_name.clone(), r.person_id.clone(), r.competition_id.clone()));
            }
        }

        if r.average > 0 {
            event_avgs
                .entry(r.event_id.clone())
                .or_default()
                .push((jdn, r.person_id.clone(), r.average));
            if r.regional_average_record.as_deref() == Some("WR") {
                event_wr_avgs
                    .entry(r.event_id.clone())
                    .or_default()
                    .push((jdn, r.average, r.person_name.clone(), r.person_id.clone(), r.competition_id.clone()));
            }
        }
    }

    for v in event_singles.values_mut() {
        v.sort_unstable_by_key(|&(jdn, _, _)| jdn);
    }
    for v in event_avgs.values_mut() {
        v.sort_unstable_by_key(|&(jdn, _, _)| jdn);
    }
    for v in event_wr_singles.values_mut() {
        v.sort_unstable_by_key(|&(jdn, ..)| jdn);
    }
    for v in event_wr_avgs.values_mut() {
        v.sort_unstable_by_key(|&(jdn, ..)| jdn);
    }

    let compute = |all: &[(i32, String, i32)],
                   wrs: &[(i32, i32, String, String, String)]|
     -> Vec<WrEntry> {
        let tl10 = kth_best_timeline(all, 10);
        let tl100 = kth_best_timeline(all, 100);
        wrs.iter()
            .map(|(wr_jdn, wr_value, name, pid, comp)| {
                let (top10_days, top10_current) =
                    find_days(&tl10, *wr_jdn, *wr_value, today_jdn);
                let (top100_days, top100_current) =
                    find_days(&tl100, *wr_jdn, *wr_value, today_jdn);
                WrEntry {
                    name: name.clone(),
                    pid: pid.clone(),
                    value: *wr_value,
                    date: jdn_to_iso(*wr_jdn),
                    comp: comp.clone(),
                    top10_days,
                    top10_current,
                    top100_days,
                    top100_current,
                }
            })
            .collect()
    };

    let wca_order = [
        "333", "222", "444", "555", "666", "777", "333bf", "333fm", "333oh", "clock", "minx",
        "pyram", "skewb", "sq1", "444bf", "555bf", "333mbf", "333mbo", "333ft", "magic", "mmagic",
    ];
    let mut event_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    for eid in event_wr_singles.keys().chain(event_wr_avgs.keys()) {
        event_set.insert(eid.clone());
    }
    let mut events: Vec<String> = wca_order
        .iter()
        .filter(|e| event_set.contains(**e))
        .map(|e| e.to_string())
        .collect();
    for e in &event_set {
        if !wca_order.contains(&e.as_str()) {
            events.push(e.clone());
        }
    }

    let mut single_out: HashMap<String, Vec<WrEntry>> = HashMap::new();
    let mut avg_out: HashMap<String, Vec<WrEntry>> = HashMap::new();

    for (eid, wrs) in &event_wr_singles {
        let all = event_singles.get(eid).map(Vec::as_slice).unwrap_or(&[]);
        let entries = compute(all, wrs);
        if !entries.is_empty() {
            single_out.insert(eid.clone(), entries);
        }
    }
    for (eid, wrs) in &event_wr_avgs {
        let all = event_avgs.get(eid).map(Vec::as_slice).unwrap_or(&[]);
        let entries = compute(all, wrs);
        if !entries.is_empty() {
            avg_out.insert(eid.clone(), entries);
        }
    }

    eprintln!(
        "  wr_longevity: {} events with single WRs, {} with average WRs",
        single_out.len(),
        avg_out.len()
    );

    #[derive(Serialize)]
    struct Output {
        events: Vec<String>,
        single: HashMap<String, Vec<WrEntry>>,
        average: HashMap<String, Vec<WrEntry>>,
    }

    let out = Output { events, single: single_out, average: avg_out };
    let path = format!("{out_dir}/wr_longevity.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &out)?;
    Ok(())
}
