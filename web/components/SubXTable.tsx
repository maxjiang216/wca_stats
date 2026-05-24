'use client';

import { useEffect, useState, useMemo } from 'react';
import { formatSingle, formatAverage } from '@/lib/format';

type Entry = { id: string; name: string; country: string; count: number; pb: number };
type Ranking = { label: string; val_cs: number; entries: Entry[] };
type EventData = { single: Ranking[]; avg: Ranking[] };
type SubXData = Record<string, EventData>;

const EVENT_LABELS: Record<string, string> = {
  '333': '3x3', '222': '2x2', '444': '4x4', '555': '5x5',
  '666': '6x6', '777': '7x7', 'pyram': 'Pyra', 'skewb': 'Skwb',
};

const EVENT_ORDER = ['333', '222', '444', '555', '666', '777', 'pyram', 'skewb'];
const SINGLE_ONLY = new Set(['pyram', 'skewb']);

function PersonLink({ id, name }: { id: string; name: string }) {
  return (
    <a
      href={`https://www.worldcubeassociation.org/persons/${id}`}
      target="_blank"
      rel="noreferrer"
    >
      {name}
    </a>
  );
}

function RankingTable({
  ranking,
  eventId,
  type,
  limit,
}: {
  ranking: Ranking;
  eventId: string;
  type: 'single' | 'avg';
  limit: number;
}) {
  const entries = ranking.entries.slice(0, limit);
  const fmt = type === 'single'
    ? (v: number) => formatSingle(v, eventId)
    : (v: number) => formatAverage(v, eventId);

  return (
    <div style={{ overflowX: 'auto' }}>
      <table>
        <thead>
          <tr>
            <th className="rank-col">#</th>
            <th>Person</th>
            <th>Country</th>
            <th style={{ textAlign: 'right' }}>Count</th>
            <th style={{ textAlign: 'right' }}>PB</th>
          </tr>
        </thead>
        <tbody>
          {entries.map((e, i) => (
            <tr key={e.id}>
              <td className="rank-col">{i + 1}</td>
              <td><PersonLink id={e.id} name={e.name} /></td>
              <td>{e.country}</td>
              <td className="value-col" style={{ textAlign: 'right' }}>{e.count}</td>
              <td style={{ textAlign: 'right' }}>{fmt(e.pb)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default function SubXTable() {
  const [data, setData] = useState<SubXData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [event, setEvent] = useState('333');
  const [type, setType] = useState<'single' | 'avg'>('single');
  const [threshIdx, setThreshIdx] = useState(0);
  const [limit, setLimit] = useState<100 | 1000>(100);

  useEffect(() => {
    fetch('/data/sub_x.json')
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch(e => setError(String(e)));
  }, []);

  // Reset threshold index when event or type changes
  const prevKey = useMemo(() => `${event}:${type}`, [event, type]);

  const eventData = data?.[event];
  const rankings = type === 'single' ? eventData?.single : eventData?.avg;
  const hasAvg = !SINGLE_ONLY.has(event);

  // Clamp threshold index if out of range
  const safeIdx = rankings ? Math.min(threshIdx, rankings.length - 1) : 0;
  const ranking = rankings?.[safeIdx];

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const events = EVENT_ORDER.filter(e => data[e]);

  return (
    <>
      <div className="toolbar" style={{ flexWrap: 'wrap', gap: 8 }}>
        <div className="toggle-group">
          {events.map(ev => (
            <button
              key={ev}
              className={event === ev ? 'active' : ''}
              onClick={() => { setEvent(ev); setThreshIdx(0); }}
            >
              {EVENT_LABELS[ev] ?? ev}
            </button>
          ))}
        </div>

        {hasAvg && (
          <div className="toggle-group">
            <button className={type === 'single' ? 'active' : ''} onClick={() => { setType('single'); setThreshIdx(0); }}>
              Single
            </button>
            <button className={type === 'avg' ? 'active' : ''} onClick={() => { setType('avg'); setThreshIdx(0); }}>
              Average
            </button>
          </div>
        )}

        {rankings && rankings.length > 1 && (
          <div className="toggle-group">
            {rankings.map((r, i) => (
              <button key={r.label} className={safeIdx === i ? 'active' : ''} onClick={() => setThreshIdx(i)}>
                {r.label}
              </button>
            ))}
          </div>
        )}

        <div className="toggle-group">
          <button className={limit === 100 ? 'active' : ''} onClick={() => setLimit(100)}>Top 100</button>
          <button className={limit === 1000 ? 'active' : ''} onClick={() => setLimit(1000)}>Top 1000</button>
        </div>
      </div>

      {ranking ? (
        <RankingTable ranking={ranking} eventId={event} type={hasAvg ? type : 'single'} limit={limit} />
      ) : (
        <div className="empty">No data for this selection.</div>
      )}
    </>
  );
}
