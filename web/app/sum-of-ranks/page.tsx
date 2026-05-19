import SumOfRanksTable from '@/components/SumOfRanksTable';

export default function SumOfRanksPage() {
  return (
    <>
      <div className="page-header">
        <h1>Sum of Ranks</h1>
        <p className="desc">
          Sum of world ranks across selected events. Lower is better. People who have not competed in
          an event receive a penalty rank of (total competitors + 1). Select events and mode below.
        </p>
      </div>
      <SumOfRanksTable />
    </>
  );
}
