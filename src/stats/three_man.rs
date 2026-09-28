//! 3-man Guildford: best split of a fixed event set across 3 teammates.
//! Same idea as two_man, but with an extra pruning step before the O(m^3)
//! triple search — see `prune_hopeless` below for the soundness argument.

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

const TOP_N: usize = 10;
const TEAM_SIZE: usize = 3;

#[derive(Serialize, Clone)]
struct PersonRef {
    id: String,
    name: String,
    country: String,
}

#[derive(Serialize, Clone)]
struct TripleEntry {
    a: PersonRef,
    b: PersonRef,
    c: PersonRef,
    time_cs: i32,
    time_a: i32,
    time_b: i32,
    time_c: i32,
    events_a: Vec<String>,
    events_b: Vec<String>,
    events_c: Vec<String>,
}

#[derive(Serialize)]
struct ChallengeOutput {
    events: Vec<String>,
    triples: Vec<TripleEntry>,
}

fn solve(db: &WcaDb, events: &[&str]) -> ChallengeOutput {
    let n = events.len();
    let n_masks = 1usize << n;
    let full_mask = n_masks - 1;

    let people = eligible_people(db, events);
    let mut people = prune_hopeless(people, n, TEAM_SIZE);
    people.sort_by_key(|p| p.total);
    let m = people.len();
    eprintln!(
        "  3-man {} events: {} persons survive pruning, {} triples",
        n,
        m,
        if m >= 3 { m * (m - 1) * (m - 2) / 6 } else { 0 }
    );

    // ss[p * n_masks + mask] = person p's total time for exactly the events in
    // `mask`, or >= MISSING if that set includes an event they can't do.
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

    // top: (max_time, total_time, a, b, mask_a, mask_b) sorted ascending by (max, total).
    let mut top: Vec<(i32, i32, usize, usize, usize, usize, usize)> = Vec::with_capacity(TOP_N + 1);
    let mut threshold = i32::MAX;
    let mut threshold_minor = i32::MAX;

    for a in 0..m {
        for b in (a + 1)..m {
            for c in (b + 1)..m {
                // Lower bound: two floors, take the tighter.
                // (1) split every event to whoever of the three is fastest at
                //     it, then divide the total three ways.
                // (2) whichever event ends up the bottleneck, its assignee
                //     can't beat their own best time at it — so the single
                //     largest per-event minimum among the three is itself a
                //     floor on the team's max, independent of the split.
                let mut sum_min = 0i32;
                let mut max_min = 0i32;
                for e in 0..n {
                    let v = people[a].avgs[e].min(people[b].avgs[e]).min(people[c].avgs[e]);
                    sum_min += v;
                    max_min = max_min.max(v);
                }
                let lower = (sum_min / 3).max(max_min);
                if lower >= threshold {
                    continue;
                }

                let base_a = a * n_masks;
                let base_b = b * n_masks;
                let base_c = c * n_masks;

                let mut best_score = i32::MAX;
                let mut best_minor = i32::MAX;
                let mut best_mask_a = 0usize;
                let mut best_mask_b = 0usize;

                for mask_a in 0..n_masks {
                    let ta = ss[base_a + mask_a];
                    if ta >= best_score || ta >= MISSING {
                        continue;
                    }
                    let rem = full_mask ^ mask_a;
                    // Enumerate submasks of `rem` for B; C gets the leftover.
                    let mut sub = rem;
                    loop {
                        let tb = ss[base_b + sub];
                        let tc = ss[base_c + (rem ^ sub)];
                        if tb < MISSING && tc < MISSING {
                            let score = ta.max(tb).max(tc);
                            let total = ta + tb + tc;
                            if (score, total) < (best_score, best_minor) {
                                best_score = score;
                                best_minor = total;
                                best_mask_a = mask_a;
                                best_mask_b = sub;
                            }
                        }
                        if sub == 0 {
                            break;
                        }
                        sub = (sub - 1) & rem;
                    }
                }

                // No feasible 3-way split (some event isn't covered by any of a/b/c).
                if best_score == i32::MAX {
                    continue;
                }

                if top.len() < TOP_N || (best_score, best_minor) < (threshold, threshold_minor) {
                    top.push((best_score, best_minor, a, b, c, best_mask_a, best_mask_b));
                    top.sort_unstable_by_key(|&(s, t, ..)| (s, t));
                    top.truncate(TOP_N);
                    if top.len() == TOP_N {
                        threshold = top[TOP_N - 1].0;
                        threshold_minor = top[TOP_N - 1].1;
                    }
                }
            }
        }
    }

    let mk_triple = |ai: usize, bi: usize, ci: usize, mask_a: usize, mask_b: usize| -> TripleEntry {
        let rem = full_mask ^ mask_a;
        let mask_c = rem ^ mask_b;
        let ta = ss[ai * n_masks + mask_a];
        let tb = ss[bi * n_masks + mask_b];
        let tc = ss[ci * n_masks + mask_c];
        let events_of = |mask: usize| -> Vec<String> {
            (0..n).filter(|&e| mask & (1 << e) != 0).map(|e| events[e].to_string()).collect()
        };
        TripleEntry {
            a: PersonRef { id: people[ai].id.clone(), name: people[ai].name.clone(), country: people[ai].country.clone() },
            b: PersonRef { id: people[bi].id.clone(), name: people[bi].name.clone(), country: people[bi].country.clone() },
            c: PersonRef { id: people[ci].id.clone(), name: people[ci].name.clone(), country: people[ci].country.clone() },
            time_cs: ta.max(tb).max(tc),
            time_a: ta,
            time_b: tb,
            time_c: tc,
            events_a: events_of(mask_a),
            events_b: events_of(mask_b),
            events_c: events_of(mask_c),
        }
    };

    let triples: Vec<TripleEntry> = top
        .iter()
        .map(|&(_, _, a, b, c, ma, mb)| mk_triple(a, b, c, ma, mb))
        .collect();

    eprintln!("  → {} global triples", triples.len());

    ChallengeOutput {
        events: events.iter().map(|s| s.to_string()).collect(),
        triples,
    }
}

