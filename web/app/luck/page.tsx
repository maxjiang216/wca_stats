import LuckTable from '@/components/LuckTable';

export default function LuckPage() {
  return (
    <>
      <div className="page-header">
        <h1>Top 100 Averages — Skill vs Luck</h1>
        <p className="desc">
          Per event (tabs above the table), the 100 fastest official averages in WCA history (best per person), each annotated with how
          lucky it was. Using the Kalman skill model we estimate the competitor&apos;s skill at every
          competition of their career, then ask: across <em>all</em> their ao5 attempts, how often
          would they produce an average at least this good? That is the <strong>career %</strong>.
          The record competition&apos;s own skill is estimated <strong>leave-one-out</strong> (from
          the surrounding comps, excluding the record week) so the result doesn&apos;t inflate its own
          chance; slow early-career comps contribute almost nothing automatically. Because a record is
          the actual best a career produced, a calibrated model gives this an average of about
          <strong> 50%</strong> — and across ~27,000 cubers ours averages exactly 50.0%. So 50% is
          par; a low percentage (under ~10%) marks a genuinely lucky record.
          &ldquo;σ&rdquo; is how many ao5 standard deviations the record sits below their skill, and
          <strong> Δ</strong> = skill-rank − ao5-rank: large positive Δ (orange/red) means the person
          ranks far higher on this one average than on sustained skill — propped up by a lucky result.
          Sort by ao5 rank, skill rank, or luckiest. Per-solve times are modeled with slightly lighter
          tails than reality, so extreme odds are a touch overstated.
        </p>
      </div>
      <LuckTable />
    </>
  );
}
