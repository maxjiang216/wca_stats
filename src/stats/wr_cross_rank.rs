use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::WcaDb;

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

/// Fenwick tree over compressed value indices, counting current PBs.
struct Bit {
    t: Vec<i64>,
}
impl Bit {
    fn new(n: usize) -> Self {
        Bit { t: vec![0; n + 1] }
    }
    fn add(&mut self, i: usize, delta: i64) {
        let mut i = i + 1;
        while i < self.t.len() {
            self.t[i] += delta;
            i += i & i.wrapping_neg();
        }
    }
    /// Sum over indices [0, i).
    fn prefix(&self, i: usize) -> i64 {
        let mut s = 0;
        let mut i = i;
        while i > 0 {
            s += self.t[i];
            i -= i & i.wrapping_neg();
        }
        s
    }
}

#[derive(Serialize)]
struct GraphPoint {
    date: String,
    value: i32,
    rank: u32,
    pool: u32,
}

#[derive(Serialize)]
struct TableRow {
    date: String,
    name: String,
    pid: String,
    comp: String,
    value: i32,
    rank: u32,
    pool: u32,
}

#[derive(Serialize)]
struct Comparison {
    id: String,
    group: String, // "avg_single" | "nxn"
    title: String,
    subject_event: String,
    subject_type: String, // "single" | "average"
    ref_event: String,
    ref_type: String,
    graph: Vec<GraphPoint>,
    table: Vec<TableRow>,
}

/// One WR step: the moment a new world record (single or average) was set.
type WrStep = (i32, i32, String, String, String); // (jdn, value, name, pid, comp)

/// Reduce raw WR rows to a strictly-improving sequence, one entry per
/// improvement, keeping the setter of the best result on each day.
fn build_steps(mut raw: Vec<WrStep>) -> Vec<WrStep> {
    raw.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut steps: Vec<WrStep> = Vec::new();
    let mut cur_best = i32::MAX;
    let mut i = 0;
    while i < raw.len() {
        let jdn = raw[i].0;
        // Best (lowest) value on this day is first after the sort above.
        let day_best = &raw[i];
        if day_best.1 < cur_best {
            cur_best = day_best.1;
            steps.push(day_best.clone());
        }
        // Skip the rest of this day.
        while i < raw.len() && raw[i].0 == jdn {
            i += 1;
        }
    }
    steps
}

/// For each query (jdn, value), rank that value among the reference pool's
/// personal bests as they stood on or before that date.
/// `ref_pool` is (jdn, person_idx, value) sorted ascending by jdn.
/// Returns (rank, pool_size) per query, in query order.
fn rank_queries(ref_pool: &[(i32, u32, i32)], queries: &[(i32, i32)]) -> Vec<(u32, u32)> {
    if ref_pool.is_empty() {
        return queries.iter().map(|_| (0, 0)).collect();
    }

    // Coordinate-compress reference values.
    let mut vals: Vec<i32> = ref_pool.iter().map(|&(_, _, v)| v).collect();
    vals.sort_unstable();
    vals.dedup();

    let mut order: Vec<usize> = (0..queries.len()).collect();
    order.sort_by_key(|&i| queries[i].0);

    let mut out = vec![(0u32, 0u32); queries.len()];
    let mut bit = Bit::new(vals.len());
    let mut pb: HashMap<u32, i32> = HashMap::new();
    let mut p = 0usize;

    for &qi in &order {
        let (qjdn, qval) = queries[qi];
        while p < ref_pool.len() && ref_pool[p].0 <= qjdn {
            let (_, pidx, v) = ref_pool[p];
            let entry = pb.entry(pidx).or_insert(i32::MAX);
            if v < *entry {
                if *entry != i32::MAX {
                    let oi = vals.binary_search(entry).unwrap();
                    bit.add(oi, -1);
                }
                let ni = vals.binary_search(&v).unwrap();
                bit.add(ni, 1);
                *entry = v;
            }
            p += 1;
        }
        // Number of PBs strictly faster than qval.
        let less = vals.partition_point(|&x| x < qval);
        let faster = bit.prefix(less) as u32;
        out[qi] = (faster + 1, pb.len() as u32);
    }
    out
}