/// TEMP debug helper: report pool size after correct pruning for a given
/// team_size, without running the O(m^k) team search.
pub fn debug_pool_size(db: &WcaDb, mini: bool, team_size: usize) {
    let events: &[&str] = if mini { MINI_EVENTS } else { GUILD_EVENTS };
    let n = events.len();
    let people = eligible_people(db, events);
    eprintln!("  eligible (>=1 valid event): {}", people.len());
    let people = prune_hopeless(people, n, team_size);
    eprintln!("  survive pruning (team_size={}): {}", team_size, people.len());
}

/// DFS over event assignments for a fixed quad, in `event_order` (slowest
/// first). Prunes a branch the instant any member's running partial sum
/// reaches `best` — sums only grow, so that branch can never win. Updates
/// `best` in place whenever a complete assignment beats it.
fn dfs_best_split(
    people: &[Person],
    quad: &[usize; 4],
    event_order: &[usize],
    depth: usize,
    partial: &mut [i32; 4],
    best: &mut i32,
) {
    if depth == event_order.len() {
        let score = partial.iter().copied().max().unwrap();
        if score < *best {
            *best = score;
        }
        return;
    }
    let e = event_order[depth];
    for m in 0..4 {
        let v = people[quad[m]].avgs[e];
        if v >= MISSING {
            continue;
        }
        let new_val = partial[m] + v;
        if new_val >= *best {
            continue; // this branch can only get worse from here — cut it
        }
        let old = partial[m];
        partial[m] = new_val;
        dfs_best_split(people, quad, event_order, depth + 1, partial, best);
        partial[m] = old;
    }
}

