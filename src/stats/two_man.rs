use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

const MINI_EVENTS: &[&str] = &[
    "222", "333", "444", "555", "clock", "minx", "skewb", "sq1", "pyram", "333oh",
];
const GUILD_EVENTS: &[&str] = &[
    "222", "333", "444", "555", "clock", "minx", "skewb", "sq1", "pyram", "333oh", "666", "777",
];

#[derive(Serialize, Clone)]
struct PersonRef {
    id: String,
    name: String,
    country: String,
}

#[derive(Serialize, Clone)]
struct PairEntry {
    a: PersonRef,
    b: PersonRef,
    time_cs: i32,
    time_a: i32,
    time_b: i32,
    events_a: Vec<String>,
    events_b: Vec<String>,
}

#[derive(Serialize)]
struct CountryEntry {
    country: String,
    pair: PairEntry,
}

#[derive(Serialize)]
struct ChallengeOutput {
    events: Vec<String>,
    pairs: Vec<PairEntry>,
    countries: Vec<CountryEntry>,
}

struct Person {
    id: String,
    name: String,
    country: String,
    avgs: Vec<i32>,
    total: i32,
}

fn solve(db: &WcaDb, events: &[&str]) -> ChallengeOutput {
    let n = events.len();
    let n_masks = 1usize << n;
    let full_mask = n_masks - 1;

    // Fast (&str, &str) → best average lookup (avoids String clones per lookup).
    let avg_lookup: HashMap<(&str, &str), i32> = db
        .ranks_average
        .iter()
        .filter(|(_, r)| r.best > 0)
        .map(|((pid, eid), r)| ((pid.as_str(), eid.as_str()), r.best))
        .collect();

    // Collect people who have valid averages for every event in this challenge.
    let mut people: Vec<Person> = db
        .persons
        .iter()
        .filter_map(|(person_id, person)| {
            let avgs: Vec<i32> = events
                .iter()
                .map(|&ev| avg_lookup.get(&(person_id.as_str(), ev)).copied())
                .collect::<Option<Vec<i32>>>()?;
            let total = avgs.iter().sum();
            Some(Person {
                id: person_id.clone(),
                name: person.name.clone(),
                country: person.country_id.clone(),
                avgs,
                total,
            })
        })
        .collect();

    // Sort by total time ascending so fastest people are evaluated first.
    // This tightens the pruning threshold quickly.
    people.sort_by_key(|p| p.total);
    let m = people.len();
    eprintln!("  2-man {} events: {} persons, {} pairs", n, m, m * (m - 1) / 2);

    // Precompute subset sums: ss[p * n_masks + mask] = total time for person p doing
    // exactly the events indicated by the bits of mask.
    // Build via DP: flip one bit at a time.
    let mut ss: Vec<i32> = vec![0i32; m * n_masks];
    for p in 0..m {
        let base = p * n_masks;
        for mask in 1..n_masks {
            let lsb = mask & mask.wrapping_neg();
            let bit = lsb.trailing_zeros() as usize;
            ss[base + mask] = ss[base + (mask ^ lsb)] + people[p].avgs[bit];
        }
    }

    // ── Main search ──────────────────────────────────────────────────────────
    // top: (max_time, min_time, a, b, mask_a) sorted ascending by (max, min).
    // Tiebreak: lower min_time (faster person's total) is better.
    let mut top: Vec<(i32, i32, usize, usize, usize)> = Vec::with_capacity(101);
    let mut threshold = i32::MAX;         // max_time of 100th-best pair (primary prune key)
    let mut threshold_minor = i32::MAX;  // min_time of 100th-best pair (secondary)

    // best_country: country → (max_time, min_time, a, b, mask_a)
    let mut best_country: HashMap<String, (i32, i32, usize, usize, usize)> = HashMap::new();

    for a in 0..m {
        let base_a = a * n_masks;

        for b in (a + 1)..m {
            // Lower bound on max_time: sum_e min(avg_a[e], avg_b[e]) / 2.
            let lower: i32 = (0..n)
                .map(|e| people[a].avgs[e].min(people[b].avgs[e]))
                .sum::<i32>()
                / 2;
            if lower >= threshold {
                continue;
            }

            let base_b = b * n_masks;

            // Find the split minimising (max_time, min_time) lexicographically.
            let mut best_score = i32::MAX;
            let mut best_minor = i32::MAX;
            let mut best_mask = 0usize;
            for mask in 0..n_masks {
                let ta = ss[base_a + mask];
                let tb = ss[base_b + (full_mask ^ mask)];
                let score = if ta > tb { ta } else { tb };
                let minor = if ta < tb { ta } else { tb };
                if (score, minor) < (best_score, best_minor) {
                    best_score = score;
                    best_minor = minor;
                    best_mask = mask;
                }
            }

            // Update same-country best.
            if people[a].country == people[b].country {
                let e = best_country
                    .entry(people[a].country.clone())
                    .or_insert((i32::MAX, i32::MAX, 0, 0, 0));
                if (best_score, best_minor) < (e.0, e.1) {
                    *e = (best_score, best_minor, a, b, best_mask);
                }
            }

            // Update global top-100.
            if top.len() < 100 || (best_score, best_minor) < (threshold, threshold_minor) {
                top.push((best_score, best_minor, a, b, best_mask));
                top.sort_unstable_by_key(|&(s, m, _, _, _)| (s, m));
                top.truncate(100);
                if top.len() == 100 {
                    threshold = top[99].0;
                    threshold_minor = top[99].1;
                }
            }
        }
    }

    // ── Convert indices → output structs ─────────────────────────────────────
    let mk_pair = |ai: usize, bi: usize, mask_a: usize| -> PairEntry {
        let ta = ss[ai * n_masks + mask_a];
        let tb = ss[bi * n_masks + (full_mask ^ mask_a)];
        let score = ta.max(tb);
        let events_a: Vec<String> = (0..n)
            .filter(|&e| mask_a & (1 << e) != 0)
            .map(|e| events[e].to_string())
            .collect();
        let events_b: Vec<String> = (0..n)
            .filter(|&e| mask_a & (1 << e) == 0)
            .map(|e| events[e].to_string())
            .collect();
        PairEntry {
            a: PersonRef { id: people[ai].id.clone(), name: people[ai].name.clone(), country: people[ai].country.clone() },
            b: PersonRef { id: people[bi].id.clone(), name: people[bi].name.clone(), country: people[bi].country.clone() },
            time_cs: score,
            time_a: ta,
            time_b: tb,
            events_a,
            events_b,
        }
    };

    let pairs: Vec<PairEntry> = top.iter().map(|&(_, _, a, b, m)| mk_pair(a, b, m)).collect();

    let mut countries: Vec<CountryEntry> = best_country
        .into_iter()
        .map(|(country, (_, _, a, b, m))| CountryEntry { country, pair: mk_pair(a, b, m) })
        .collect();
    countries.sort_by_key(|c| (c.pair.time_cs, c.pair.time_a.min(c.pair.time_b)));

    eprintln!("  → {} global pairs, {} countries", pairs.len(), countries.len());

    ChallengeOutput {
        events: events.iter().map(|s| s.to_string()).collect(),
        pairs,
        countries,
    }
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let mini = solve(db, MINI_EVENTS);
    let guild = solve(db, GUILD_EVENTS);

    #[derive(Serialize)]
    struct Out { mini: ChallengeOutput, guild: ChallengeOutput }
    serde_json::to_writer(
        std::fs::File::create(format!("{out_dir}/two_man.json"))?,
        &Out { mini, guild },
    )?;
    Ok(())
}
