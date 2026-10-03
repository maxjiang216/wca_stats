use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;
use crate::db::models::RawRank;

/// People kept per event per ranking type.
const TOP_N: usize = 100;

#[derive(Serialize)]
struct Entry {
    rank: usize,
    person_id: String,
    person_name: String,
    country_id: String,
    value: i32,
    world_rank: u32,
}

#[derive(Serialize, Default)]
struct EventRankings {
    single: Vec<Entry>,
    average: Vec<Entry>,
}

/// Top `TOP_N` per event of a ranks table, skipping (event, person) pairs in `excluded`.
fn top_excluding(
    db: &WcaDb,
    ranks: &HashMap<(String, String), RawRank>,
    excluded: &HashSet<(&str, &str)>,
) -> HashMap<String, Vec<Entry>> {
    let mut by_event: HashMap<&str, Vec<&RawRank>> = HashMap::new();
    for r in ranks.values() {
        if r.best > 0 && !excluded.contains(&(r.event_id.as_str(), r.person_id.as_str())) {
            by_event.entry(r.event_id.as_str()).or_default().push(r);
        }
    }
    by_event
        .into_iter()
        .map(|(event, mut rows)| {
            rows.sort_by_key(|r| (r.best, r.world_rank));
            rows.truncate(TOP_N);
            let mut out: Vec<Entry> = Vec::with_capacity(rows.len());
            for (i, r) in rows.iter().enumerate() {
                // Competition ranking: equal PBs share a rank.
                let rank = match out.last() {
                    Some(prev) if prev.value == r.best => prev.rank,
                    _ => i + 1,
                };
                let person = db.persons.get(&r.person_id);
                out.push(Entry {
                    rank,
                    person_id: r.person_id.clone(),
                    person_name: person.map(|p| p.name.clone()).unwrap_or_default(),
                    country_id: person.map(|p| p.country_id.clone()).unwrap_or_default(),
                    value: r.best,
                    world_rank: r.world_rank,
                });
            }
            (event.to_string(), out)
        })
        .collect()
}

fn write_rankings(db: &WcaDb, excluded: &HashSet<(&str, &str)>, path: &str) -> Result<usize> {
    let mut out: HashMap<String, EventRankings> = HashMap::new();
    for (event, list) in top_excluding(db, &db.ranks_single, excluded) {
        out.entry(event).or_default().single = list;
    }
    for (event, list) in top_excluding(db, &db.ranks_average, excluded) {
        out.entry(event).or_default().average = list;
    }
    serde_json::to_writer(std::fs::File::create(path)?, &out)?;
    Ok(out.len())
}

/// "Best without a record": top PBs among people who have never set any
/// regional record (NR/CR/WR), single or average, in that event.
/// "Best without a win": top PBs among people who have never won a final
/// (position 1 with a valid result) in that event.
pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let final_rounds: Vec<&str> = db
        .round_types
        .values()
        .filter(|rt| rt.is_final == 1)
        .map(|rt| rt.id.as_str())
        .collect();

    let mut record_holders: HashSet<(&str, &str)> = HashSet::new();
    let mut winners: HashSet<(&str, &str)> = HashSet::new();
    for r in &db.results {
        let key = (r.event_id.as_str(), r.person_id.as_str());
        let has_record = |m: &Option<String>| m.as_deref().is_some_and(|s| !s.is_empty());
        if has_record(&r.regional_single_record) || has_record(&r.regional_average_record) {
            record_holders.insert(key);
        }
        if r.pos == 1 && r.best > 0 && final_rounds.contains(&r.round_type_id.as_str()) {
            winners.insert(key);
        }
    }

    let n = write_rankings(db, &record_holders, &format!("{out_dir}/best_without_record.json"))?;
    write_rankings(db, &winners, &format!("{out_dir}/best_without_win.json"))?;
    eprintln!(
        "  best_without: {n} events, {} record holders, {} winners excluded",
        record_holders.len(),
        winners.len()
    );
    Ok(())
}
