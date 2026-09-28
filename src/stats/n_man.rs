//! k-man Guildford (k = 3, 4): best split of a fixed event set across k
//! teammates, reported globally, per continent, and per country — the same
//! shape as two_man, generalized to any team size.
//!
//! Each region only needs its single best team, which is exactly the case
//! `prune_hopeless` is sound for. The search enumerates k-combinations of the
//! pruned pool, rejects most with two cheap lower bounds, and finds the best
//! event split for survivors with a slowest-event-first DFS. A k-team can
//! always match its region's best (k-1)-team (the extra member idles), so each
//! search is seeded with that score, which makes the bounds bite immediately.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;

use crate::db::WcaDb;
use crate::stats::team_common::{eligible_people, prune_hopeless, Person, GUILD_EVENTS, MINI_EVENTS, MISSING};

#[derive(Serialize, Clone)]
struct Member {
    id: String,
    name: String,
    country: String,
    time_cs: i32,
    events: Vec<String>,
}

#[derive(Serialize, Clone)]
struct Team {
    time_cs: i32,
    members: Vec<Member>,
}

#[derive(Serialize)]
struct RegionEntry {
    id: String,
    name: String,
    team: Team,
}

#[derive(Serialize)]
struct ChallengeOutput {
    events: Vec<String>,
    global: Option<Team>,
    continents: Vec<RegionEntry>,
    countries: Vec<RegionEntry>,
}

/// Best team found in one pool: indices into the pool plus each member's
/// event mask, minimizing (max time, total time), then combo indices.
struct Found {
    score: i32,
    total: i32,
    combo: Vec<usize>,
    masks: Vec<u32>,
}

/// (max, total) packed so `fetch_min` orders it lexicographically.
fn pack(score: i32, total: i32) -> u64 {
    ((score as u32 as u64) << 32) | total as u32 as u64
}
fn unpack(v: u64) -> (i32, i32) {
    ((v >> 32) as i32, v as u32 as i32)
}

/// Search state for combinations sharing one first member.
struct Search<'a> {
    pool: &'a [Person],
    k: usize,
    order: &'a [usize],
    /// mins[d][e] = fastest time at event e among the first d+1 chosen members.
    mins: Vec<Vec<i32>>,
    combo: Vec<usize>,
    /// Best (max, total) any worker has found — used only for pruning, and
    /// only to cut candidates that are *strictly* worse, so every co-optimal
    /// team is still reached and the final choice doesn't depend on timing.
    shared: &'a AtomicU64,
    best: (i32, i32),
    best_combo: Vec<usize>,
    best_masks: Vec<u32>,
}

impl Search<'_> {
    fn bound(&self) -> (i32, i32) {
        self.best.min(unpack(self.shared.load(Ordering::Relaxed)))
    }

    fn combos(&mut self, start: usize, depth: usize) {
        if depth == self.k {
            self.leaf();
            return;
        }
        let m = self.pool.len();
        for i in start..=(m - (self.k - depth)) {
            let (prev, cur) = self.mins.split_at_mut(depth);
            for ((c, &p), &v) in cur[0].iter_mut().zip(&prev[depth - 1]).zip(&self.pool[i].avgs) {
                *c = p.min(v);
            }
            self.combo.push(i);
            self.combos(i + 1, depth + 1);
            self.combo.pop();
        }
    }

    fn leaf(&mut self) {
        // Two floors on the team's max: everything split perfectly evenly among
        // the fastest-at-each-event members, and the slowest event's best time.
        let mins = &self.mins[self.k - 1];
        let sum: i64 = mins.iter().map(|&v| v as i64).sum();
        let max = *mins.iter().max().unwrap();
        if max >= MISSING {
            return; // some event nobody in the team can do
        }
        let lower = ((sum / self.k as i64) as i32).max(max);
        let b = self.bound();
        // Every split's total is >= the sum of per-event fastest times, so a
        // team that can at best tie the max is out if that sum is already worse.
        if lower > b.0 || (lower == b.0 && sum > b.1 as i64) {
            return;
        }
        // rest[d] = sum of the team's fastest times over events order[d..].
        let mut rest = vec![0i32; self.order.len() + 1];
        for d in (0..self.order.len()).rev() {
            rest[d] = rest[d + 1] + mins[self.order[d]];
        }
        let mut partial = vec![0i32; self.k];
        let mut masks = vec![0u32; self.k];
        self.split(0, &rest, &mut partial, &mut masks);
    }

    /// DFS over event assignments. Cuts a branch only when a partial sum
    /// strictly exceeds the best max, so equal-max splits still compete on
    /// the total tiebreak.
    fn split(&mut self, depth: usize, rest: &[i32], partial: &mut [i32], masks: &mut [u32]) {
        let total: i32 = partial.iter().sum();
        if depth == self.order.len() {
            let score = *partial.iter().max().unwrap();
            if (score, total) < self.best {
                self.best = (score, total);
                self.best_combo = self.combo.clone();
                self.best_masks = masks.to_vec();
                self.shared.fetch_min(pack(score, total), Ordering::Relaxed);
            }
            return;
        }
        let b = self.bound();
        // Already at the best max: can only win on total, which can't drop
        // below what's committed plus the fastest times for what's left.
        if *partial.iter().max().unwrap() == b.0 && total + rest[depth] > b.1 {
            return;
        }
        let e = self.order[depth];
        for j in 0..self.k {
            let v = self.pool[self.combo[j]].avgs[e];
            if v >= MISSING {
                continue;
            }
            let nv = partial[j] + v;
            if nv > b.0 {
                continue;
            }
            partial[j] = nv;
            masks[j] |= 1 << e;
            self.split(depth + 1, rest, partial, masks);
            partial[j] -= v;
            masks[j] &= !(1 << e);
        }
    }
}

