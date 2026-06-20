import WrCompareChart from '@/components/WrCompareChart';

export const metadata = { title: 'WR Compare: 3×3 vs SQ1 — WCA Stats' };

export default function WrComparePage() {
  return (
    <>
      <div className="page-header">
        <h1>WR Compare: 3×3 vs Square-1</h1>
        <p className="desc">
          World record progression for 3×3 and Square-1, single and average, on a shared log time
          axis. Each line steps down as a new record is set and holds until the next.
        </p>
      </div>
      <WrCompareChart />
    </>
  );
}
