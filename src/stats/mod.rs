use anyhow::Result;
use crate::db::WcaDb;

mod dominance;
mod first_records;
pub mod kalman_skill;
mod mbld;
mod memo_bits;
mod nations_cup;
mod nth_solve;
mod ranking_countries;
mod ranks_export;
mod relay;
mod skill_estimator;
pub mod solve_dist;
mod sub_x;
mod n_man;
mod team_common;
mod two_man;
mod wr_cross_rank;
mod wr_half_life;
mod wr_longevity;

/// Research diagnostics that only print to stderr (calibration probes,
/// hyperparameter sweeps, bias breakdowns) are expensive and off by default;
/// set `STATS_DIAG=1` to run them.
pub fn diag_enabled() -> bool {
    std::env::var_os("STATS_DIAG").is_some()
}

pub fn run(db: &WcaDb, out_dir: &str) -> Result<()> {
    if std::env::var("ONLY_TEAMS").is_ok() {
        two_man::write(db, out_dir)?;
        n_man::write(db, out_dir)?;
        return Ok(());
    }
    if std::env::var("ONLY_MEMO_BITS").is_ok() {
        eprintln!("memo_bits");
        memo_bits::write(db, out_dir)?;
        return Ok(());
    }
    let mut timings: Vec<(&str, std::time::Duration)> = Vec::new();
    macro_rules! stage {
        ($name:expr, $call:expr) => {{
            eprintln!("{}", $name);
            let t = std::time::Instant::now();
            $call?;
            let d = t.elapsed();
            eprintln!("  [{} took {:.2?}]", $name, d);
            timings.push(($name, d));
        }};
    }
    stage!("nth_solve", nth_solve::write(db, out_dir));
    stage!("mbld", mbld::write(db, out_dir));
    stage!("mbld_rankings", mbld::write_rankings(db, out_dir));
    stage!("ranking_countries", ranking_countries::write(db, out_dir));
    stage!("relay", relay::write(db, out_dir));
    stage!("ranks_export", ranks_export::write(db, out_dir));
    stage!("nations_cup", nations_cup::write(db, out_dir));
    stage!("two_man", two_man::write(db, out_dir));
    stage!("n_man", n_man::write(db, out_dir));
    stage!("sub_x", sub_x::write(db, out_dir));
    stage!("wr_half_life", wr_half_life::write(db, out_dir));
    stage!("skill_estimator", skill_estimator::write(db, out_dir));
    stage!("kalman_skill", kalman_skill::write(db, out_dir));
    stage!("first_records", first_records::write(db, out_dir));
    stage!("wr_longevity", wr_longevity::write(db, out_dir));
    stage!("wr_cross_rank", wr_cross_rank::write(db, out_dir));
    stage!("dominance", dominance::write(db, out_dir));
    stage!("memo_bits", memo_bits::write(db, out_dir));

    timings.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("\nStage timings (slowest first):");
    for (name, d) in &timings {
        eprintln!("  {name:<20} {d:>10.2?}");
    }
    Ok(())
}
