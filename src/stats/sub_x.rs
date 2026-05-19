use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

struct Def {
    event_id: &'static str,
    val_cs: i32,
    label: &'static str,
}

// Each def appears in singles and/or averages lists as appropriate.
const SINGLE_DEFS: &[Def] = &[
    Def { event_id: "333",   val_cs:   400, label: "sub-4"    },
    Def { event_id: "333",   val_cs:   500, label: "sub-5"    },
    Def { event_id: "333",   val_cs:   600, label: "sub-6"    },
    Def { event_id: "222",   val_cs:   100, label: "sub-1"    },
    Def { event_id: "444",   val_cs:  2000, label: "sub-20"   },
    Def { event_id: "555",   val_cs:  4000, label: "sub-40"   },
    Def { event_id: "666",   val_cs:  7000, label: "sub-1:10" },
    Def { event_id: "777",   val_cs: 10000, label: "sub-1:40" },
    Def { event_id: "777",   val_cs: 11000, label: "sub-1:50" },
    Def { event_id: "777",   val_cs: 12000, label: "sub-2:00" },
    Def { event_id: "pyram", val_cs:   100, label: "sub-1"    },
    Def { event_id: "skewb", val_cs:   100, label: "sub-1"    },
];

// Same thresholds but only for events that have official averages.
const AVG_DEFS: &[Def] = &[
    Def { event_id: "333",   val_cs:   400, label: "sub-4"    },
    Def { event_id: "333",   val_cs:   500, label: "sub-5"    },
    Def { event_id: "333",   val_cs:   600, label: "sub-6"    },
    Def { event_id: "222",   val_cs:   100, label: "sub-1"    },
    Def { event_id: "444",   val_cs:  2000, label: "sub-20"   },
    Def { event_id: "555",   val_cs:  4000, label: "sub-40"   },
    Def { event_id: "666",   val_cs:  7000, label: "sub-1:10" },
    Def { event_id: "777",   val_cs: 10000, label: "sub-1:40" },
    Def { event_id: "777",   val_cs: 11000, label: "sub-1:50" },
    Def { event_id: "777",   val_cs: 12000, label: "sub-2:00" },
    // pyram and skewb: no average rankings requested
];

#[derive(Serialize)]
struct Entry {
    id: String,
    name: String,
    country: String,
    count: u32,
    pb: i32,
}

#[derive(Serialize)]
struct Ranking {
    label: String,
    val_cs: i32,
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct EventData {
    single: Vec<Ranking>,
    avg: Vec<Ranking>,
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    // Assign a stable index to each person for flat array counting.
    let person_ids: Vec<&str> = db.persons.keys().map(String::as_str).collect();
    let person_idx: HashMap<&str, usize> = person_ids
        .iter()
        .enumerate()
        .map(|(i, &s)| (s, i))
        .collect();
    let np = person_ids.len();

    // For each result we look up which single/average definitions apply.
    // Build event_id → [(def_index, val_cs)] maps for fast dispatch.
    let mut single_by_event: HashMap<&str, Vec<(usize, i32)>> = HashMap::new();
    let mut avg_by_event: HashMap<&str, Vec<(usize, i32)>> = HashMap::new();
    for (i, d) in SINGLE_DEFS.iter().enumerate() {
        single_by_event.entry(d.event_id).or_default().push((i, d.val_cs));
    }
    for (i, d) in AVG_DEFS.iter().enumerate() {
        avg_by_event.entry(d.event_id).or_default().push((i, d.val_cs));
    }

    // Flat count arrays: counts[def_index * np + person_index].
    let mut s_counts: Vec<u32> = vec![0u32; SINGLE_DEFS.len() * np];
    let mut a_counts: Vec<u32> = vec![0u32; AVG_DEFS.len()   * np];

    // Single pass through all results — O(|results| + |attempts for target events|).
    for r in &db.results {
        let eid = r.event_id.as_str();

        // Average counts.
        if r.average > 0 {
            if let Some(adefs) = avg_by_event.get(eid) {
                if let Some(&pi) = person_idx.get(r.person_id.as_str()) {
                    for &(di, thresh) in adefs {
                        if r.average < thresh {
                            a_counts[di * np + pi] += 1;
                        }
                    }
                }
            }
        }

        // Single counts (from individual attempt values).
        if let Some(sdefs) = single_by_event.get(eid) {
            if let Some(&pi) = person_idx.get(r.person_id.as_str()) {
                if let Some(attempts) = db.attempts.get(&r.id) {
                    for &val in attempts {
                        if val <= 0 { continue; }
                        for &(di, thresh) in sdefs {
                            if val < thresh {
                                s_counts[di * np + pi] += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    // PB lookups for context column.
    let pb_single: HashMap<(&str, &str), i32> = db.ranks_single.iter()
        .map(|((pid, eid), r)| ((pid.as_str(), eid.as_str()), r.best))
        .collect();
    let pb_avg: HashMap<(&str, &str), i32> = db.ranks_average.iter()
        .map(|((pid, eid), r)| ((pid.as_str(), eid.as_str()), r.best))
        .collect();

    // Convert a count slice → sorted + truncated Vec<Entry>.
    let mk_entries = |counts: &[u32], event_id: &str, use_single_pb: bool| -> Vec<Entry> {
        let pb_map = if use_single_pb { &pb_single } else { &pb_avg };
        let mut v: Vec<(u32, i32, usize)> = counts
            .iter()
            .enumerate()
            .filter(|&(_, &c)| c > 0)
            .map(|(pi, &c)| {
                let pid = person_ids[pi];
                let pb = pb_map.get(&(pid, event_id)).copied().unwrap_or(0);
                (c, pb, pi)
            })
            .collect();
        v.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        v.truncate(1000);
        v.into_iter()
            .map(|(count, pb, pi)| {
                let pid = person_ids[pi];
                let p = db.persons.get(pid).unwrap();
                Entry {
                    id: pid.to_string(),
                    name: p.name.clone(),
                    country: p.country_id.clone(),
                    count,
                    pb,
                }
            })
            .collect()
    };

    // Collect unique event IDs in definition order.
    let mut events: Vec<&str> = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for d in SINGLE_DEFS.iter().chain(AVG_DEFS.iter()) {
        if seen.insert(d.event_id) {
            events.push(d.event_id);
        }
    }

    // Assemble per-event output.
    let mut output: HashMap<&str, EventData> = HashMap::new();
    for &event_id in &events {
        let singles: Vec<Ranking> = SINGLE_DEFS
            .iter()
            .enumerate()
            .filter(|(_, d)| d.event_id == event_id)
            .map(|(di, d)| {
                let entries = mk_entries(&s_counts[di * np..(di + 1) * np], event_id, true);
                eprintln!("  sub_x {event_id} single {} — {} entries", d.label, entries.len());
                Ranking { label: d.label.to_string(), val_cs: d.val_cs, entries }
            })
            .collect();

        let avgs: Vec<Ranking> = AVG_DEFS
            .iter()
            .enumerate()
            .filter(|(_, d)| d.event_id == event_id)
            .map(|(di, d)| {
                let entries = mk_entries(&a_counts[di * np..(di + 1) * np], event_id, false);
                eprintln!("  sub_x {event_id} avg   {} — {} entries", d.label, entries.len());
                Ranking { label: d.label.to_string(), val_cs: d.val_cs, entries }
            })
            .collect();

        output.insert(event_id, EventData { single: singles, avg: avgs });
    }

    let path = format!("{out_dir}/sub_x.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &output)?;
    Ok(())
}
