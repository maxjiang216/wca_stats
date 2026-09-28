import TwoManTable from '@/components/TwoManTable';

export default function TwoManPage() {
  return (
    <>
      <div className="page-header">
        <h1>2-Man Guildford</h1>
        <p className="desc">
          Two competitors split the events and solve them simultaneously. The team time is the
          bottleneck — max of each person&apos;s total. Shown globally, then the best pair for each
          continent, then for each country (countries only appear if some pair of their competitors
          can cover every event between them). A competitor doesn&apos;t need a valid average in
          every event themselves — only in whichever events they end up assigned.
        </p>
      </div>
      <TwoManTable />
    </>
  );
}
