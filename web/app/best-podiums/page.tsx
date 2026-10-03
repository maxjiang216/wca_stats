import BestPodiumsTable from '@/components/BestPodiumsTable';

export const metadata = { title: 'Best Podiums — WCA Stats' };

export default function BestPodiumsPage() {
  return (
    <>
      <div className="page-header">
        <h1>Best Podiums</h1>
        <p className="desc">
          Fastest final-round podiums: the sum of the 1st, 2nd and 3rd place results, using
          whichever result the round was ranked by — the single for Best-of-X rounds, the
          average for Mo3 and Ao5 rounds. Podiums with a missing or DNF ranking result are
          skipped. Head-to-head finals and Multi-Blind are not included.
        </p>
      </div>
      <BestPodiumsTable />
    </>
  );
}
