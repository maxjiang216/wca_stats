//! Rankings for the Nth-best solve in an average.
//!
//! ao5: best 2nd–5th solve (2nd = fastest counting solve; 5th = best "worst solve").
//! mo3: best 2nd and 3rd solve (all three count, so 2nd is the middle, 3rd the worst).
//!
//! A single parallel fold over all results fills every stat slot at once.

use std::collections::HashMap;

use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;

use crate::db::{models::RawResult, WcaDb};

#[derive(Debug, Serialize)]
pub struct Entry {
    pub rank: usize,
    pub person_id: String,
    pub person_name: String,
    pub country_id: String,
    pub competition_id: String,
    pub value_cs: i32,
    pub single_cs: i32,
    pub average_cs: i32,
}

struct StatDef {
    filename: &'static str,
    label: &'static str,
    is_ao5: bool,
    /// 0-based index into the sorted-ascending attempt array.
    pos: usize,
}

const STATS: &[StatDef] = &[
    StatDef { filename: "ao5_2nd", label: "ao5 2nd (best counting)", is_ao5: true,  pos: 1 },
    StatDef { filename: "ao5_3rd", label: "ao5 3rd",                  is_ao5: true,  pos: 2 },
    StatDef { filename: "ao5_4th", label: "ao5 4th (worst counting)", is_ao5: true,  pos: 3 },
    StatDef { filename: "ao5_5th", label: "ao5 5th (best worst)",     is_ao5: true,  pos: 4 },
    StatDef { filename: "mo3_2nd", label: "mo3 2nd",                  is_ao5: false, pos: 1 },
    StatDef { filename: "mo3_3rd", label: "mo3 3rd (worst)",          is_ao5: false, pos: 2 },
];

/// Per-(event → person) best value and result reference, for one stat slot.
type Slot<'a> = HashMap<&'a str, HashMap<&'a str, (i32, &'a RawResult)>>;

fn merge_slot<'a>(mut a: Slot<'a>, b: Slot<'a>) -> Slot<'a> {
    for (event, persons) in b {
        let em = a.entry(event).or_default();
        for (person, (v, r)) in persons {
            let e = em.entry(person).or_insert((i32::MAX, r));
            if v < e.0 { *e = (v, r); }
        }
    }
    a
}

/// Sort `n` attempt values ascending, mapping DNF/DNS/0 → i32::MAX.
#[inline]
fn sort_attempts<const N: usize>(attempts: &[i32]) -> [i32; N] {
    let mut s = [i32::MAX; N];
    for (i, &a) in attempts.iter().enumerate().take(N) {
        if a > 0 { s[i] = a; }
    }
    s.sort_unstable();
    s
}

fn build_rankings<'a>(
    db: &'a WcaDb,
    slot: Slot<'a>,
) -> HashMap<String, Vec<Entry>> {
    let mut out = HashMap::new();

    for (event_id, person_map) in slot {
        let mut rows: Vec<(i32, &RawResult)> = person_map.into_values().collect();
        rows.sort_unstable_by(|(a, ra), (b, rb)| {
            let end_date = |r: &RawResult| {
                db.competitions
                    .get(r.competition_id.as_str())
                    .map(|c| (c.end_year as u32) * 10000 + (c.end_month as u32) * 100 + c.end_day as u32)
                    .unwrap_or(u32::MAX)
            };
            a.cmp(b)
                .then_with(|| end_date(ra).cmp(&end_date(rb)))
                .then_with(|| ra.person_id.cmp(&rb.person_id))
        });

        // Include up to rank 1000, preserving ties at the boundary.
        let cutoff = rows
            .iter()
            .enumerate()
            .find(|(i, _)| *i >= 1000 && rows[*i].0 != rows[i - 1].0)
            .map(|(i, _)| i)
            .unwrap_or(rows.len());
        let rows = &rows[..cutoff];

        let mut entries = Vec::with_capacity(rows.len());
        let mut rank = 1;
        for (i, (v, r)) in rows.iter().enumerate() {
            if i > 0 && *v != rows[i - 1].0 { rank = i + 1; }
            let name = db
                .persons
                .get(r.person_id.as_str())
                .map(|p| p.name.as_str())
                .unwrap_or(r.person_name.as_str());
            entries.push(Entry {
                rank,
                person_id: r.person_id.clone(),
                person_name: name.to_owned(),
                country_id: r.person_country_id.clone(),
                competition_id: r.competition_id.clone(),
                value_cs: *v,
                single_cs: r.best,
                average_cs: r.average,
            });
        }

        out.insert(event_id.to_owned(), entries);
    }

    out
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let n = STATS.len();

    // Single parallel fold: one pass over all 6.5M results, filling every slot.
    let slots: Vec<Slot<'_>> = db
        .results
        .par_iter()
        .fold(
            || vec![Slot::new(); n],
            |mut acc, result| {
                let is_ao5 = matches!(result.format_id.as_str(), "a" | "5");
                // Bo3 (format '3') rounds count for mo3 records officially.
                // Exclude multi-blind events whose attempt encoding is not centiseconds.
                let is_mo3 = matches!(result.format_id.as_str(), "m" | "3")
                    && !matches!(result.event_id.as_str(), "333mbf" | "333mbo");

                if !is_ao5 && !is_mo3 {
                    return acc;
                }
                // ao5 and standard mo3 require a valid average; Bo3 has average=0.
                if (is_ao5 || result.format_id == "m") && result.average <= 0 {
                    return acc;
                }

                let Some(attempts) = db.attempts.get(&result.id) else {
                    return acc;
                };

                if is_ao5 && attempts.len() == 5 {
                    let s = sort_attempts::<5>(attempts);
                    for (si, stat) in STATS.iter().enumerate().filter(|(_, s)| s.is_ao5) {
                        let v = s[stat.pos];
                        if v == i32::MAX { continue; }
                        let e = acc[si]
                            .entry(result.event_id.as_str()).or_default()
                            .entry(result.person_id.as_str()).or_insert((i32::MAX, result));
                        if v < e.0 { *e = (v, result); }
                    }
                }

                if is_mo3 && attempts.len() == 3 {
                    let s = sort_attempts::<3>(attempts);
                    for (si, stat) in STATS.iter().enumerate().filter(|(_, s)| !s.is_ao5) {
                        let v = s[stat.pos];
                        if v == i32::MAX { continue; }
                        let e = acc[si]
                            .entry(result.event_id.as_str()).or_default()
                            .entry(result.person_id.as_str()).or_insert((i32::MAX, result));
                        if v < e.0 { *e = (v, result); }
                    }
                }

                acc
            },
        )
        .reduce(
            || vec![Slot::new(); n],
            |mut a, b| {
                for (ai, bi) in a.iter_mut().zip(b) {
                    *ai = merge_slot(std::mem::take(ai), bi);
                }
                a
            },
        );

    // Build rankings and write one JSON file per stat.
    for (slot, stat) in slots.into_iter().zip(STATS.iter()) {
        let rankings = build_rankings(db, slot);

        let mut event_ids: Vec<&String> = rankings.keys().collect();
        event_ids.sort();
        eprint!("  {} — ", stat.label);
        for eid in &event_ids {
            if let Some(top) = rankings[*eid].first() {
                eprint!("{}: {}cs  ", eid, top.value_cs);
            }
        }
        eprintln!();

        let path = format!("{out_dir}/{}.json", stat.filename);
        let file = std::fs::File::create(&path)?;
        serde_json::to_writer(file, &rankings)?;
    }

    Ok(())
}
