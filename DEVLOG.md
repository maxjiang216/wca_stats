# WCA Stats — Development Log

## Session: 2026-05-18

Three major features were added in this session: **Sub-X Rankings**, **WR Half-Life chart**, and a **Skill Estimator** with full bias analysis and predictive comparison.

---

## 1. Sub-X Rankings

**Pages:** `/sub-x`  
**Data:** `out/sub_x.json`  
**Files:** `src/stats/sub_x.rs`, `web/components/SubXTable.tsx`, `web/app/sub-x/page.tsx`

Counts how many times each competitor has gone sub-X in official WCA competitions. Covers a curated set of thresholds across eight events (3x3, 2x2, 4x4, 5x5, 6x6, 7x7, Pyraminx, Skewb) for both singles and averages where applicable.

The Rust backend uses a flat count array indexed by `(def_idx, person_idx)` for cache-friendly accumulation in a single pass through all results and attempts. The frontend supports toggling between single/average, switching thresholds, and showing top 100 or top 1000.

---

## 2. WR Half-Life

**Page:** `/wr-half-life`  
**Data:** `out/wr_half_life.json`  
**Files:** `src/stats/wr_half_life.rs`, `web/components/WrHalfLifeChart.tsx`, `web/app/wr-half-life/page.tsx`

At each date, computes the number of days until at least half of the world records active on that date were no longer world records. This measures how rapidly the competitive landscape was evolving.

### Algorithm

- For each active (event, single/average) pair, build a strictly improving WR timeline: a sequence of `(date_set, date_broken)` intervals, where `date_broken = ∞` for the current WR.
- Retired events (magic, mmagic) are excluded by filtering against `db.events`.
- Sample weekly. At each date `d`, find the WR active on `d` via binary search (`partition_point`), compute remaining life `= date_broken − d` (∞ if still standing), sort the remaining lives, and take the `⌈N/2⌉`-th value — the half-life.
- Data points where the median is still ∞ (i.e., fewer than half of WRs have been broken) are omitted.

### Output

2,263 weekly data points from 1982 to late 2025. Notable features visible in the chart:
- Pre-2003: only one WR tracked (3×3 single); half-life declines as the WR keeps falling.
- 2003 WCA era begins: jump to 16 WRs tracked simultaneously.
- COVID-19 spike (~2020): half-life peaks at ~883 days as competitions halted globally.
- Recent era: half-lives in the 200–300 day range.

### Visualisation

Custom SVG chart (no external library): log-scale Y axis (7 days to 20 years), linear fractional-year X axis, every-5-year grid lines, vertical annotations for "WCA era" (2003) and "COVID" (2020), interactive hover crosshair with tooltip showing date, formatted half-life, and number of WRs tracked.

---

## 3. Skill Estimator

**Page:** `/skill-estimator`  
**Data:** `out/skill_estimator.json`, `out/skill_estimator_comparison.json`  
**Files:** `src/stats/skill_estimator.rs`, `web/components/SkillEstimatorTable.tsx`, `web/app/skill-estimator/page.tsx`

An exponentially weighted moving average (EWMA) model of each competitor's current skill level, estimated from their competition history. Twelve events are supported (all active WCA events except blindfolded, FMC, and feet).

### Data Preparation

For each `(person, competition, event)` tuple, all individual solve attempts (from `db.attempts`) are collected. Non-DNF solves (`value > 0`) are averaged to produce a single `mean_cs` for that competition appearance. This aggregates across all rounds at the competition, so a person who competed in first round, semifinals, and final contributes one mean derived from all their valid solves that day. If no individual attempt data is available (rare for older results), the official `average` or `best` column is used as a fallback.

### EWMA Update Rule

The model maintains an estimate `µ` and an accumulated weight `w` for each competitor. When they compete at a new competition `Δt` days after the previous:

```
w_eff   = w · exp(−λ · Δt)          # old history decays
µ_new   = (w_eff · µ + mean_cs) / (w_eff + 1)
w_new   = w_eff + 1
```

The first competition initialises `µ = mean_cs`, `w = 1` with no prediction made. The prediction for subsequent competitions is the `µ` value entering that competition.

### Loss Function

The loss penalises prediction errors weighted by:

```
weight = n_solves / mean_cs
```

- `n_solves`: number of valid (non-DNF) solves that went into the competition mean — competitions with more solves carry more statistical weight.
- `1 / mean_cs`: corrects for scale across events (an error of 2 seconds matters more for a 5-second solver than a 40-second solver).

Total loss:

```
L(λ) = Σ  (actual_mean − µ_prediction)² · (n_solves / actual_mean)
```

### Optimal Decay Parameter

For each event, `λ` is found by **ternary search** on `[0, 0.05]` (100 iterations, converges to machine precision), minimising `L(λ)` over all competitors with at least two competitions.

Resulting half-lives:

| Event | λ (per day) | Half-life |
|-------|------------|-----------|
| 333, 555, minx | 0.050 | 14 days |
| 444 | 0.046 | 15 days |
| clock | 0.044 | 16 days |
| 333oh | 0.037 | 19 days |
| 777 | 0.032 | 22 days |
| 666 | 0.028 | 25 days |
| pyram | 0.027 | 26 days |
| 222 | 0.023 | 30 days |
| sq1 | 0.023 | 30 days |
| skewb | 0.022 | 31 days |

333, 555, and minx hit the upper boundary of the search range, suggesting their optimal decay may be even faster (recent form dominates for these events).

### Output

Top 1000 competitors per event, sorted by current EWMA estimate (lower = faster). The final estimate is `µ` after processing all of a person's competition history with optimal `λ`. Names and countries are taken from `db.persons` (current WCA registration).

### Frontend

