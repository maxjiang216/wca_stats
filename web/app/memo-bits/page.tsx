import MemoBitsTable from '@/components/MemoBitsTable';

export const metadata = { title: 'Memorization Load — WCA Stats' };

export default function MemoBitsPage() {
  return (
    <>
      <div className="page-header">
        <h1>Memorization Load</h1>
        <p className="desc">
          Modeling blindfold memorization as an information-theoretic load: the log2 of the number
          of distinct visual states of the puzzle a solver must be able to tell apart to reconstruct
          it. Only solved (non-DNF) solves count; DNS/DNF attempts contribute nothing. For 3x3, 4x4,
          and 5x5 this is a fixed number of bits per solve; for MBLD, each successfully solved cube
          within an attempt counts as one 3x3-equivalent solve.
        </p>
      </div>
      <MemoBitsTable />
    </>
  );
}
