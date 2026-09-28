//! Shared plumbing for the "N-man Guildford" team searches (two_man,
//! three_man, ...): loading candidates and pruning down to the people who
//! could possibly matter, before the O(m^k) team search.

use std::collections::HashMap;

use crate::db::WcaDb;

/// Stand-in for "no valid average in this event" — far larger than any real
/// time (max is ~hundreds of thousands of cs), so it never wins a comparison,
/// but small enough that summing a few doesn't overflow i32.
pub const MISSING: i32 = 1_000_000_000;

pub const MINI_EVENTS: &[&str] = &[
    "222", "333", "444", "555", "clock", "minx", "skewb", "sq1", "pyram", "333oh",
];
pub const GUILD_EVENTS: &[&str] = &[
    "222", "333", "444", "555", "clock", "minx", "skewb", "sq1", "pyram", "333oh", "666", "777",
];

#[derive(Clone)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub country: String,
    pub continent: String,
    pub avgs: Vec<i32>, // MISSING where the person has no valid average for that event
    pub total: i32,     // sum of valid avgs only — a sort/display heuristic, not used for correctness
}

/// Everyone with a valid average in *at least one* of `events` — a person
/// doesn't need to cover every event themselves, only the ones assigned to
/// them, so we can't require full coverage the way `relay.rs` does.
/// Sorted by WCA ID so every downstream search visits candidates in a fixed
/// order (HashMap iteration order would make tie-breaks vary run to run).
pub fn eligible_people(db: &WcaDb, events: &[&str]) -> Vec<Person> {
    let avg_lookup: HashMap<(&str, &str), i32> = db
        .ranks_average
        .iter()
        .filter(|(_, r)| r.best > 0)
        .map(|((pid, eid), r)| ((pid.as_str(), eid.as_str()), r.best))
        .collect();

    let mut people: Vec<Person> = db
        .persons
        .iter()
        .filter_map(|(person_id, person)| {
            let avgs: Vec<i32> = events
                .iter()
                .map(|&ev| avg_lookup.get(&(person_id.as_str(), ev)).copied().unwrap_or(MISSING))
                .collect();
            if avgs.iter().all(|&t| t == MISSING) {
                return None;
            }
            let total: i32 = avgs.iter().filter(|&&t| t < MISSING).sum();
            let continent = db
                .countries
                .get(&person.country_id)
                .map(|c| c.continent_id.clone())
                .unwrap_or_default();
            Some(Person {
                id: person_id.clone(),
                name: person.name.clone(),
                country: person.country_id.clone(),
                continent,
                avgs,
                total,
            })
        })
        .collect();
    people.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    people
}

/// Drop candidates that can *never* appear in an optimal team of `team_size`.
///
/// Single-person domination (A <= B on every event) is NOT enough to drop B:
/// if the optimal team happens to already contain A in one of the other
/// `team_size - 1` slots, B might still be the best available pick for the
/// remaining slot, dominated or not. But if B has `team_size` or more distinct
/// dominators, at most `team_size - 1` of them can occupy the other slots, so
/// by pigeonhole at least one dominator is always free to swap in for B
/// without collision — and that swap is never worse (it beats B on every
/// event). So B is only safe to drop once its dominator count reaches
/// `team_size`. Domination is computed once against the full input set (not
/// the shrinking pool): the partial order has no cycles, so repeatedly
/// swapping a dropped person for one of its `team_size`+ dominators always
/// terminates at someone kept in the pool.
///
/// This pruning is sound for finding the SINGLE best-scoring team. It is NOT
/// guaranteed to preserve every distinct composition among the top-K teams
/// for K>1 (two different top-K teams could collapse onto the same
/// substitute) — callers that need an actual top-K list, not just the best,
/// should not rely on this beyond K=1.
pub fn prune_hopeless(mut people: Vec<Person>, n: usize, team_size: usize) -> Vec<Person> {
    let m = people.len();

    // Anyone who could dominate X must (a) have a valid, real average at every
    // event X does, so in particular at X's rarest valid event, and (b) beat or
    // match X there. So for each event, keep a time-sorted list of everyone
    // valid at it; a candidate dominator of X must appear in the prefix of
    // that list (strictly faster than X) for whichever of X's events has the
    // fewest total entrants — that bounds how many people we actually need to
    // run the full n-dimensional check against.
    let mut event_entries: Vec<Vec<(i32, usize)>> = vec![Vec::new(); n];
    for (i, p) in people.iter().enumerate() {
        for e in 0..n {
            if p.avgs[e] < MISSING {
                event_entries[e].push((p.avgs[e], i));
            }
        }
    }
    for v in event_entries.iter_mut() {
        v.sort_unstable_by_key(|&(t, _)| t);
    }

    let mut dominator_count = vec![0usize; m];
    for i in 0..m {
        let anchor = (0..n)
            .filter(|&e| people[i].avgs[e] < MISSING)
            .min_by_key(|&e| event_entries[e].len());
        let Some(anchor) = anchor else { continue };

        let v = people[i].avgs[anchor];
        let prefix_len = event_entries[anchor].partition_point(|&(t, _)| t < v);
        for &(_, j) in &event_entries[anchor][..prefix_len] {
            if j == i {
                continue;
            }
            if (0..n).all(|e| people[j].avgs[e] <= people[i].avgs[e])
                && (0..n).any(|e| people[j].avgs[e] < people[i].avgs[e])
            {
                dominator_count[i] += 1;
                if dominator_count[i] >= team_size {
                    break;
                }
            }
        }
    }

    let mut counts = dominator_count.into_iter();
    people.retain(|_| counts.next().unwrap() < team_size);
    people
}
