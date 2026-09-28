//! 2-man Guildford: best split of a fixed event set across 2 teammates,
//! reported globally, ranked by continent, and ranked by country.
//!
//! Each of those is a single "who's the best pair" question over a
//! restricted population (the world / a continent / a country), so we only
//! ever need the single best pair per group — see `team_common::prune_hopeless`
//! for why that's the case the dominance pruning is sound for.

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;
use crate::stats::team_common::{eligible_people, prune_hopeless, Person, MISSING};

const MINI_EVENTS: &[&str] = &[
    "222", "333", "444", "555", "clock", "minx", "skewb", "sq1", "pyram", "333oh",
];
const GUILD_EVENTS: &[&str] = &[
    "222", "333", "444", "555", "clock", "minx", "skewb", "sq1", "pyram", "333oh", "666", "777",
];
const TEAM_SIZE: usize = 2;

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
struct RegionEntry {
    id: String,
    name: String,
    pair: PairEntry,
}

#[derive(Serialize)]
struct ChallengeOutput {
    events: Vec<String>,
    global: Option<PairEntry>,
    continents: Vec<RegionEntry>,
    countries: Vec<RegionEntry>,
}

fn solve(db: &WcaDb, events: &[&str]) -> ChallengeOutput {
    let n = events.len();

    let all_people = eligible_people(db, events);
    eprintln!("  2-man {} events: {} eligible people", n, all_people.len());

    let global_pool = prune_hopeless(all_people.clone(), n, TEAM_SIZE);
    eprintln!("  2-man {} events: {} survive global pruning", n, global_pool.len());
    let global = best_pair_with_events(&global_pool, n, events);

    let mut by_continent: std::collections::HashMap<String, Vec<Person>> = std::collections::HashMap::new();
    let mut by_country: std::collections::HashMap<String, Vec<Person>> = std::collections::HashMap::new();
    for p in &all_people {
        by_continent.entry(p.continent.clone()).or_default().push(p.clone());
        by_country.entry(p.country.clone()).or_default().push(p.clone());
    }

    let mut continents: Vec<RegionEntry> = by_continent
        .into_iter()
        .filter(|(id, _)| !id.is_empty())
        .filter_map(|(id, group)| {
            let pool = prune_hopeless(group, n, TEAM_SIZE);
            let pair = best_pair_with_events(&pool, n, events)?;
            Some(RegionEntry {
                id: id.clone(),
                name: db.continents.get(&id).map(|c| c.name.clone()).unwrap_or(id),
                pair,
            })
        })
        .collect();
    continents.sort_by(|x, y| (x.pair.time_cs, &x.name).cmp(&(y.pair.time_cs, &y.name)));

    let mut countries: Vec<RegionEntry> = by_country
        .into_iter()
        .filter_map(|(id, group)| {
            if group.len() < 2 {
                return None;
            }
            let pool = prune_hopeless(group, n, TEAM_SIZE);
            let pair = best_pair_with_events(&pool, n, events)?;
            Some(RegionEntry {
                id: id.clone(),
                name: db.countries.get(&id).map(|c| c.name.clone()).unwrap_or(id),
                pair,
            })
        })
        .collect();
    countries.sort_by(|x, y| (x.pair.time_cs, &x.name).cmp(&(y.pair.time_cs, &y.name)));

    eprintln!(
        "  2-man {} events: global={:?}, {} continents, {} countries",
        n,
        global.as_ref().map(|p| p.time_cs),
        continents.len(),
        countries.len()
    );

    ChallengeOutput {
        events: events.iter().map(|s| s.to_string()).collect(),
        global,
        continents,
        countries,
    }
}

/// Like `best_pair`, but also fills in the event-split fields (needs `events`
/// for the names, which the low-level search doesn't carry).
fn best_pair_with_events(people: &[Person], n: usize, events: &[&str]) -> Option<PairEntry> {
    let (pair, mask_a) = best_pair_raw(people, n)?;
    let mut pair = pair;
    pair.events_a = (0..n).filter(|&e| mask_a & (1 << e) != 0).map(|e| events[e].to_string()).collect();
    pair.events_b = (0..n).filter(|&e| mask_a & (1 << e) == 0).map(|e| events[e].to_string()).collect();
    Some(pair)
}

fn best_pair_raw(people: &[Person], n: usize) -> Option<(PairEntry, usize)> {
    let m = people.len();
    if m < 2 {
        return None;
    }
    let n_masks = 1usize << n;
    let full_mask = n_masks - 1;

    let mut ss: Vec<i32> = vec![0i32; m * n_masks];
    for p in 0..m {
        let base = p * n_masks;
        for mask in 1..n_masks {
            let lsb = mask & mask.wrapping_neg();
            let bit = lsb.trailing_zeros() as usize;
            let prev = ss[base + (mask ^ lsb)];
            ss[base + mask] = if prev >= MISSING || people[p].avgs[bit] >= MISSING {
                MISSING
            } else {
                prev + people[p].avgs[bit]
            };
        }
    }

    let mut best_score = i32::MAX;
    let mut best_total = i32::MAX;
    let mut best: (usize, usize, usize) = (0, 0, 0);

    for a in 0..m {
        let base_a = a * n_masks;
        for b in (a + 1)..m {
            let mut sum_min = 0i32;
            let mut max_min = 0i32;
            for e in 0..n {
                let v = people[a].avgs[e].min(people[b].avgs[e]);
                sum_min += v;
                max_min = max_min.max(v);
            }
            let lower = (sum_min / 2).max(max_min);
            // `>` not `>=`: a pair tying the best time can still win on total.
            if lower > best_score {
                continue;
            }

            let base_b = b * n_masks;
            for mask in 0..n_masks {
                let ta = ss[base_a + mask];
                if ta >= MISSING || ta > best_score {
                    continue;
                }
                let tb = ss[base_b + (full_mask ^ mask)];
                if tb >= MISSING {
                    continue;
                }
                let score = ta.max(tb);
                let total = ta + tb;
                if (score, total) < (best_score, best_total) {
                    best_score = score;
                    best_total = total;
                    best = (a, b, mask);
                }
            }
        }
    }

    if best_score == i32::MAX {
        return None;
    }
    let (ai, bi, mask_a) = best;
    let ta = ss[ai * n_masks + mask_a];
    let tb = ss[bi * n_masks + (full_mask ^ mask_a)];
    Some((
        PairEntry {
            a: PersonRef { id: people[ai].id.clone(), name: people[ai].name.clone(), country: people[ai].country.clone() },
            b: PersonRef { id: people[bi].id.clone(), name: people[bi].name.clone(), country: people[bi].country.clone() },
            time_cs: ta.max(tb),
            time_a: ta,
            time_b: tb,
            events_a: Vec::new(),
            events_b: Vec::new(),
        },
        mask_a,
    ))
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
