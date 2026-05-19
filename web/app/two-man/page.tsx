import TwoManTable from '@/components/TwoManTable';

export default function TwoManPage() {
  return (
    <>
      <div className="page-header">
        <h1>2-Man Guildford</h1>
        <p className="desc">
          Two competitors split the events and solve them simultaneously. The team time is the
          bottleneck — max of each person&apos;s total. The optimal event split is found by exhaustive
          search over all possible assignments (1,024 for Mini, 4,096 for Guildford).
          Only competitors with a valid average for every event in the challenge qualify.
        </p>
      </div>
      <TwoManTable />
    </>
  );
}
