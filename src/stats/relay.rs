//! Relay / challenge rankings: sum of each person's PB average across a fixed event set.
//! Uses ranks_average.best for every event (ao5 or mo3 depending on the event's WCA format).

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

struct Stat {
    filename: &'static str,
    label: &'static str,
    events: &'static [&'static str],
}

#[rustfmt::skip]
const STATS: &[Stat] = &[
    Stat { filename: "relay_2_4", label: "2–4 Relay",
           events: &["222", "333", "444"] },
    Stat { filename: "relay_2_5", label: "2–5 Relay",
           events: &["222", "333", "444", "555"] },
    Stat { filename: "relay_2_6", label: "2–6 Relay",
           events: &["222", "333", "444", "555", "666"] },
    Stat { filename: "relay_2_7", label: "2–7 Relay",
           events: &["222", "333", "444", "555", "666", "777"] },
    Stat { filename: "mini_guildford", label: "Mini Guildford",
           events: &["222", "333", "444", "555", "clock", "minx", "skewb", "sq1", "pyram", "333oh"] },
    Stat { filename: "guildford", label: "Guildford Challenge",
           events: &["222", "333", "444", "555", "clock", "minx", "skewb", "sq1", "pyram", "333oh", "666", "777"] },
];

#[derive(Serialize)]
struct Entry {
    rank: usize,
    person_id: String,
    person_name: String,
    country_id: String,
    total_cs: i32,
    event_avgs: Vec<i32>,
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    // Build person → (event → best_average) lookup from ranks_average.
    // Only include valid (positive) averages.
    let mut person_avgs: HashMap<&str, HashMap<&str, i32>> = HashMap::new();
    for ((person_id, event_id), rank) in &db.ranks_average {
        if rank.best > 0 {
            person_avgs
                .entry(person_id.as_str())
                .or_default()
                .insert(event_id.as_str(), rank.best);
        }
    }

    for stat in STATS {
        // Only persons who have valid averages in every required event qualify.
        let mut rows: Vec<(i32, &str, Vec<i32>)> = person_avgs
            .iter()
            .filter_map(|(person_id, avgs)| {
                let event_avgs: Option<Vec<i32>> = stat
                    .events
                    .iter()
                    .map(|e| avgs.get(*e).copied())
                    .collect();
                let event_avgs = event_avgs?;
                let total: i32 = event_avgs.iter().sum();
                Some((total, *person_id, event_avgs))
            })
            .collect();

        rows.sort_unstable_by(|(a, pa, _), (b, pb, _)| {
            a.cmp(b).then_with(|| pa.cmp(pb))
        });

        let cutoff = rows
            .iter()
            .enumerate()
            .find(|(i, _)| *i >= 1000 && rows[*i].0 != rows[i - 1].0)
            .map(|(i, _)| i)
            .unwrap_or(rows.len());
        let rows = &rows[..cutoff];

        let mut entries: Vec<Entry> = Vec::with_capacity(rows.len());
        let mut rank = 1;
        for (i, (total, person_id, event_avgs)) in rows.iter().enumerate() {
            if i > 0 && *total != rows[i - 1].0 {
                rank = i + 1;
            }
            let p = db.persons.get(*person_id);
            entries.push(Entry {
                rank,
                person_id: person_id.to_string(),
                person_name: p.map(|p| p.name.as_str()).unwrap_or(person_id).to_owned(),
                country_id: p.map(|p| p.country_id.as_str()).unwrap_or("Unknown").to_owned(),
                total_cs: *total,
                event_avgs: event_avgs.clone(),
            });
        }

        eprint!("  {} — {} qualifiers", stat.label, entries.len());
        if let Some(top) = entries.first() {
            eprint!(", best: {}cs", top.total_cs);
        }
        eprintln!();

        let path = format!("{out_dir}/{}.json", stat.filename);
        serde_json::to_writer(std::fs::File::create(&path)?, &entries)?;
    }

    Ok(())
}
