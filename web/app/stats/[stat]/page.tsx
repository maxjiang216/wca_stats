import { notFound } from 'next/navigation';
import RankingTable from '@/components/RankingTable';
import MbldRankingTable from '@/components/MbldRankingTable';
import RelayRankingTable from '@/components/RelayRankingTable';
import { STATS, getStat } from '@/lib/stats';

export function generateStaticParams() {
  return STATS.map((s) => ({ stat: s.id }));
}

export default async function StatPage({
  params,
}: {
  params: Promise<{ stat: string }>;
}) {
  const { stat: statId } = await params;
  const stat = getStat(statId);
  if (!stat) notFound();

  return (
    <>
      <div className="page-header">
        <h1>{stat.title}</h1>
        <p className="desc">{stat.description}</p>
      </div>
      {stat.group === 'mbld' ? (
        <MbldRankingTable statId={statId} />
      ) : stat.group === 'relay' ? (
        <RelayRankingTable statId={statId} />
      ) : (
        <RankingTable statId={statId} />
      )}
    </>
  );
}