/// TEMP feasibility probe: find the single best 4-way split, with progress
/// printed periodically so a runaway search shows itself before the timeout.
/// `sample` caps the pool to its N fastest-by-total people, to estimate cost
/// on a slice before committing to the full pool.
pub fn debug_four_man(db: &WcaDb, mini: bool, sample: Option<usize>, incumbent: i32) {
    let events: &[&str] = if mini { MINI_EVENTS } else { GUILD_EVENTS };
    let n = events.len();

    let people = eligible_people(db, events);
    let mut people = prune_hopeless(people, n, 4);
    // A person can only help beat the (k-1)-team incumbent if their single
    // fastest valid event alone is already faster than it — otherwise
    // whatever event they're assigned makes the team's max >= incumbent.
    people.retain(|p| p.avgs.iter().copied().min().unwrap_or(MISSING) < incumbent);
    people.sort_by_key(|p| p.total);
    if let Some(k) = sample {
        people.truncate(k);
    }
    let m = people.len();
    eprintln!("  (after incumbent={} prefilter)", incumbent);
    let combos: u64 = if m >= 4 {
        (m as u64) * (m as u64 - 1) * (m as u64 - 2) * (m as u64 - 3) / 24
    } else {
        0
    };
    eprintln!("  4-man {} events: {} persons, {} quads", n, m, combos);

    // Event order for the DFS split search: slowest (highest average time
    // across survivors) first, so the branches most likely to blow the
    // budget get decided — and pruned — earliest.
    let mut event_order: Vec<usize> = (0..n).collect();
    {
        let mut avg_time = vec![0i64; n];
        let mut count = vec![0i64; n];
        for p in &people {
            for e in 0..n {
                if p.avgs[e] < MISSING {
                    avg_time[e] += p.avgs[e] as i64;
                    count[e] += 1;
                }
            }
        }
        event_order.sort_unstable_by_key(|&e| {
            std::cmp::Reverse(if count[e] > 0 { avg_time[e] / count[e] } else { 0 })
        });
    }

    let t0 = std::time::Instant::now();
    let mut best = incumbent;
    let mut checked: u64 = 0;
    let mut full_searches: u64 = 0;

    'outer: for a in 0..m {
        for b in (a + 1)..m {
            for c in (b + 1)..m {
                for d in (c + 1)..m {
                    checked += 1;
                    if checked % 20_000_000 == 0 {
                        eprintln!(
                            "    ...{}M quads checked, {} full searches, best={}, elapsed={:.1?}",
                            checked / 1_000_000,
                            full_searches,
                            best,
                            t0.elapsed()
                        );
                        if t0.elapsed() > std::time::Duration::from_secs(45) {
                            eprintln!("  aborting probe early (>45s)");
                            break 'outer;
                        }
                    }

                    let mut sum_min = 0i32;
                    let mut max_min = 0i32;
                    for e in 0..n {
                        let v = people[a].avgs[e]
                            .min(people[b].avgs[e])
                            .min(people[c].avgs[e])
                            .min(people[d].avgs[e]);
                        sum_min += v;
                        max_min = max_min.max(v);
                    }
                    let lower = (sum_min / 4).max(max_min);
                    if lower >= best {
                        continue;
                    }

                    full_searches += 1;
                    let quad = [a, b, c, d];
                    let mut partial = [0i32; 4];
                    dfs_best_split(&people, &quad, &event_order, 0, &mut partial, &mut best);
                }
            }
        }
    }

    eprintln!(
        "  4-man {} events: done, {} quads checked, {} full searches, best={}, elapsed={:.1?}",
        n, checked, full_searches, best, t0.elapsed()
    );
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let mini = solve(db, MINI_EVENTS);
    let guild = solve(db, GUILD_EVENTS);

    #[derive(Serialize)]
    struct Out { mini: ChallengeOutput, guild: ChallengeOutput }
    serde_json::to_writer(
        std::fs::File::create(format!("{out_dir}/three_man.json"))?,
        &Out { mini, guild },
    )?;
    Ok(())
}
