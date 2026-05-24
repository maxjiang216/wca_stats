import KinchRanksTable from '@/components/KinchRanksTable';

export default function KinchRanksPage() {
  return (
    <>
      <div className="page-header">
        <h1>KinchRanks</h1>
        <p className="desc">
          All-round ranking where each event score is WR÷PB×100. Higher is better; 100 = world record.
          Scores are averaged across selected events (missing events score 0). 3BF and FM use the best
          of single and average. MBLD uses an adjusted points formula.
        </p>
      </div>
      <KinchRanksTable />
    </>
  );
}
