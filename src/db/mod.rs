pub mod models;

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use csv::ReaderBuilder;
use models::*;

/// Attempt values per result id, ordered by attempt_number, stored flat
/// (CSR): result id `i`'s attempts are `values[offsets[i]..offsets[i+1]]`.
/// Result ids are dense (max ≈ row count), so indexing by id directly beats
/// a HashMap of millions of tiny Vecs on both load time and memory.
pub struct Attempts {
    offsets: Vec<u32>,
    values: Vec<i32>,
}

impl Attempts {
    pub fn get(&self, id: &u32) -> Option<&[i32]> {
        let i = *id as usize;
        let (&s, &e) = (self.offsets.get(i)?, self.offsets.get(i + 1)?);
        (s != e).then(|| &self.values[s as usize..e as usize])
    }

    fn load(path: &Path) -> Result<Self> {
        let mut rdr = ReaderBuilder::new().delimiter(b'\t').from_path(path)?;
        let mut rows: Vec<(u32, u8, i32)> = Vec::new();
        let mut rec = csv::ByteRecord::new();
        while rdr.read_byte_record(&mut rec)? {
            // Columns: value, attempt_number, result_id
            rows.push((parse_int(&rec[2]) as u32, parse_int(&rec[1]) as u8, parse_int(&rec[0])));
        }
        // The export is grouped by result_id; only sort if that ever stops holding.
        if !rows.is_sorted_by_key(|r| r.0) {
            rows.sort_unstable_by_key(|r| r.0);
        }
        let max_id = rows.last().map_or(0, |r| r.0 as usize);
        let mut offsets = vec![0u32; max_id + 2];
        let mut values = Vec::with_capacity(rows.len());
        let mut i = 0;
        while i < rows.len() {
            let id = rows[i].0;
            let mut j = i;
            while j < rows.len() && rows[j].0 == id {
                j += 1;
            }
            let group = &mut rows[i..j];
            group.sort_unstable_by_key(|r| r.1);
            values.extend(group.iter().map(|r| r.2));
            offsets[id as usize + 1] = values.len() as u32;
            i = j;
        }
        // Ids with no attempts inherit the previous end, giving an empty range.
        for k in 1..offsets.len() {
            if offsets[k] < offsets[k - 1] {
                offsets[k] = offsets[k - 1];
            }
        }
        Ok(Attempts { offsets, values })
    }
}

fn parse_int(b: &[u8]) -> i32 {
    let (neg, digits) = match b.first() {
        Some(b'-') => (true, &b[1..]),
        _ => (false, b),
    };
    let v = digits.iter().fold(0i32, |acc, &c| acc * 10 + (c - b'0') as i32);
    if neg { -v } else { v }
}

pub struct WcaDb {
    pub results: Vec<RawResult>,
    pub attempts: Attempts,
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

        fn report(label: &str, n: usize) {
            eprintln!("Loading {label:<24}{n:>10} rows");
        }
        fn load_map<T, K>(path: &Path, label: &str, key: impl Fn(&T) -> K) -> Result<HashMap<K, T>>
        where
            T: for<'de> serde::Deserialize<'de>,
            K: std::hash::Hash + Eq,
        {
            let v: Vec<T> = load_tsv(path)?;
            report(label, v.len());
            Ok(v.into_iter().map(|t| (key(&t), t)).collect())
        }

        // The big files are independent: parse them concurrently.
        let loaded = std::thread::scope(|sc| {
            let results = sc.spawn(|| -> Result<Vec<RawResult>> {
                let v = load_tsv(&dir.join("WCA_export_results.tsv"))?;
                report("results", v.len());
                Ok(v)
            });
            let attempts = sc.spawn(|| -> Result<Attempts> {
                let a = Attempts::load(&dir.join("WCA_export_result_attempts.tsv"))?;
                report("result attempts", a.values.len());
                Ok(a)
            });
            let ranks_single = sc.spawn(|| {
                load_map(&dir.join("WCA_export_ranks_single.tsv"), "ranks (single)", |r: &RawRank| {
                    (r.person_id.clone(), r.event_id.clone())
                })
            });
            let ranks_average = sc.spawn(|| {
                load_map(&dir.join("WCA_export_ranks_average.tsv"), "ranks (average)", |r: &RawRank| {
                    (r.person_id.clone(), r.event_id.clone())
                })
            });
            let persons = sc.spawn(|| -> Result<HashMap<String, RawPerson>> {
                let v: Vec<RawPerson> = load_tsv(&dir.join("WCA_export_persons.tsv"))?;
                report("persons", v.len());
                Ok(v.into_iter().filter(|p| p.sub_id == 1).map(|p| (p.wca_id.clone(), p)).collect())
            });
            let competitions = sc.spawn(|| {
                load_map(&dir.join("WCA_export_competitions.tsv"), "competitions", |c: &RawCompetition| c.id.clone())
            });

            let small = (|| -> Result<_> {
                let events = load_map(&dir.join("WCA_export_events.tsv"), "events", |e: &RawEvent| e.id.clone())?;
                let formats = load_map(&dir.join("WCA_export_formats.tsv"), "formats", |f: &RawFormat| f.id.clone())?;
                let countries =
                    load_map(&dir.join("WCA_export_countries.tsv"), "countries", |c: &RawCountry| c.id.clone())?;
                let continents =
                    load_map(&dir.join("WCA_export_continents.tsv"), "continents", |c: &RawContinent| c.id.clone())?;
                let round_types =
                    load_map(&dir.join("WCA_export_round_types.tsv"), "round types", |r: &RawRoundType| r.id.clone())?;
                let champ_rows: Vec<RawChampionship> = load_tsv(&dir.join("WCA_export_championships.tsv"))?;
                report("championships", champ_rows.len());
                let mut championships: HashMap<String, Vec<String>> = HashMap::new();
                for c in champ_rows {
                    championships.entry(c.competition_id).or_default().push(c.championship_type);
                }
                Ok((events, formats, countries, continents, round_types, championships))
            })();

            let join = |what: &str| format!("{what} loader thread panicked");
            (
                results.join().expect(&join("results")),
                attempts.join().expect(&join("attempts")),
                ranks_single.join().expect(&join("ranks_single")),
                ranks_average.join().expect(&join("ranks_average")),
                persons.join().expect(&join("persons")),
                competitions.join().expect(&join("competitions")),
                small,
            )
        });
        let (results, attempts, ranks_single, ranks_average, persons, competitions, small) = loaded;
        let (results, attempts, ranks_single, ranks_average, persons, competitions) =
            (results?, attempts?, ranks_single?, ranks_average?, persons?, competitions?);
        let (events, formats, countries, continents, round_types, championships) = small?;

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
