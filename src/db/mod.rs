pub mod models;

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use csv::ReaderBuilder;
use models::*;

pub struct WcaDb {
    pub results: Vec<RawResult>,
    /// Sorted attempt values (by attempt_number) keyed by result id.
    pub attempts: HashMap<u32, Vec<i32>>,
    pub persons: HashMap<String, RawPerson>,
    pub competitions: HashMap<String, RawCompetition>,
    pub events: HashMap<String, RawEvent>,
    pub formats: HashMap<String, RawFormat>,
    pub countries: HashMap<String, RawCountry>,
    pub continents: HashMap<String, RawContinent>,
    pub round_types: HashMap<String, RawRoundType>,
    /// competition_id → list of championship types at that competition.
    pub championships: HashMap<String, Vec<String>>,
    pub ranks_single: HashMap<(String, String), RawRank>,
    pub ranks_average: HashMap<(String, String), RawRank>,
}

fn load_tsv<T>(path: &Path) -> Result<Vec<T>>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let mut rdr = ReaderBuilder::new().delimiter(b'\t').from_path(path)?;
    let mut out = Vec::new();
    for (i, record) in rdr.deserialize::<T>().enumerate() {
        match record {
            Ok(r) => out.push(r),
            Err(e) => eprintln!("  warn: row {i}: {e}"),
        }
    }
    Ok(out)
}

impl WcaDb {
    pub fn load(data_dir: &str) -> Result<Self> {
        let dir = Path::new(data_dir);

        macro_rules! load {
            ($label:expr, $file:expr, $ty:ty) => {{
                eprint!("Loading {:<24}", $label);
                let v: Vec<$ty> = load_tsv(&dir.join($file))?;
                eprintln!("{:>10} rows", v.len());
                v
            }};
        }

        let persons: HashMap<String, RawPerson> =
            load!("persons", "WCA_export_persons.tsv", RawPerson)
                .into_iter()
                .filter(|p| p.sub_id == 1)
                .map(|p| (p.wca_id.clone(), p))
                .collect();

        let competitions: HashMap<String, RawCompetition> =
            load!("competitions", "WCA_export_competitions.tsv", RawCompetition)
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect();

        let events: HashMap<String, RawEvent> =
            load!("events", "WCA_export_events.tsv", RawEvent)
                .into_iter()
                .map(|e| (e.id.clone(), e))
                .collect();

        let formats: HashMap<String, RawFormat> =
            load!("formats", "WCA_export_formats.tsv", RawFormat)
                .into_iter()
                .map(|f| (f.id.clone(), f))
                .collect();

        let countries: HashMap<String, RawCountry> =
            load!("countries", "WCA_export_countries.tsv", RawCountry)
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect();

        let continents: HashMap<String, RawContinent> =
            load!("continents", "WCA_export_continents.tsv", RawContinent)
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect();

        let round_types: HashMap<String, RawRoundType> =
            load!("round types", "WCA_export_round_types.tsv", RawRoundType)
                .into_iter()
                .map(|r| (r.id.clone(), r))
                .collect();

        let mut championships: HashMap<String, Vec<String>> = HashMap::new();
        for c in load!("championships", "WCA_export_championships.tsv", RawChampionship) {
            championships
                .entry(c.competition_id)
                .or_default()
                .push(c.championship_type);
        }

        let ranks_single: HashMap<(String, String), RawRank> =
            load!("ranks (single)", "WCA_export_ranks_single.tsv", RawRank)
                .into_iter()
                .map(|r| ((r.person_id.clone(), r.event_id.clone()), r))
                .collect();

        let ranks_average: HashMap<(String, String), RawRank> =
            load!("ranks (average)", "WCA_export_ranks_average.tsv", RawRank)
                .into_iter()
                .map(|r| ((r.person_id.clone(), r.event_id.clone()), r))
                .collect();

        eprint!("Loading {:<24}", "result attempts");
        let attempt_rows: Vec<RawResultAttempt> =
            load_tsv(&dir.join("WCA_export_result_attempts.tsv"))?;
        eprintln!("{:>10} rows", attempt_rows.len());
        let mut attempts: HashMap<u32, Vec<(u8, i32)>> = HashMap::new();
        for a in attempt_rows {
            attempts
                .entry(a.result_id)
                .or_default()
                .push((a.attempt_number, a.value));
        }
        let attempts: HashMap<u32, Vec<i32>> = attempts
            .into_iter()
            .map(|(id, mut v)| {
                v.sort_unstable_by_key(|(n, _)| *n);
                (id, v.into_iter().map(|(_, val)| val).collect())
            })
            .collect();

        let results = load!("results", "WCA_export_results.tsv", RawResult);

        Ok(WcaDb {
            results,
            attempts,
            persons,
            competitions,
            events,
            formats,
            countries,
            continents,
            round_types,
            championships,
            ranks_single,
            ranks_average,
        })
    }
}
