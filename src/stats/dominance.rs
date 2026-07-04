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

/// Fenwick tree over compressed value indices, counting inserted results.
struct Bit {
    t: Vec<i64>,
}
impl Bit {
    fn new(n: usize) -> Self {
        Bit { t: vec![0; n + 1] }
    }
    fn add(&mut self, i: usize, delta: i64) {
        let mut i = i + 1;
        while i < self.t.len() {
            self.t[i] += delta;
            i += i & i.wrapping_neg();
        }
    }
    /// Sum over indices [0, i).
    fn prefix(&self, i: usize) -> i64 {
        let mut s = 0;
        let mut i = i;
        while i > 0 {
            s += self.t[i];
            i -= i & i.wrapping_neg();
        }
        s
    }
}

/// k-th smallest entity value from a multiset (value -> #entities at value).
fn kth_value(ms: &BTreeMap<i32, u32>, k: usize) -> Option<i32> {
    let mut c = 0usize;
    for (&v, &cnt) in ms {
        c += cnt as usize;
        if c >= k {
            return Some(v);
        }
    }
    None
}

#[derive(Serialize)]
struct Point {
    date: String,
    /// Results faster than the 2nd-best person's PB (all held by the #1 person).
    top1: u32,
    /// Results faster than the 3rd-best person's PB (held by the top 2 people).
    top2: u32,
    /// Results faster than the 2nd-best country's best (held by one country).
    country: u32,
}

/// (jdn, person_idx, country_idx, value), one per result.
type Row = (i32, u32, u32, i32);

