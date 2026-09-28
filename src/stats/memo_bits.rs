//! Memorization Load — most memorization done in a single competition.
//!
//! For each blindfolded solve, "memorization load" is modeled as the log2 of
//! the number of distinct visual states of the puzzle a solver must be able
//! to tell apart to reconstruct it (an information-theoretic upper bound on
//! memo effort). Only solved (non-DNF) blind solves count for 333bf/444bf/
//! 555bf; for 333mbf, only successfully solved cubes count (not attempts).
//!
//! Derivation (see DEVLOG for the full piece-by-piece reasoning):
//!   - 3x3 (333bf): centers are physically fixed and anchor the frame.
//!       corners: 8! * 3^7 (last orientation forced mod 3)
//!       edges:   12!/2 * 2^11 (perm parity tied to corners, last flip forced mod 2)
//!   - 4x4 (444bf): no fixed center -> the whole solved cube can be picked up
//!     and viewed from 24 equivalent rotations, so divide by 24. Same-color
//!     center pieces are genuinely interchangeable (no visual difference) so
//!     they're reduced by 4!^6. Wing edges are NOT reduced: swapping the two
//!     wings of one edge is a real, visually distinguishable state — that's
//!     exactly what causes "OLL/PLL parity".
//!       corners: 8! * 3^7
//!       wings:   24! (fully distinguishable, no orientation d.o.f.)
//!       centers: 24!/(4!^6)
//!       / 24 (free whole-cube rotation)
//!   - 5x5 (555bf): true single-piece center exists again (anchors the
//!     frame, no /24). Two independent same-color-quad center types
//!     (X-centers, +-centers), each reduced like 4x4's centers. Odd cubes
//!     carry the standard global permutation-parity tie (this is exactly
//!     why odd cubes, unlike 4x4, don't suffer parity errors) -> a final /2.
//!       corners: 8! * 3^7
//!       wings:   24!
//!       centers: 2 * 24!/(4!^6)
//!       / 2 (global parity tie)
//!   - 333mbf: each cube is a 3x3, so each solved cube = the 333bf figure.

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

