import WrLongevityTable from '@/components/WrLongevityTable';

export const metadata = { title: 'WR Longevity — WCA Stats' };

export default function WrLongevityPage() {
  return (
    <>
      <div className="page-header">
        <h1>WR Longevity</h1>
        <p className="desc">
          For each world record, how long was that result better than the global 10th and 100th
          best personal bests? A WR with a long top-10 run dominated the elite field; a short run
          means the competition caught up quickly.
        </p>
      </div>
      <WrLongevityTable />
    </>
  );
}
