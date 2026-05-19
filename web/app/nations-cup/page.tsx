import NationsCupTable from '@/components/NationsCupTable';

export default function NationsCupPage() {
  return (
    <>
      <div className="page-header">
        <h1>Nations Cup Dream Team</h1>
        <p className="desc">
          Each country is ranked by the sum of their top 3 competitors&apos; personal best averages in
          each event. Countries need at least 3 ranked competitors to qualify. For MBLD (no official
          average), personal best singles are used.
        </p>
      </div>
      <NationsCupTable />
    </>
  );
}