/// Events slowest-first (by mean time across the pool), so a doomed branch
/// overshoots the cutoff within a step or two.
fn slowest_first(pool: &[Person], n: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut sum = vec![0i64; n];
    let mut cnt = vec![0i64; n];
    for p in pool {
        for e in 0..n {
            if p.avgs[e] < MISSING {
                sum[e] += p.avgs[e] as i64;
                cnt[e] += 1;
            }
        }
    }
    order.sort_by_key(|&e| std::cmp::Reverse(if cnt[e] > 0 { sum[e] / cnt[e] } else { 0 }));
    order
}

/// Best k-team in `pool` with max time <= `seed` (if given). Parallel over the
/// first member; the per-worker results are reduced by (score, total, combo),
/// so the answer doesn't depend on scheduling.
fn best_team(pool: &[Person], n: usize, k: usize, seed: Option<i32>) -> Option<Found> {
    let m = pool.len();
    if m < k {
        return None;
    }
    let order = slowest_first(pool, n);
    let shared = AtomicU64::new(pack(seed.unwrap_or(i32::MAX), i32::MAX));
    (0..=m - k)
        .into_par_iter()
        .filter_map(|a| {
            let mut s = Search {
                pool,
                k,
                order: &order,
                mins: vec![vec![0; n]; k],
                combo: vec![a],
                shared: &shared,
                best: (seed.unwrap_or(i32::MAX), i32::MAX),
                best_combo: Vec::new(),
                best_masks: Vec::new(),
            };
            s.mins[0].copy_from_slice(&pool[a].avgs);
            s.combos(a + 1, 1);
            (!s.best_combo.is_empty()).then(|| Found {
                score: s.best.0,
                total: s.best.1,
                combo: s.best_combo,
                masks: s.best_masks,
            })
        })
        .min_by(|x, y| (x.score, x.total, &x.combo).cmp(&(y.score, y.total, &y.combo)))
}

fn to_team(pool: &[Person], f: &Found, events: &[&str]) -> Team {
    let members = f
        .combo
        .iter()
        .zip(&f.masks)
        .map(|(&i, &mask)| {
            let p = &pool[i];
            let evs: Vec<usize> = (0..events.len()).filter(|&e| mask & (1 << e) != 0).collect();
            Member {
                id: p.id.clone(),
                name: p.name.clone(),
                country: p.country.clone(),
                time_cs: evs.iter().map(|&e| p.avgs[e]).sum(),
                events: evs.iter().map(|&e| events[e].to_string()).collect(),
            }
        })
        .collect();
    Team { time_cs: f.score, members }
}

/// Region key -> pruned pool, for the world, each continent, and each country.
struct Pools {
    global: Vec<Person>,
    continents: Vec<(String, Vec<Person>)>,
    countries: Vec<(String, Vec<Person>)>,
}

