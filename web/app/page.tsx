import Link from 'next/link';
import { STATS } from '@/lib/stats';

export default function Home() {
  return (
    <>
      <div className="page-header">
        <h1>WCA Statistics</h1>
        <p className="desc">
          Derived rankings from the WCA results export. Pick a stat from the sidebar
          or below.
        </p>
      </div>

      <div className="home-grid">
        {STATS.map((s) => (
          <Link key={s.id} href={`/stats/${s.id}`} className="home-card">
            <h3>{s.title}</h3>
            <p>{s.description}</p>
          </Link>
        ))}
      </div>
    </>
  );
}
