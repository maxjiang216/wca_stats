use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

// Continental record labels in the WCA export.
const CONTINENTAL: &[&str] = &["AfR", "AsR", "ER", "NAR", "OcR", "SAR"];

fn is_wr(label: Option<&str>) -> bool {
    label == Some("WR")
}
fn is_cr(label: Option<&str>) -> bool {
    CONTINENTAL.iter().any(|&c| label == Some(c))
}
fn is_wr_or_cr(label: Option<&str>) -> bool {
    is_wr(label) || is_cr(label)
}
fn record_level(label: Option<&str>) -> &'static str {
    if is_wr(label) { "WR" } else { "CR" }
}

fn ymd_to_jdn(year: u16, month: u8, day: u8) -> i32 {
    let y = year as i32;
    let m = month as i32;
    let d = day as i32;
    let a = (14 - m) / 12;
    let y2 = y + 4800 - a;
    let m2 = m + 12 * a - 3;
    d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045
}

fn jdn_to_iso(j: i32) -> String {
    let a = j + 32044;
    let b = (4 * a + 3) / 146097;
    let c = a - (146097 * b) / 4;
    let d2 = (4 * c + 3) / 1461;
    let e = c - (1461 * d2) / 4;
    let m = (5 * e + 2) / 153;
    let day = e - (153 * m + 2) / 5 + 1;
    let month = m + 3 - 12 * (m / 10);
    let year = 100 * b + d2 - 4800 + m / 10;
    format!("{year:04}-{month:02}-{day:02}")
}

#[derive(Serialize)]
pub struct FirstRecord {
    pub name: String,
    pub pid: String,
    pub date: String,
    pub comp: String,
    pub record: &'static str, // "WR" or "CR"
    pub value: i32,
}

/// (country_id, event_id) → FirstRecord
type Table = HashMap<String, HashMap<String, FirstRecord>>;

/// Compare two candidates: prefer earlier date, then WR over CR, then lower value.
struct Candidate {
    jdn: i32,
    name: String,
    pid: String,
    comp_id: String,
    record: &'static str,
    value: i32,
}

fn better(new: &Candidate, old: &Candidate) -> bool {
    if new.jdn < old.jdn { return true; }
    if new.jdn > old.jdn { return false; }
    // Same date: prefer WR over CR.
    if new.record == "WR" && old.record != "WR" { return true; }
    if old.record == "WR" && new.record != "WR" { return false; }
    // Same date, same level: prefer better (lower) value.
    new.value < old.value
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let comp_day: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    // (country, event) → best candidate so far
    let mut single_best: HashMap<(String, String), Candidate> = HashMap::new();
    let mut avg_best:    HashMap<(String, String), Candidate> = HashMap::new();

    for r in &db.results {
        let Some(&jdn) = comp_day.get(r.competition_id.as_str()) else { continue; };

        // Single
        if is_wr_or_cr(r.regional_single_record.as_deref()) && r.best > 0 {
            let cand = Candidate {
                jdn,
                name: r.person_name.clone(),
                pid:  r.person_id.clone(),
                comp_id: r.competition_id.clone(),
                record: record_level(r.regional_single_record.as_deref()),
                value: r.best,
            };
            let key = (r.person_country_id.clone(), r.event_id.clone());
            match single_best.get(&key) {
                None => { single_best.insert(key, cand); }
                Some(old) if better(&cand, old) => { single_best.insert(key, cand); }
                _ => {}
            }
        }

        // Average
        if is_wr_or_cr(r.regional_average_record.as_deref()) && r.average > 0 {
            let cand = Candidate {
                jdn,
                name: r.person_name.clone(),
                pid:  r.person_id.clone(),
                comp_id: r.competition_id.clone(),
                record: record_level(r.regional_average_record.as_deref()),
                value: r.average,
            };
            let key = (r.person_country_id.clone(), r.event_id.clone());
            match avg_best.get(&key) {
                None => { avg_best.insert(key, cand); }
                Some(old) if better(&cand, old) => { avg_best.insert(key, cand); }
                _ => {}
            }
        }
    }

    // Build output tables.
    let mut single: Table = HashMap::new();
    let mut avg:    Table = HashMap::new();

    for ((country, event), c) in single_best {
        single
            .entry(country)
            .or_default()
            .insert(event, FirstRecord {
                name: c.name, pid: c.pid,
                date: jdn_to_iso(c.jdn),
                comp: c.comp_id,
                record: c.record, value: c.value,
            });
    }
    for ((country, event), c) in avg_best {
        avg
            .entry(country)
            .or_default()
            .insert(event, FirstRecord {
                name: c.name, pid: c.pid,
                date: jdn_to_iso(c.jdn),
                comp: c.comp_id,
                record: c.record, value: c.value,
            });
    }

    // Collect events that appear in either table, preserving WCA display order.
    let wca_order = [
        "333","222","444","555","666","777",
        "333bf","333fm","333oh","clock","minx","pyram","skewb","sq1",
        "444bf","555bf","333mbf","333mbo","333ft","magic","mmagic",
    ];
    let mut event_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    for map in single.values().chain(avg.values()) {
        event_set.extend(map.keys().cloned());
    }
    let mut events: Vec<String> = wca_order.iter()
        .filter(|e| event_set.contains(**e))
        .map(|e| e.to_string())
        .collect();
    // Any events not in wca_order go at the end.
    for e in &event_set {
        if !wca_order.contains(&e.as_str()) { events.push(e.clone()); }
    }

    // Country names.
    let countries: HashMap<String, String> = db.countries.iter()
        .map(|(id, c)| (id.clone(), c.name.clone()))
        .collect();

    // Also collect person_country_id values that aren't in db.countries (historical).
    let all_country_ids: std::collections::HashSet<String> = single.keys()
        .chain(avg.keys())
        .cloned()
        .collect();

    #[derive(Serialize)]
    struct Output {
        events: Vec<String>,
        countries: HashMap<String, String>,
        single: Table,
        average: Table,
    }

    let mut country_names = countries;
    for cid in &all_country_ids {
        country_names.entry(cid.clone()).or_insert_with(|| cid.clone());
    }

    eprintln!(
        "  first_records: {} countries with single records, {} with average records, {} events",
        single.len(), avg.len(), events.len()
    );

    let out = Output { events, countries: country_names, single, average: avg };
    let path = format!("{out_dir}/first_records.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &out)?;
    Ok(())
}
