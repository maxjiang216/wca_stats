use serde::{Deserialize, Deserializer};

fn null_str<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let s = String::deserialize(d)?;
    Ok(if s == "NULL" { None } else { Some(s) })
}

fn null_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    let s = String::deserialize(d)?;
    if s == "NULL" {
        Ok(None)
    } else {
        s.parse().map(Some).map_err(serde::de::Error::custom)
    }
}

/// One row per person per round. `best`/`average` are centiseconds;
/// -1 = DNF, -2 = DNS, 0 = not applicable (e.g. no average in format).
#[derive(Debug, Deserialize)]
pub struct RawResult {
    pub id: u32,
    pub pos: i32,
    pub best: i32,
    pub average: i32,
    pub competition_id: String,
    pub round_type_id: String,
    pub event_id: String,
    pub person_name: String,
    pub person_id: String,
    pub format_id: String,
    #[serde(deserialize_with = "null_str")]
    pub regional_single_record: Option<String>,
    #[serde(deserialize_with = "null_str")]
    pub regional_average_record: Option<String>,
    pub person_country_id: String,
}

/// Individual solve attempt linked to a result row.
/// `value` uses the same centisecond encoding as `best`/`average`.
#[derive(Debug, Deserialize)]
pub struct RawResultAttempt {
    pub value: i32,
    pub attempt_number: u8,
    pub result_id: u32,
}

/// One row per current name (sub_id == 1); sub_id > 1 rows are historical name changes.
#[derive(Debug, Deserialize)]
pub struct RawPerson {
    pub name: String,
    pub gender: String,
    pub wca_id: String,
    pub sub_id: u8,
    pub country_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RawCompetition {
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "null_str")]
    pub information: Option<String>,
    #[serde(deserialize_with = "null_str")]
    pub external_website: Option<String>,
    pub venue: String,
    pub city_name: String,
    pub country_id: String,
    pub venue_address: String,
    pub venue_details: String,
    pub cell_name: String,
    pub cancelled: u8,
    pub event_specs: String,
    pub delegates: String,
    pub organizers: String,
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub end_year: u16,
    pub end_month: u8,
    pub end_day: u8,
    #[serde(deserialize_with = "null_i64")]
    pub latitude_microdegrees: Option<i64>,
    #[serde(deserialize_with = "null_i64")]
    pub longitude_microdegrees: Option<i64>,
}

impl RawCompetition {
    pub fn events(&self) -> impl Iterator<Item = &str> {
        self.event_specs.split_whitespace()
    }
}

#[derive(Debug, Deserialize)]
pub struct RawRank {
    pub best: i32,
    pub person_id: String,
    pub event_id: String,
    pub world_rank: u32,
    pub continent_rank: u32,
    pub country_rank: u32,
}

#[derive(Debug, Deserialize)]
pub struct RawChampionship {
    pub id: u32,
    pub competition_id: String,
    pub championship_type: String,
}

#[derive(Debug, Deserialize)]
pub struct RawEvent {
    pub id: String,
    pub format: String,
    pub name: String,
    pub rank: u32,
}

#[derive(Debug, Deserialize)]
pub struct RawCountry {
    pub id: String,
    pub iso2: String,
    pub name: String,
    pub continent_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RawContinent {
    pub id: String,
    pub name: String,
    pub record_name: String,
}

#[derive(Debug, Deserialize)]
pub struct RawRoundType {
    pub id: String,
    #[serde(rename = "final")]
    pub is_final: u8,
    pub name: String,
    pub rank: u32,
    pub cell_name: String,
}

#[derive(Debug, Deserialize)]
pub struct RawFormat {
    pub id: String,
    pub expected_solve_count: u8,
    pub name: String,
    pub sort_by: String,
    pub sort_by_second: String,
    pub trim_fastest_n: u8,
    pub trim_slowest_n: u8,
}
