import FirstRecordsTable from '@/components/FirstRecordsTable';

export const metadata = { title: 'First CR/WR by Country — WCA Stats' };

export default function FirstRecordsPage() {
  return (
    <>
      <div className="page-header">
        <h1>First CR/WR by Country</h1>
        <p className="desc">
          For each country and event, the first competitor to set a continental record (CR)
          or world record (WR) in that event. WR badges indicate the result was a world record
          at the time.
        </p>
      </div>
      <FirstRecordsTable />
    </>
  );
}
