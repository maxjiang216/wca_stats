import MbldRankingsTable from '@/components/MbldRankingsTable';

export const metadata = { title: 'MBLD N-Point Rankings — WCA Stats' };

export default function MbldRankingsPage() {
  return (
    <>
      <div className="page-header">
        <h1>MBLD N-Point Rankings</h1>
        <p className="desc">
          <strong>By Points:</strong> for each points value N, the fastest single attempts achieving
          exactly N points (solved − missed = N), ranked by time. The solved/attempted count may
          vary.{' '}
          <strong>By N/N:</strong> fastest clean attempts of exactly N cubes with zero misses,
          ranked by time.
        </p>
      </div>
      <MbldRankingsTable />
    </>
  );
}
