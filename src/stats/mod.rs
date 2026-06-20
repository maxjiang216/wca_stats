use anyhow::Result;
use crate::db::WcaDb;

mod first_records;
mod kalman_skill;
mod mbld;
mod nations_cup;
mod nth_solve;
mod ranking_countries;
mod ranks_export;
mod relay;
mod skill_estimator;
pub mod solve_dist;
mod sub_x;
mod two_man;
mod wr_cross_rank;
mod wr_half_life;
mod wr_longevity;

pub fn run(db: &WcaDb, out_dir: &str) -> Result<()> {
    eprintln!("nth_solve");
    nth_solve::write(db, out_dir)?;
    eprintln!("mbld");
    mbld::write(db, out_dir)?;
    eprintln!("mbld_rankings");
    mbld::write_rankings(db, out_dir)?;
    eprintln!("ranking_countries");
    ranking_countries::write(db, out_dir)?;
    eprintln!("relay");
    relay::write(db, out_dir)?;
    eprintln!("ranks_export");
    ranks_export::write(db, out_dir)?;
    eprintln!("nations_cup");
    nations_cup::write(db, out_dir)?;
    eprintln!("two_man");
    two_man::write(db, out_dir)?;
    eprintln!("sub_x");
    sub_x::write(db, out_dir)?;
    eprintln!("wr_half_life");
    wr_half_life::write(db, out_dir)?;
    eprintln!("skill_estimator");
    skill_estimator::write(db, out_dir)?;
    eprintln!("kalman_skill");
    kalman_skill::write(db, out_dir)?;
    eprintln!("first_records");
    first_records::write(db, out_dir)?;
    eprintln!("wr_longevity");
    wr_longevity::write(db, out_dir)?;
    eprintln!("wr_cross_rank");
    wr_cross_rank::write(db, out_dir)?;
    Ok(())
}
