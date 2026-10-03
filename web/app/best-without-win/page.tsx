import BestWithoutTable from '@/components/BestWithoutTable';

export const metadata = { title: 'Best Without a Win — WCA Stats' };

export default function BestWithoutWinPage() {
  return (
    <>
      <div className="page-header">
        <h1>Best Without a Win</h1>
        <p className="desc">
          Top 100 personal bests per event among competitors who have never won that event: no
          1st place in a final round (ranked by the competition&apos;s format) with a valid
          result. World rank is the competitor&apos;s rank among everyone, winners included.
        </p>
      </div>
      <BestWithoutTable file="best_without_win.json" />
    </>
  );
}
