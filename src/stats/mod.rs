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
mod team_common;
mod three_man;
mod two_man;
mod wr_cross_rank;
mod wr_half_life;
mod wr_longevity;

pub fn run(db: &WcaDb, out_dir: &str) -> Result<()> {
    if std::env::var("ONLY_THREE_MAN").is_ok() {
        eprintln!("two_man");
        two_man::write(db, out_dir)?;
        eprintln!("three_man");
        three_man::write(db, out_dir)?;
        return Ok(());
    }
    if std::env::var("DEBUG_POOL_MINI_2").is_ok() {
        three_man::debug_pool_size(db, true, 2);
        return Ok(());
    }
    if std::env::var("DEBUG_POOL_GUILD_2").is_ok() {
        three_man::debug_pool_size(db, false, 2);
        return Ok(());
    }
    if std::env::var("DEBUG_POOL_MINI_3").is_ok() {
        three_man::debug_pool_size(db, true, 3);
        return Ok(());
    }
    if std::env::var("DEBUG_POOL_GUILD_3").is_ok() {
        three_man::debug_pool_size(db, false, 3);
        return Ok(());
    }
    if std::env::var("DEBUG_POOL_MINI_4").is_ok() {
        three_man::debug_pool_size(db, true, 4);
        return Ok(());
    }
    if std::env::var("DEBUG_POOL_GUILD_4").is_ok() {
        three_man::debug_pool_size(db, false, 4);
        return Ok(());
    }
    if std::env::var("DEBUG_FOUR_MAN_MINI").is_ok() {
        // Seeded from the already-known 3-man mini-Guildford optimum (3782cs).
        three_man::debug_four_man(db, true, None, 3782);
        return Ok(());
    }
    if std::env::var("DEBUG_FOUR_MAN_GUILD").is_ok() {
        // Seeded from the already-known 3-man Guildford optimum (10065cs).
        three_man::debug_four_man(db, false, None, 10065);
        return Ok(());
    }
    if std::env::var("ONLY_MEMO_BITS").is_ok() {
        eprintln!("memo_bits");
        memo_bits::write(db, out_dir)?;
        return Ok(());
    }
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
    eprintln!("three_man");
    three_man::write(db, out_dir)?;
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
    eprintln!("dominance");
    dominance::write(db, out_dir)?;
    eprintln!("memo_bits");
    memo_bits::write(db, out_dir)?;
    Ok(())
}
