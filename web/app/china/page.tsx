'use client';

import { useEffect, useState } from 'react';
import ChinaChart, { type ChartPoint } from '@/components/ChinaChart';
import { sortEvents, EVENT_NAMES } from '@/lib/stats';

type EventData = { total: number; points: ChartPoint[] };
type AllData = Record<string, EventData>;

export default function ChinaPage() {
  const [data, setData] = useState<AllData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [eventId, setEventId] = useState<string | null>(null);

  useEffect(() => {
    fetch('/data/china_all.json')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then((d: AllData) => {
        setData(d);
        setEventId(sortEvents(Object.keys(d))[0] ?? null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  const events = data ? sortEvents(Object.keys(data)) : [];
  const active = eventId ?? events[0];

  return (
    <>
      <div className="page-header">
        <h1>China vs USA — by Event</h1>
        <p className="desc">
          Of the top&nbsp;N competitors by world average ranking for each event, what percentage
          are Chinese or American? Hover to read values at any rank.
        </p>
      </div>

      {error && <div className="empty">Failed to load: {error}</div>}
      {!data && !error && <div className="loading">Loading…</div>}

      {data && (
        <>
          <div className="tabs">
            {events.map((e) => (
              <button
                key={e}
                className={e === active ? 'active' : ''}
                onClick={() => setEventId(e)}
              >
                {EVENT_NAMES[e] ?? e}
              </button>
            ))}
          </div>

          {active && data[active] && (
            <ChinaChart total={data[active].total} points={data[active].points} />
          )}
        </>
      )}
    </>
  );
}
