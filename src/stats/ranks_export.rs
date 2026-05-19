//! Export per-person per-event rank and PB data for client-side Sum of Ranks and KinchRanks.
//! Outputs all_ranks.json — persons who appear in top 2000 for any current event.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

/// Current WCA events (no magic, mmagic, feet, 333mbo).
pub const EVENTS: &[&str] = &[
    "222", "333", "444", "555", "666", "777",
    "333oh", "333bf", "333fm", "333mbf",
    "444bf", "555bf",
    "clock", "minx", "pyram", "skewb", "sq1",
];

/// Events that have no official WCA average ranking.
const NO_AVG: &[&str] = &["333mbf"];

const TOP_N: u32 = 2000;

#[derive(Serialize)]
pub struct PersonData {
    pub id: String,
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "c")]
    pub country: String,
    /// Single rank per event, 0 = not done (EVENTS order).
    pub sr: Vec<u32>,
    /// Average rank per event, 0 = not done or event has no avg ranking.
    pub ar: Vec<u32>,
    /// Single PB value per event, 0 = not done.
    pub ps: Vec<i32>,
    /// Average PB value per event, 0 = not done or no avg ranking.
    pub pa: Vec<i32>,
}

#[derive(Serialize)]
pub struct AllRanksData {
    pub events: Vec<String>,
    /// Total single-ranked persons per event.
    pub total_s: Vec<u32>,
    /// Total avg-ranked persons per event (0 for events with no avg).
    pub total_a: Vec<u32>,
    /// WR single value per event (0 if none).
    pub wr_s: Vec<i32>,
    /// WR average value per event (0 if none).
    pub wr_a: Vec<i32>,
    pub persons: Vec<PersonData>,
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let n = EVENTS.len();
    let event_idx: HashMap<&str, usize> = EVENTS.iter().enumerate().map(|(i, &e)| (e, i)).collect();
    let no_avg: HashSet<&str> = NO_AVG.iter().copied().collect();

    // Phase 1: compute per-event totals and WRs from the full tables.
    let mut total_s = vec![0u32; n];
    let mut total_a = vec![0u32; n];
    let mut wr_s = vec![0i32; n];
    let mut wr_a = vec![0i32; n];

    for ((_, event_id), rank) in &db.ranks_single {
        if let Some(&idx) = event_idx.get(event_id.as_str()) {
            total_s[idx] += 1;
            if rank.world_rank == 1 {
                wr_s[idx] = rank.best;
            }
        }
    }
    for ((_, event_id), rank) in &db.ranks_average {
        if no_avg.contains(event_id.as_str()) {
            continue;
        }
        if let Some(&idx) = event_idx.get(event_id.as_str()) {
            total_a[idx] += 1;
            if rank.world_rank == 1 {
                wr_a[idx] = rank.best;
            }
        }
    }

    // Phase 2: collect the pool of persons who appear in top N for any current event.
    let mut pool: HashSet<&str> = HashSet::new();
    for ((person_id, event_id), rank) in &db.ranks_single {
        if event_idx.contains_key(event_id.as_str()) && rank.world_rank <= TOP_N {
            pool.insert(person_id.as_str());
        }
    }
    for ((person_id, event_id), rank) in &db.ranks_average {
        if no_avg.contains(event_id.as_str()) {
            continue;
        }
        if event_idx.contains_key(event_id.as_str()) && rank.world_rank <= TOP_N {
            pool.insert(person_id.as_str());
        }
    }

    // Phase 3: build lookup tables for pool persons across all current events.
    // (person_id, event_idx) → (world_rank, best_value)
    let mut s_lookup: HashMap<(&str, usize), (u32, i32)> = HashMap::new();
    let mut a_lookup: HashMap<(&str, usize), (u32, i32)> = HashMap::new();

    for ((person_id, event_id), rank) in &db.ranks_single {
        if let Some(&idx) = event_idx.get(event_id.as_str()) {
            if pool.contains(person_id.as_str()) {
                s_lookup.insert((person_id.as_str(), idx), (rank.world_rank, rank.best));
            }
        }
    }
    for ((person_id, event_id), rank) in &db.ranks_average {
        if no_avg.contains(event_id.as_str()) {
            continue;
        }
        if let Some(&idx) = event_idx.get(event_id.as_str()) {
            if pool.contains(person_id.as_str()) {
                a_lookup.insert((person_id.as_str(), idx), (rank.world_rank, rank.best));
            }
        }
    }

    // Phase 4: build PersonData for each pool member.
    let mut persons: Vec<PersonData> = pool
        .iter()
        .map(|&person_id| {
            let mut sr = vec![0u32; n];
            let mut ar = vec![0u32; n];
            let mut ps = vec![0i32; n];
            let mut pa = vec![0i32; n];
            for i in 0..n {
                if let Some(&(r, v)) = s_lookup.get(&(person_id, i)) {
                    sr[i] = r;
                    ps[i] = v;
                }
                if let Some(&(r, v)) = a_lookup.get(&(person_id, i)) {
                    ar[i] = r;
                    pa[i] = v;
                }
            }
            let p = db.persons.get(person_id);
            PersonData {
                id: person_id.to_string(),
                name: p.map(|p| p.name.as_str()).unwrap_or(person_id).to_owned(),
                country: p.map(|p| p.country_id.as_str()).unwrap_or("Unknown").to_owned(),
                sr, ar, ps, pa,
            }
        })
        .collect();

    persons.sort_by(|a, b| a.id.cmp(&b.id));

    eprintln!(
        "  all_ranks — {} persons in pool (top {} per event)",
        persons.len(),
        TOP_N
    );

    let out = AllRanksData {
        events: EVENTS.iter().map(|s| s.to_string()).collect(),
        total_s,
        total_a,
        wr_s,
        wr_a,
        persons,
    };

    let path = format!("{out_dir}/all_ranks.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &out)?;

    Ok(())
}
