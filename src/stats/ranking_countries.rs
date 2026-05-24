//! Pre-compute cumulative country percentages at log-sampled rank positions for every event.
//! Uses average-of-5 world rankings; falls back to single for events with no average (e.g. 333mbf).
//! Outputs china_all.json.

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

const NUM_POINTS: usize = 2000;

#[derive(Serialize)]
struct Point {
    rank: u32,
    china: f64,
    usa: f64,
}

#[derive(Serialize)]
struct EventData {
    total: usize,
    points: Vec<Point>,
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    // event_id → vec of (world_rank, is_china, is_usa)
    let mut by_event: HashMap<&str, Vec<(u32, bool, bool)>> = HashMap::new();

    for ((person_id, event_id), rank) in &db.ranks_average {
        // MBLD has no official average ranking; skip so we fall back to single below.
        if matches!(event_id.as_str(), "333mbf" | "333mbo") {
            continue;
        }
        let country = db
            .persons
            .get(person_id.as_str())
            .map(|p| p.country_id.as_str())
            .unwrap_or("");
        by_event
            .entry(event_id.as_str())
            .or_default()
            .push((rank.world_rank, country == "China", country == "USA"));
    }

    // Snapshot which events are covered by average rankings before mutating by_event further.
    let avg_covered: std::collections::HashSet<&str> = by_event.keys().copied().collect();

    // For events with no average ranking, fall back to single rankings.
    for ((person_id, event_id), rank) in &db.ranks_single {
        if avg_covered.contains(event_id.as_str()) {
            continue;
        }
        let country = db
            .persons
            .get(person_id.as_str())
            .map(|p| p.country_id.as_str())
            .unwrap_or("");
        by_event
            .entry(event_id.as_str())
            .or_default()
            .push((rank.world_rank, country == "China", country == "USA"));
    }

    let mut all_events: HashMap<String, EventData> = HashMap::new();

    for (event_id, mut ranks) in by_event {
        ranks.sort_unstable_by_key(|(r, _, _)| *r);
        let n = ranks.len();

        let mut china_cum = vec![0u32; n + 1];
        let mut usa_cum   = vec![0u32; n + 1];
        for (i, (_, is_china, is_usa)) in ranks.iter().enumerate() {
            china_cum[i + 1] = china_cum[i] + u32::from(*is_china);
            usa_cum[i + 1]   = usa_cum[i]   + u32::from(*is_usa);
        }

        let mut points: Vec<Point> = Vec::new();
        let mut last_rank = 0u32;
        for step in 0..NUM_POINTS {
            let t = step as f64 / (NUM_POINTS - 1) as f64;
            let rank = ((n as f64).powf(t)).round() as usize;
            let rank = rank.clamp(1, n);
            if rank as u32 != last_rank {
                last_rank = rank as u32;
                points.push(Point {
                    rank: rank as u32,
                    china: china_cum[rank] as f64 / rank as f64 * 100.0,
                    usa:   usa_cum[rank]   as f64 / rank as f64 * 100.0,
                });
            }
        }

        all_events.insert(event_id.to_owned(), EventData { total: n, points });
    }

    let mut event_ids: Vec<&String> = all_events.keys().collect();
    event_ids.sort();
    eprint!("  china_all — {} events: ", all_events.len());
    for eid in &event_ids {
        eprint!("{eid}  ");
    }
    eprintln!();

    let path = format!("{out_dir}/china_all.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &all_events)?;

    Ok(())
}
