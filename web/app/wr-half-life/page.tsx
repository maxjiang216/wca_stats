import WrHalfLifeChart from '@/components/WrHalfLifeChart';

export default function WrHalfLifePage() {
  return (
    <>
      <div className="page-header">
        <h1>World Record Half-Life</h1>
        <p className="desc">
          At each date, the number of days until at least half of the world records
          active on that date were no longer world records. Lower values mean records were
          falling rapidly; higher values mean the field had stabilised.
          Includes all singles and averages across every active WCA event.
        </p>
      </div>
      <WrHalfLifeChart />
    </>
  );
}
