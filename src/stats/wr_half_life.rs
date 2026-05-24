use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

#[derive(Serialize)]
pub struct DataPoint {
    pub date: String,
    pub half_life: f64, // days
    pub n_wr: u32,
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

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    const INF: i32 = i32::MAX;

    // Only active events (excludes retired events like "magic", "mmagic").
    let active: std::collections::HashSet<&str> =
        db.events.keys().map(String::as_str).collect();

    // Competition start-date lookup.
    let comp_day: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    // Collect WR results: (event_id, is_avg) -> Vec<(jdn, value)>.
    let mut raw: HashMap<(String, bool), Vec<(i32, i32)>> = HashMap::new();

    for r in &db.results {
        if !active.contains(r.event_id.as_str()) {
            continue;
        }
        let Some(&date) = comp_day.get(r.competition_id.as_str()) else {
            continue;
        };
        if r.regional_single_record.as_deref() == Some("WR") && r.best > 0 {
            raw.entry((r.event_id.clone(), false))
                .or_default()
                .push((date, r.best));
        }
        if r.regional_average_record.as_deref() == Some("WR") && r.average > 0 {
            raw.entry((r.event_id.clone(), true))
                .or_default()
                .push((date, r.average));
        }
    }

    // Build WR timelines: Vec of (date_set, date_broken) pairs per (event, type).
    // For each event/type, the sequence of WRs is strictly improving.
    // date_broken = INF means the WR is still current.
    let mut timelines: Vec<Vec<(i32, i32)>> = Vec::new();

    for (_, mut records) in raw {
        records.sort_unstable(); // (date, value) ascending

        // Best value per date (multiple WRs in same comp → keep best).
        let mut by_date: Vec<(i32, i32)> = Vec::new();
        let mut i = 0;
        while i < records.len() {
            let date = records[i].0;
            let mut best = records[i].1;
            while i < records.len() && records[i].0 == date {
                if records[i].1 < best {
                    best = records[i].1;
                }
                i += 1;
            }
            by_date.push((date, best));
        }

        // Build strictly improving WR sequence.
        let mut timeline: Vec<(i32, i32)> = Vec::new();
        let mut cur_best = i32::MAX;
        for (date, val) in by_date {
            if val < cur_best {
                if let Some(last) = timeline.last_mut() {
                    last.1 = date; // close previous WR at this date
                }
                timeline.push((date, INF));
                cur_best = val;
            }
        }
        if !timeline.is_empty() {
            timelines.push(timeline);
        }
    }

    eprintln!(
        "  wr_half_life: {} event×type timelines",
        timelines.len()
    );

    // Sample weekly from first to last competition day.
    let first_day = comp_day.values().copied().min().unwrap_or(0);
    let last_day = comp_day.values().copied().max().unwrap_or(0);

    let mut points: Vec<DataPoint> = Vec::new();
    let mut d = first_day;

    while d <= last_day {
        let mut remaining: Vec<i32> = Vec::new();

        for tl in &timelines {
            // Binary search: find the latest WR set on or before d.
            let idx = tl.partition_point(|&(ds, _)| ds <= d);
            if idx == 0 {
                continue; // This event had no WR by date d.
            }
            let (_, date_broken) = tl[idx - 1];
            remaining.push(if date_broken == INF {
                INF
            } else {
                date_broken - d
            });
        }

        let n = remaining.len();
        if n > 0 {
            remaining.sort_unstable();
            // Index of the ⌈n/2⌉-th element (0-indexed) = (n-1)/2.
            let half_idx = (n - 1) / 2;
            let hl = remaining[half_idx];
            if hl != INF {
                points.push(DataPoint {
                    date: jdn_to_iso(d),
                    half_life: hl as f64,
                    n_wr: n as u32,
                });
            }
        }

        d += 7;
    }

    eprintln!("  wr_half_life: {} data points", points.len());
    let path = format!("{out_dir}/wr_half_life.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &points)?;
    Ok(())
}
