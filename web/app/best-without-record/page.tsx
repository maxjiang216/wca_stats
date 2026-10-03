import BestWithoutTable from '@/components/BestWithoutTable';

export const metadata = { title: 'Best Without a Record — WCA Stats' };

export default function BestWithoutRecordPage() {
  return (
    <>
      <div className="page-header">
        <h1>Best Without a Record</h1>
        <p className="desc">
          Top 100 personal bests per event among competitors who have never set a regional
          record (NR, CR or WR) in that event — single or average. World rank is the
          competitor&apos;s rank among everyone, record holders included.
        </p>
      </div>
      <BestWithoutTable file="best_without_record.json" />
    </>
  );
}