/// Build the weekly dominance-gap series for one event/type stream.
fn build_series(mut rows: Vec<Row>, today_jdn: i32) -> Vec<Point> {
    if rows.is_empty() {
        return Vec::new();
    }
    rows.sort_unstable_by_key(|&(j, ..)| j);

    // Coordinate-compress result values for the Fenwick count-below queries.
    let mut vals: Vec<i32> = rows.iter().map(|&(_, _, _, v)| v).collect();
    vals.sort_unstable();
    vals.dedup();

    let mut bit = Bit::new(vals.len());
    let mut total: u32 = 0;
    // Multisets of entity bests, keyed by value, for order-statistic thresholds.
    let mut person_best: HashMap<u32, i32> = HashMap::new();
    let mut person_ms: BTreeMap<i32, u32> = BTreeMap::new();
    let mut country_best: HashMap<u32, i32> = HashMap::new();
    let mut country_ms: BTreeMap<i32, u32> = BTreeMap::new();

    let bump = |ms: &mut BTreeMap<i32, u32>, v: i32, delta: i32| {
        if delta > 0 {
            *ms.entry(v).or_insert(0) += 1;
        } else if let Some(c) = ms.get_mut(&v) {
            if *c <= 1 {
                ms.remove(&v);
            } else {
                *c -= 1;
            }
        }
    };

    let first = rows[0].0;
    let mut p = 0usize;
    let mut points: Vec<Point> = Vec::new();
    let mut d = first;
    while d <= today_jdn {
        while p < rows.len() && rows[p].0 <= d {
            let (_, pidx, cidx, v) = rows[p];
            // person best
            let pe = person_best.entry(pidx).or_insert(i32::MAX);
            if v < *pe {
                if *pe != i32::MAX {
                    bump(&mut person_ms, *pe, -1);
                }
                bump(&mut person_ms, v, 1);
                *pe = v;
            }
            // country best
            let ce = country_best.entry(cidx).or_insert(i32::MAX);
            if v < *ce {
                if *ce != i32::MAX {
                    bump(&mut country_ms, *ce, -1);
                }
                bump(&mut country_ms, v, 1);
                *ce = v;
            }
            // result value into Fenwick
            let oi = vals.binary_search(&v).unwrap();
            bit.add(oi, 1);
            total += 1;
            p += 1;
        }

        // count results strictly faster than a threshold (None => all so far).
        let count_below = |thr: Option<i32>| -> u32 {
            match thr {
                Some(t) => bit.prefix(vals.partition_point(|&x| x < t)) as u32,
                None => total,
            }
        };
        let top1 = count_below(kth_value(&person_ms, 2));
        let top2 = count_below(kth_value(&person_ms, 3));
        let country = count_below(kth_value(&country_ms, 2));

        points.push(Point {
            date: jdn_to_iso(d),
            top1,
            top2,
            country,
        });
        d += 7;
    }

    // Compress: drop interior samples equal to their predecessor (step lines).
    let n = points.len();
    let mut out: Vec<Point> = Vec::with_capacity(n);
    for (i, pt) in points.into_iter().enumerate() {
        let keep = i == 0
            || i == n - 1
            || out
                .last()
                .map_or(true, |l| l.top1 != pt.top1 || l.top2 != pt.top2 || l.country != pt.country);
        if keep {
            out.push(pt);
        }
    }
    out
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let person_ids: Vec<&str> = db.persons.keys().map(String::as_str).collect();
    let person_idx: HashMap<&str, u32> = person_ids
        .iter()
        .enumerate()
        .map(|(i, &s)| (s, i as u32))
        .collect();

    let comp_day: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    let today_jdn = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        2440588 + (secs / 86400) as i32
    };

    // (event_id) -> single/average rows.
    let mut single_rows: HashMap<String, Vec<Row>> = HashMap::new();
    let mut avg_rows: HashMap<String, Vec<Row>> = HashMap::new();
    let mut ci_map: HashMap<String, u32> = HashMap::new();
    let mut intern_country = |c: &str| -> u32 {
        if let Some(&i) = ci_map.get(c) {
            i
        } else {
            let i = ci_map.len() as u32;
            ci_map.insert(c.to_string(), i);
            i
        }
    };

    for r in &db.results {
        let Some(&jdn) = comp_day.get(r.competition_id.as_str()) else {
            continue;
        };
        let Some(&pidx) = person_idx.get(r.person_id.as_str()) else {
            continue;
        };
        let cidx = intern_country(r.person_country_id.as_str());
        if r.best > 0 {
            single_rows
                .entry(r.event_id.clone())
                .or_default()
                .push((jdn, pidx, cidx, r.best));
        }
        if r.average > 0 {
            avg_rows
                .entry(r.event_id.clone())
                .or_default()
                .push((jdn, pidx, cidx, r.average));
        }
    }

    let wca_order = [
        "333", "222", "444", "555", "666", "777", "333bf", "333fm", "333oh", "clock", "minx",
        "pyram", "skewb", "sq1", "444bf", "555bf", "333mbf",
    ];
    let mut event_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    for eid in single_rows.keys().chain(avg_rows.keys()) {
        event_set.insert(eid.clone());
    }
    let mut events: Vec<String> = wca_order
        .iter()
        .filter(|e| event_set.contains(**e))
        .map(|e| e.to_string())
        .collect();

    let mut single_out: HashMap<String, Vec<Point>> = HashMap::new();
    let mut avg_out: HashMap<String, Vec<Point>> = HashMap::new();
    for e in &events {
        if let Some(rows) = single_rows.remove(e) {
            let s = build_series(rows, today_jdn);
            if !s.is_empty() {
                single_out.insert(e.clone(), s);
            }
        }
        if let Some(rows) = avg_rows.remove(e) {
            let s = build_series(rows, today_jdn);
            if !s.is_empty() {
                avg_out.insert(e.clone(), s);
            }
        }
    }
    // keep only events with at least one series
    events.retain(|e| single_out.contains_key(e) || avg_out.contains_key(e));

    eprintln!(
        "  dominance: {} events ({} single, {} average series)",
        events.len(),
        single_out.len(),
        avg_out.len()
    );

    #[derive(Serialize)]
    struct Output {
        events: Vec<String>,
        single: HashMap<String, Vec<Point>>,
        average: HashMap<String, Vec<Point>>,
    }

    let out = Output {
        events,
        single: single_out,
        average: avg_out,
    };
    let path = format!("{out_dir}/dominance.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &out)?;
    Ok(())
}