fn log2_fact(n: u64) -> f64 {
    (2..=n).map(|k| (k as f64).log2()).sum()
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

/// Inclusive day count of a competition (end_day - start_day + 1).
fn comp_days(c: &crate::db::models::RawCompetition) -> u32 {
    let start = ymd_to_jdn(c.year, c.month, c.day);
    let end = ymd_to_jdn(c.end_year, c.end_month, c.end_day);
    (end - start + 1).max(1) as u32
}

fn bits_3bf() -> f64 {
    let corners = log2_fact(8) + 7.0 * 3f64.log2();
    let edges = log2_fact(12) - 1.0 + 11.0;
    corners + edges
}

fn bits_4bf() -> f64 {
    let corners = log2_fact(8) + 7.0 * 3f64.log2();
    let wings = log2_fact(24);
    let centers = log2_fact(24) - 6.0 * log2_fact(4);
    corners + wings + centers - 24f64.log2()
}

fn bits_5bf() -> f64 {
    let corners = log2_fact(8) + 7.0 * 3f64.log2();
    let wings = log2_fact(24);
    let centers = log2_fact(24) - 6.0 * log2_fact(4);
    corners + wings + 2.0 * centers - 1.0
}

/// Decode a WCA MBLD encoded attempt value -> solved cube count, or None if
/// invalid/DNF/DNS.
fn mbld_solved(value: i32) -> Option<u32> {
    if value <= 0 {
        return None;
    }
    let missed = (value % 100) as u32;
    let points = 99 - (value / 10_000_000);
    if points <= 0 {
        return None;
    }
    Some(points as u32 + missed)
}

#[derive(Default)]
struct Totals {
    total_bits: f64,
    bf3_solves: u32,
    bf4_solves: u32,
    bf5_solves: u32,
    mbf_solved: u32,
}

#[derive(Serialize)]
struct Entry {
    rank: usize,
    person_id: String,
    person_name: String,
    country_id: String,
    competition_id: String,
    competition_name: String,
    date: String,
    days: u32,
    total_bits: f64,
    per_day_bits: f64,
    bf3_solves: u32,
    bf4_solves: u32,
    bf5_solves: u32,
    mbf_solved: u32,
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let b3 = bits_3bf();
    let b4 = bits_4bf();
    let b5 = bits_5bf();

    // (competition_id, person_id) -> running totals
    let mut agg: HashMap<(&str, &str), Totals> = HashMap::new();

    for r in &db.results {
        let bits_per_solve = match r.event_id.as_str() {
            "333bf" => b3,
            "444bf" => b4,
            "555bf" => b5,
            "333mbf" => 0.0, // handled separately below
            _ => continue,
        };

        let Some(attempts) = db.attempts.get(&r.id) else {
            continue;
        };
        let key = (r.competition_id.as_str(), r.person_id.as_str());

        if r.event_id == "333mbf" {
            for &v in attempts {
                if let Some(solved) = mbld_solved(v) {
                    let e = agg.entry(key).or_default();
                    e.total_bits += b3 * solved as f64;
                    e.mbf_solved += solved;
                }
            }
            continue;
        }

        for &v in attempts {
            if v <= 0 {
                continue; // DNF/DNS: no memorization credited
            }
            let e = agg.entry(key).or_default();
            e.total_bits += bits_per_solve;
            match r.event_id.as_str() {
                "333bf" => e.bf3_solves += 1,
                "444bf" => e.bf4_solves += 1,
                "555bf" => e.bf5_solves += 1,
                _ => unreachable!(),
            }
        }
    }

    let mk_entry = |rank: usize, comp_id: &str, person_id: &str, t: &Totals| -> Entry {
        let p = db.persons.get(person_id);
        let c = db.competitions.get(comp_id);
        let days = c.map(comp_days).unwrap_or(1);
        Entry {
            rank,
            person_id: person_id.to_string(),
            person_name: p.map(|p| p.name.clone()).unwrap_or_default(),
            country_id: p.map(|p| p.country_id.clone()).unwrap_or_default(),
            competition_id: comp_id.to_string(),
            competition_name: c.map(|c| c.name.clone()).unwrap_or_default(),
            date: c
                .map(|c| format!("{:04}-{:02}-{:02}", c.end_year, c.end_month, c.end_day))
                .unwrap_or_default(),
            days,
            total_bits: t.total_bits,
            per_day_bits: t.total_bits / days as f64,
            bf3_solves: t.bf3_solves,
            bf4_solves: t.bf4_solves,
            bf5_solves: t.bf5_solves,
            mbf_solved: t.mbf_solved,
        }
    };

    let rows: Vec<((&str, &str), Totals)> = agg.into_iter().collect();

    // most total memorization in a single competition
    let mut by_total = rows.iter().collect::<Vec<_>>();
    by_total.sort_unstable_by(|a, b| b.1.total_bits.partial_cmp(&a.1.total_bits).unwrap());
    by_total.truncate(100);
    let entries_total: Vec<Entry> = by_total
        .into_iter()
        .enumerate()
        .map(|(i, ((comp_id, person_id), t))| mk_entry(i + 1, comp_id, person_id, t))
        .collect();

    // most memorization per day of competition
    let mut by_day = rows.iter().collect::<Vec<_>>();
    by_day.sort_unstable_by(|a, b| {
        let da = db.competitions.get(a.0 .0).map(comp_days).unwrap_or(1);
        let db_ = db.competitions.get(b.0 .0).map(comp_days).unwrap_or(1);
        let ra = a.1.total_bits / da as f64;
        let rb = b.1.total_bits / db_ as f64;
        rb.partial_cmp(&ra).unwrap()
    });
    by_day.truncate(100);
    let entries_by_day: Vec<Entry> = by_day
        .into_iter()
        .enumerate()
        .map(|(i, ((comp_id, person_id), t))| mk_entry(i + 1, comp_id, person_id, t))
        .collect();

    eprintln!(
        "  memo_bits: {} total-entries, {} per-day-entries (bits/solve: 3bf={b3:.2} 4bf={b4:.2} 5bf={b5:.2})",
        entries_total.len(),
        entries_by_day.len()
    );

    #[derive(Serialize)]
    struct Output {
        bits_3bf: f64,
        bits_4bf: f64,
        bits_5bf: f64,
        by_total: Vec<Entry>,
        by_day: Vec<Entry>,
    }
    let out = Output {
        bits_3bf: b3,
        bits_4bf: b4,
        bits_5bf: b5,
        by_total: entries_total,
        by_day: entries_by_day,
    };
    serde_json::to_writer(std::fs::File::create(format!("{out_dir}/memo_bits.json"))?, &out)?;
    Ok(())
}