/// Subject value active on date `d`, from a strictly-improving timeline.
fn value_at(timeline: &[(i32, i32)], d: i32) -> Option<i32> {
    let idx = timeline.partition_point(|&(j, _)| j <= d);
    if idx == 0 {
        None
    } else {
        Some(timeline[idx - 1].1)
    }
}

pub fn write(db: &WcaDb, out_dir: &str) -> Result<()> {
    let person_ids: Vec<&str> = db.persons.keys().map(String::as_str).collect();
    let person_idx: HashMap<&str, u32> = person_ids
        .iter()
        .enumerate()
        .map(|(i, &s)| (s, i as u32))
        .collect();

    let comp_day: HashMap<&str, i32> = db
        .competitions
        .iter()
        .map(|(id, c)| (id.as_str(), ymd_to_jdn(c.year, c.month, c.day)))
        .collect();

    // Use the real current date, not max(comp_day) — the export contains
    // pre-registered future competitions.
    let today_jdn = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        2440588 + (secs / 86400) as i32
    };

    // Events we need pools / WR steps for. All store centiseconds for both
    // single and average, so cross-ranking is unit-consistent.
    let needed: std::collections::HashSet<&str> = [
        "222", "333", "444", "555", "666", "777", "333oh", "minx", "pyram", "skewb", "sq1",
        "clock", "444bf", "555bf",
    ]
    .into_iter()
    .collect();

    // Per-event reference pools and WR step rows.
    let mut pool_single: HashMap<&str, Vec<(i32, u32, i32)>> = HashMap::new();
    let mut pool_avg: HashMap<&str, Vec<(i32, u32, i32)>> = HashMap::new();
    let mut wr_single: HashMap<&str, Vec<WrStep>> = HashMap::new();
    let mut wr_avg: HashMap<&str, Vec<WrStep>> = HashMap::new();

    for r in &db.results {
        let eid = r.event_id.as_str();
        if !needed.contains(eid) {
            continue;
        }
        let Some(&jdn) = comp_day.get(r.competition_id.as_str()) else {
            continue;
        };
        let Some(&pidx) = person_idx.get(r.person_id.as_str()) else {
            continue;
        };

        if r.best > 0 {
            pool_single.entry(eid).or_default().push((jdn, pidx, r.best));
            if r.regional_single_record.as_deref() == Some("WR") {
                wr_single.entry(eid).or_default().push((
                    jdn,
                    r.best,
                    r.person_name.clone(),
                    r.person_id.clone(),
                    r.competition_id.clone(),
                ));
            }
        }
        if r.average > 0 {
            pool_avg.entry(eid).or_default().push((jdn, pidx, r.average));
            if r.regional_average_record.as_deref() == Some("WR") {
                wr_avg.entry(eid).or_default().push((
                    jdn,
                    r.average,
                    r.person_name.clone(),
                    r.person_id.clone(),
                    r.competition_id.clone(),
                ));
            }
        }
    }

    for v in pool_single.values_mut() {
        v.sort_unstable_by_key(|&(j, ..)| j);
    }
    for v in pool_avg.values_mut() {
        v.sort_unstable_by_key(|&(j, ..)| j);
    }

    // Build one comparison from a subject WR series ranked against a ref pool.
    let build = |id: String,
                 group: &str,
                 title: String,
                 subject_event: &str,
                 subject_type: &str,
                 ref_event: &str,
                 ref_type: &str,
                 steps_raw: Option<&Vec<WrStep>>,
                 ref_pool: Option<&Vec<(i32, u32, i32)>>|
     -> Option<Comparison> {
        let steps = build_steps(steps_raw?.clone());
        if steps.is_empty() {
            return None;
        }
        let ref_pool = ref_pool?;
        if ref_pool.is_empty() {
            return None;
        }

        // Strictly-improving timeline for sampling.
        let timeline: Vec<(i32, i32)> = steps.iter().map(|s| (s.0, s.1)).collect();

        // Table: rank each historical WR the week it was set.
        let table_q: Vec<(i32, i32)> = steps.iter().map(|s| (s.0, s.1)).collect();
        let table_r = rank_queries(ref_pool, &table_q);
        let table: Vec<TableRow> = steps
            .iter()
            .zip(table_r)
            .map(|(s, (rank, pool))| TableRow {
                date: jdn_to_iso(s.0),
                name: s.2.clone(),
                pid: s.3.clone(),
                comp: s.4.clone(),
                value: s.1,
                rank,
                pool,
            })
            .collect();

        // Graph: weekly samples of the then-current WR ranked against the
        // then-current reference field.
        let first = timeline[0].0;
        let mut graph_q: Vec<(i32, i32)> = Vec::new();
        let mut d = first;
        while d <= today_jdn {
            if let Some(v) = value_at(&timeline, d) {
                graph_q.push((d, v));
            }
            d += 7;
        }
        let graph_r = rank_queries(ref_pool, &graph_q);
        let graph: Vec<GraphPoint> = graph_q
            .iter()
            .zip(graph_r)
            .filter(|(_, (_, pool))| *pool > 0)
            .map(|(&(d, v), (rank, pool))| GraphPoint {
                date: jdn_to_iso(d),
                value: v,
                rank,
                pool,
            })
            .collect();

        Some(Comparison {
            id,
            group: group.to_string(),
            title,
            subject_event: subject_event.to_string(),
            subject_type: subject_type.to_string(),
            ref_event: ref_event.to_string(),
            ref_type: ref_type.to_string(),
            graph,
            table,
        })
    };

    let mut comparisons: Vec<Comparison> = Vec::new();

    // Group 1: each event's WR average ranked among that event's singles.
    let av_events = [
        "333", "222", "444", "555", "666", "777", "333oh", "minx", "pyram", "skewb", "sq1",
        "clock",
    ];
    for &e in &av_events {
        if let Some(c) = build(
            format!("av_{e}"),
            "avg_single",
            format!("{e} WR average ranked among {e} singles"),
            e,
            "average",
            e,
            "single",
            wr_avg.get(e),
            pool_single.get(e),
        ) {
            comparisons.push(c);
        }
    }

    // Group 2: NxN ranked among (N-1)x(N-1), singles vs singles and averages vs averages.
    let nxn_pairs = [
        ("333", "222"),
        ("444", "333"),
        ("555", "444"),
        ("666", "555"),
        ("777", "666"),
    ];
    for &(big, small) in &nxn_pairs {
        if let Some(c) = build(
            format!("nxn_single_{big}"),
            "nxn",
            format!("{big} WR single ranked among {small} singles"),
            big,
            "single",
            small,
            "single",
            wr_single.get(big),
            pool_single.get(small),
        ) {
            comparisons.push(c);
        }
        if let Some(c) = build(
            format!("nxn_avg_{big}"),
            "nxn",
            format!("{big} WR average ranked among {small} averages"),
            big,
            "average",
            small,
            "average",
            wr_avg.get(big),
            pool_avg.get(small),
        ) {
            comparisons.push(c);
        }
    }

    // Group 3: hand-picked cross-event pairs (subject ranked among reference),
    // for both single and average/mean.
    let cross_pairs = [
        ("333oh", "333"), // one-handed vs two-handed
        ("555bf", "444bf"), // 5BLD vs 4BLD
    ];
    for &(big, small) in &cross_pairs {
        if let Some(c) = build(
            format!("cross_single_{big}"),
            "cross",
            format!("{big} WR single ranked among {small} singles"),
            big,
            "single",
            small,
            "single",
            wr_single.get(big),
            pool_single.get(small),
        ) {
            comparisons.push(c);
        }
        if let Some(c) = build(
            format!("cross_avg_{big}"),
            "cross",
            format!("{big} WR average ranked among {small} averages"),
            big,
            "average",
            small,
            "average",
            wr_avg.get(big),
            pool_avg.get(small),
        ) {
            comparisons.push(c);
        }
    }

    eprintln!("  wr_cross_rank: {} comparisons", comparisons.len());
    for c in &comparisons {
        eprintln!(
            "    {} — {} graph pts, {} table rows",
            c.id,
            c.graph.len(),
            c.table.len()
        );
    }

    #[derive(Serialize)]
    struct Output {
        comparisons: Vec<Comparison>,
    }

    let out = Output { comparisons };
    let path = format!("{out_dir}/wr_cross_rank.json");
    serde_json::to_writer(std::fs::File::create(&path)?, &out)?;
    Ok(())
}