fn pools(all: &[Person], n: usize, k: usize) -> Pools {
    let mut by_continent: HashMap<&str, Vec<Person>> = HashMap::new();
    let mut by_country: HashMap<&str, Vec<Person>> = HashMap::new();
    for p in all {
        if !p.continent.is_empty() {
            by_continent.entry(p.continent.as_str()).or_default().push(p.clone());
        }
        by_country.entry(p.country.as_str()).or_default().push(p.clone());
    }
    let prune = |groups: HashMap<&str, Vec<Person>>| -> Vec<(String, Vec<Person>)> {
        let mut v: Vec<(String, Vec<Person>)> = groups
            .into_par_iter()
            .filter(|(_, g)| g.len() >= k)
            .map(|(id, g)| (id.to_string(), prune_hopeless(g, n, k)))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };
    Pools {
        global: prune_hopeless(all.to_vec(), n, k),
        continents: prune(by_continent),
        countries: prune(by_country),
    }
}

/// Best score per region key ("" = global), used to seed the next team size.
type Seeds = HashMap<String, i32>;

fn solve_size(
    db: &WcaDb,
    all: &[Person],
    events: &[&str],
    k: usize,
    seeds: &Seeds,
) -> (ChallengeOutput, Seeds) {
    let n = events.len();
    let t = std::time::Instant::now();
    let p = pools(all, n, k);
    let mut next: Seeds = HashMap::new();

    let global = best_team(&p.global, n, k, seeds.get("").copied());
    if let Some(f) = &global {
        next.insert(String::new(), f.score);
    }
    let global = global.map(|f| to_team(&p.global, &f, events));

    let regions = |list: &[(String, Vec<Person>)], prefix: &str| -> Vec<(String, Found)> {
        list.par_iter()
            .filter_map(|(id, pool)| {
                let seed = seeds.get(&format!("{prefix}{id}")).copied();
                best_team(pool, n, k, seed).map(|f| (id.clone(), f))
            })
            .collect()
    };
    let cont_found = regions(&p.continents, "c:");
    let ctry_found = regions(&p.countries, "n:");

    let mut build = |list: &[(String, Vec<Person>)],
                     found: Vec<(String, Found)>,
                     prefix: &str,
                     name_of: &dyn Fn(&str) -> Option<String>|
     -> Vec<RegionEntry> {
        let pools: HashMap<&str, &Vec<Person>> = list.iter().map(|(id, v)| (id.as_str(), v)).collect();
        let mut out: Vec<RegionEntry> = found
            .into_iter()
            .map(|(id, f)| {
                next.insert(format!("{prefix}{id}"), f.score);
                RegionEntry {
                    name: name_of(&id).unwrap_or_else(|| id.clone()),
                    team: to_team(pools[id.as_str()], &f, events),
                    id,
                }
            })
            .collect();
        out.sort_by(|a, b| (a.team.time_cs, &a.name).cmp(&(b.team.time_cs, &b.name)));
        out
    };
    let continents = build(&p.continents, cont_found, "c:", &|id| db.continents.get(id).map(|c| c.name.clone()));
    let countries = build(&p.countries, ctry_found, "n:", &|id| db.countries.get(id).map(|c| c.name.clone()));

    eprintln!(
        "  {k}-man {n} events: pool {}, global {:?}, {} continents, {} countries ({:.2?})",
        p.global.len(),
        global.as_ref().map(|t| t.time_cs),
        continents.len(),
        countries.len(),
        t.elapsed()
    );
    let out = ChallengeOutput {
        events: events.iter().map(|s| s.to_string()).collect(),
        global,
        continents,
        countries,
    };
    (out, next)
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    #[derive(Serialize)]
    struct Out {
        mini: ChallengeOutput,
        guild: ChallengeOutput,
    }
    let mut by_size: Vec<(usize, Out)> = Vec::new();
    let mut per_challenge: Vec<Vec<ChallengeOutput>> = Vec::new();
    for events in [MINI_EVENTS, GUILD_EVENTS] {
        let all = eligible_people(db, events);
        // 2-man results only seed the 3-man search (two_man.rs owns that page).
        let (_, mut seeds) = solve_size(db, &all, events, 2, &HashMap::new());
        let mut outs = Vec::new();
        for k in [3, 4] {
            let (out, next) = solve_size(db, &all, events, k, &seeds);
            outs.push(out);
            seeds = next;
        }
        per_challenge.push(outs);
    }
    let mut guild = per_challenge.pop().unwrap().into_iter();
    let mut mini = per_challenge.pop().unwrap().into_iter();
    for k in [3, 4] {
        by_size.push((k, Out { mini: mini.next().unwrap(), guild: guild.next().unwrap() }));
    }
    for (k, out) in by_size {
        serde_json::to_writer(std::fs::File::create(format!("{out_dir}/team_{k}.json"))?, &out)?;
    }
    Ok(())
}
