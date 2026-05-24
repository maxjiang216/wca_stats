import SubXTable from '@/components/SubXTable';

export default function SubXPage() {
  return (
    <>
      <div className="page-header">
        <h1>Sub-X Rankings</h1>
        <p className="desc">
          Number of official sub-X individual solves (singles) or sub-X averages per competitor.
          Only results from official WCA competitions are counted.
        </p>
      </div>
      <SubXTable />
    </>
  );
}
