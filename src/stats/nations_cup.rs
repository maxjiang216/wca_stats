use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;
use super::ranks_export::EVENTS;

#[derive(Serialize)]
struct Member {
    name: String,
    person_id: String,
    avg_cs: i32,
    comp: String,
}

#[derive(Serialize)]
struct CountryEntry {
    country: String,
    total: i64,
    members: Vec<Member>,
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    // Per-event, per-person intermediate storage.
    struct RawEntry {
        country: String,
        total: i64,
        members: Vec<(i32, String, String)>, // (value, person_id, name)
    }

    // Phase 1: for each event find the top-3 competitors per country.
    // Also build needed: event_id → person_id → (target_value, use_single).
    let mut needed: HashMap<String, HashMap<String, (i32, bool)>> = HashMap::new();
    let mut per_event: HashMap<&str, Vec<RawEntry>> = HashMap::new();

    for &event_id in EVENTS {
        let use_single = event_id == "333mbf";
        let ranks = if use_single { &db.ranks_single } else { &db.ranks_average };

        // country_id → Vec<(best_value, person_id, name)>
        let mut by_country: HashMap<&str, Vec<(i32, &str, &str)>> = HashMap::new();
        for ((person_id, ev_id), rank) in ranks {
            if ev_id.as_str() != event_id || rank.best <= 0 { continue; }
            let Some(person) = db.persons.get(person_id.as_str()) else { continue };
            by_country.entry(person.country_id.as_str())
                .or_default()
                .push((rank.best, person_id.as_str(), person.name.as_str()));
        }

        let ev_needed = needed.entry(event_id.to_string()).or_default();
        let mut entries: Vec<RawEntry> = Vec::new();

        for (country, mut members) in by_country {
            if members.len() < 3 { continue; }
            members.sort_by_key(|&(v, _, _)| v);
            members.truncate(3);
            let total: i64 = members.iter().map(|&(v, _, _)| v as i64).sum();
            let top3 = members.iter().map(|&(v, pid, name)| {
                ev_needed.insert(pid.to_string(), (v, use_single));
                (v, pid.to_string(), name.to_string())
            }).collect();
            entries.push(RawEntry { country: country.to_string(), total, members: top3 });
        }

        entries.sort_by_key(|e| e.total);
        per_event.insert(event_id, entries);
    }

    // Phase 2: scan results once to find competition IDs.
    // needed maps event → person → (target_value, use_single).
    // For each result row, do cheap &str lookups (no alloc unless we find a match).
    //
    // comp_found: event_id → person_id → competition_id
    let mut comp_found: HashMap<String, HashMap<String, String>> = HashMap::new();

    for r in &db.results {
        let Some(persons) = needed.get(r.event_id.as_str()) else { continue };
        let Some(&(target_val, use_single)) = persons.get(r.person_id.as_str()) else { continue };
        let val = if use_single { r.best } else { r.average };
        if val != target_val || val <= 0 { continue; }
        comp_found.entry(r.event_id.clone())
            .or_default()
            .entry(r.person_id.clone())
            .or_insert_with(|| r.competition_id.clone());
    }

    // Phase 3: assemble final output.
    let mut all_events: HashMap<&str, Vec<CountryEntry>> = HashMap::new();
    let empty: HashMap<String, String> = HashMap::new();

    for (&event_id, entries) in &per_event {
        let comps = comp_found.get(event_id).unwrap_or(&empty);
        let final_entries = entries.iter().map(|e| CountryEntry {
            country: e.country.clone(),
            total: e.total,
            members: e.members.iter().map(|(v, pid, name)| Member {
                name: name.clone(),
                person_id: pid.clone(),
                avg_cs: *v,
                comp: comps.get(pid).cloned().unwrap_or_default(),
            }).collect(),
        }).collect();
        all_events.insert(event_id, final_entries);
    }

    let max_countries = all_events.values().map(|v| v.len()).max().unwrap_or(0);
    eprintln!("  nations_cup — up to {max_countries} qualifying countries per event");

    let path = format!("{out_dir}/nations_cup.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &all_events)?;
    Ok(())
}
