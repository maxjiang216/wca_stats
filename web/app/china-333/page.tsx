'use client';

import { useEffect, useState } from 'react';
import ChinaChart, { type ChartPoint } from '@/components/ChinaChart';

type RawData = { total: number; points: ChartPoint[] };

export default function China333Page() {
  const [data, setData] = useState<RawData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('/data/china_333.json')
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch(e => setError(String(e)));
  }, []);

  return (
    <>
      <div className="page-header">
        <h1>China vs USA — 3x3 Average Rankings</h1>
        <p className="desc">
          Of the top&nbsp;N competitors by WCA 3×3 average-of-5 world ranking, what percentage are
          Chinese or American? Hover the chart to see values at any rank.
        </p>
      </div>

      {error && <div className="empty">Failed to load: {error}</div>}
      {!data && !error && <div className="loading">Loading…</div>}
      {data && <ChinaChart total={data.total} points={data.points} />}
    </>
  );
}
