use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;
use crate::db::models::RawResult;

/// Podiums kept per event per format group.
const TOP_N: usize = 100;

/// Format groups, each ranked by the value the format itself ranks by:
/// Best-of-X by single, Mo3/Ao5 by average. Head-to-head ('h') finals are
/// decided by matches, not times, so they're left out.
const GROUPS: [(&str, &[&str]); 3] = [("bo", &["1", "2", "3", "5"]), ("mo3", &["m"]), ("ao5", &["a"])];

/// Multi-blind values are packed points/time codes; summing them is meaningless.
const SKIP_EVENTS: [&str; 2] = ["333mbf", "333mbo"];

#[derive(Serialize)]
struct Member {
    person_id: String,
    person_name: String,
    country_id: String,
    pos: i32,
    value: i32,
}

#[derive(Serialize)]
struct Podium {
    rank: usize,
    competition_id: String,
    competition_name: String,
    date: String,
    format_id: String,
    total: i64,
    members: Vec<Member>,
}

/// The three podium finishers of one final, or None if fewer than three
/// placed or any of them lacks a valid ranking value (DNF average, etc.).
fn podium<'a>(rows: &mut Vec<&'a RawResult>, use_avg: bool) -> Option<Vec<&'a RawResult>> {
    rows.retain(|r| r.pos >= 1 && r.pos <= 3);
    if rows.len() < 3 {
        return None;
    }
    // Ties for 3rd (pos 1,2,3,3) share identical results, so any 3 give the same sum.
    rows.sort_by_key(|r| (r.pos, r.person_id.as_str()));
    rows.truncate(3);
    let value = |r: &RawResult| if use_avg { r.average } else { r.best };
    rows.iter().all(|r| value(r) > 0).then(|| rows.clone())
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let final_rounds: Vec<&str> = db
        .round_types
        .values()
        .filter(|rt| rt.is_final == 1)
        .map(|rt| rt.id.as_str())
        .collect();

    // (competition, event) -> final-round results.
    let mut finals: HashMap<(&str, &str), Vec<&RawResult>> = HashMap::new();
    for r in &db.results {
        if !final_rounds.contains(&r.round_type_id.as_str())
            || SKIP_EVENTS.contains(&r.event_id.as_str())
        {
            continue;
        }
        finals
            .entry((r.competition_id.as_str(), r.event_id.as_str()))
            .or_default()
            .push(r);
    }

    // group -> event -> podiums
    let mut out: HashMap<&str, HashMap<String, Vec<Podium>>> = HashMap::new();
    for ((comp_id, event_id), mut rows) in finals {
        let format_id = rows[0].format_id.clone();
        let Some(&(group, _)) = GROUPS.iter().find(|(_, fs)| fs.contains(&format_id.as_str())) else {
            continue;
        };
        let use_avg = group != "bo";
        let Some(top3) = podium(&mut rows, use_avg) else {
            continue;
        };
        let comp = db.competitions.get(comp_id);
        let members: Vec<Member> = top3
            .iter()
            .map(|r| Member {
                person_id: r.person_id.clone(),
                person_name: r.person_name.clone(),
                country_id: r.person_country_id.clone(),
                pos: r.pos,
                value: if use_avg { r.average } else { r.best },
            })
            .collect();
        out.entry(group).or_default().entry(event_id.to_string()).or_default().push(Podium {
            rank: 0,
            competition_id: comp_id.to_string(),
            competition_name: comp.map(|c| c.name.clone()).unwrap_or_default(),
            date: comp
                .map(|c| format!("{:04}-{:02}-{:02}", c.year, c.month, c.day))
                .unwrap_or_default(),
            format_id,
            total: members.iter().map(|m| m.value as i64).sum(),
            members,
        });
    }

    let mut n_podiums = 0;
    for events in out.values_mut() {
        for list in events.values_mut() {
            n_podiums += list.len();
            list.sort_by(|a, b| a.total.cmp(&b.total).then_with(|| a.date.cmp(&b.date)));
            list.truncate(TOP_N);
            // Competition ranking: equal totals share a rank.
            for i in 0..list.len() {
                list[i].rank = if i > 0 && list[i].total == list[i - 1].total {
                    list[i - 1].rank
                } else {
                    i + 1
                };
            }
        }
    }
    eprintln!("  best_podiums: {n_podiums} complete final podiums");

    let path = format!("{out_dir}/best_podiums.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &out)?;
    Ok(())
}
