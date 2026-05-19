mod db;
mod stats;

use anyhow::Result;
use db::WcaDb;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().unwrap_or_else(|| "data".to_string());
    let out_dir = args.next().unwrap_or_else(|| "out".to_string());

    let t0 = std::time::Instant::now();
    let db = WcaDb::load(&data_dir)?;
    eprintln!("Load: {:.2?}", t0.elapsed());

    eprintln!();
    eprintln!("Database summary:");
    eprintln!("  {:>8} results", db.results.len());
    eprintln!("  {:>8} persons", db.persons.len());
    eprintln!("  {:>8} competitions", db.competitions.len());
    eprintln!("  {:>8} events", db.events.len());

    std::fs::create_dir_all(&out_dir)?;
    let t1 = std::time::Instant::now();
    stats::run(&db, &out_dir)?;
    eprintln!("Stats: {:.2?}", t1.elapsed());

    Ok(())
}
