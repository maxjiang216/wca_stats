import DominanceChart from '@/components/DominanceChart';

export const metadata = { title: 'Top-Result Dominance — WCA Stats' };

export default function DominancePage() {
  return (
    <>
      <div className="page-header">
        <h1>Top-Result Dominance</h1>
        <p className="desc">
          Order every result (not just personal bests) from fastest to slowest. Three questions,
          tracked weekly: how many of the leading results belong to a single person before the
          2nd-best person&apos;s best appears (<strong>#1 person</strong>); how many belong to the top
          two people before the 3rd-best person&apos;s best (<strong>Top 2 people</strong>); and how
          many come from one country before any other country&apos;s best (<strong>One country</strong>).
          A tall line means the field below the leader was a wasteland — nobody else was close.
        </p>
      </div>
      <DominanceChart />
    </>
  );
}
