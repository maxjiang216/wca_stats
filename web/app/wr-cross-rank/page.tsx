import WrCrossRank from '@/components/WrCrossRank';

export const metadata = { title: 'WR Cross-Rank — WCA Stats' };

export default function WrCrossRankPage() {
  return (
    <>
      <div className="page-header">
        <h1>WR Cross-Rank</h1>
        <p className="desc">
          How would a world record place if it were judged against a different ranking? Each world
          record average is ranked among the singles of the same event; each NxN record is ranked
          among the next-smaller cube; plus 3×3 OH vs 3×3 and 5BLD vs 4BLD. The graph tracks the
          rank of the then-current record over time; the table shows where each historical record
          stood the week it was set.
        </p>
      </div>
      <WrCrossRank />
    </>
  );
}