Event tabs for all 12 included events. Table columns: rank, name (linked to WCA profile), country, EWMA estimate (formatted as a time), number of competitions, date of last competition. Top 100 shown by default; button to expand to 1000.

---

## 4. Bias Analysis

After fitting `λ`, the model's predictions are evaluated for systematic bias.

**Definition:** relative error = `(actual − prediction) / prediction`
- Positive: competitor performed slower than predicted.
- Negative: competitor performed faster than predicted (improved).

### By Speed (Quintiles of Predicted Value)

Across all events, the pattern is consistent: the model is nearly unbiased for fast competitors and substantially negatively biased for slow competitors.

Example (3×3):

| Quintile | Pred (~) | Bias |
|----------|----------|------|
| Q1 (fastest) | 10.0 s | +0.1% |
| Q2 | 13.5 s | −1.4% |
| Q3 | 17.2 s | −3.5% |
| Q4 | 24.0 s | −7.4% |
| Q5 (slowest) | 48.1 s | −13.6% |

### By Career Age

Also consistent across events: early-career competitors are predicted to perform much worse than they actually do.

Example (3×3):

| Career age | n predictions | Bias |
|------------|---------------|------|
| < 3 months | 66,671 | −10.6% |
| 3–6 months | 72,512 | −11.3% |
| 6–12 months | 118,779 | −8.3% |
| 1–2 years | 150,135 | −4.8% |
| 2–5 years | 179,247 | −2.8% |
| 5+ years | 163,116 | −0.8% |

### Root Cause

Both biases trace to the same source: **the EWMA treats skill as stationary, but newer and slower competitors are actively on an improvement curve**. The model cannot distinguish "stable at this level" from "still improving." Elite competitors with long careers (the group shown on the rankings page) are in the well-calibrated 5+ year / Q1 regime.

A fix would require adding an explicit trend term (e.g., Holt's double exponential smoothing: estimate both a level and a slope).

---

## 5. Method Comparison: EWMA vs PB for Predicting Competition Winners

### Setup

For each competition final (identified by `round_type_id` with `is_final = 1`), two predictions are made:

- **EWMA method**: among the finalists who have appeared in this event before (≥ 2 prior competitions), the one with the lowest EWMA estimate going into this competition.
- **PB method**: among the finalists who have a prior official average (ao5/mo3) in this event, the one with the lowest personal best going into this competition.

Only competitions where the two methods predict **different** winners are counted, weighted by `1 / winner_avg` so that elite competitions (faster winner) have greater influence. Both methods are assessed only among finalists with known estimates — first-timers cannot be predicted.

### Results

| Event | Disagreements | EWMA | PB | Winner |
|-------|--------------|------|----|--------|
| Square-1 | 1,199 | **59.6%** | 40.4% | EWMA |
| Megaminx | 925 | **58.6%** | 41.4% | EWMA |
| Skewb | 2,247 | **55.3%** | 44.7% | EWMA |
| Pyraminx | 2,611 | **53.7%** | 46.3% | EWMA |
| 3×3 OH | 2,072 | **53.0%** | 47.0% | EWMA |
| 7×7 | 501 | **52.8%** | 47.2% | EWMA |
| 6×6 | 662 | **52.4%** | 47.6% | EWMA |
| 5×5 | 1,157 | **52.3%** | 47.7% | EWMA |
| 4×4 | 2,044 | **50.5%** | 49.5% | EWMA |
| 3×3 | 3,136 | 49.6% | **50.4%** | PB |
| 2×2 | 3,769 | 49.0% | **51.0%** | PB |
| Clock | 1,414 | 47.6% | **52.4%** | PB |
| **Overall** | **21,737** | **51.5%** | **48.5%** | **EWMA** |

### Interpretation

EWMA wins overall (51.5% vs 48.5%). The advantage is largest for events with fewer competitions per year (sq1, minx, skewb, pyram): because competitors compete infrequently, a PB from 18 months ago is less representative of current ability than a recent EWMA. The EWMA's recency weighting is most valuable here.

PB wins for 3×3, 2×2, and Clock — the most densely competed events. At the elite level of these events, competitors compete so frequently that their PB remains current, making it a reliable predictor. The 3×3 and 2×2 results are essentially coin-flip territory (≈50/50), confirming that both methods are roughly equivalent for the main events.

---

## Files Changed

### Rust (`src/`)

| File | Change |
|------|--------|
| `src/stats/mod.rs` | Added `mod sub_x`, `mod wr_half_life`, `mod skill_estimator` and their `write()` calls |
| `src/stats/sub_x.rs` | New — Sub-X rankings stat |
| `src/stats/wr_half_life.rs` | New — WR half-life computation |
| `src/stats/skill_estimator.rs` | New — EWMA skill estimator, bias analysis, method comparison |

### Web (`web/`)

| File | Change |
|------|--------|
| `web/components/SubXTable.tsx` | New — Sub-X rankings table component |
| `web/components/WrHalfLifeChart.tsx` | New — WR half-life SVG chart component |
| `web/components/SkillEstimatorTable.tsx` | New — Skill estimator rankings table |
| `web/app/sub-x/page.tsx` | New |
| `web/app/wr-half-life/page.tsx` | New |
| `web/app/skill-estimator/page.tsx` | New |
| `web/components/Sidebar.tsx` | Added links for all three new pages |

### Output files (generated, not committed)

- `out/sub_x.json` — Sub-X ranking data
- `out/wr_half_life.json` — 2,263 weekly half-life data points (111 KB)
- `out/skill_estimator.json` — Top 1,000 per event × 12 events (1.5 MB)
- `out/skill_estimator_comparison.json` — Per-event EWMA vs PB comparison scores
