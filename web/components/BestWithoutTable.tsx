'use client';

import { useEffect, useState } from 'react';
import { formatAverage, formatMbldSingle, formatSingle } from '@/lib/format';
import { EVENT_NAMES, sortEvents } from '@/lib/stats';

type Entry = {
  rank: number;
  person_id: string;
  person_name: string;
  country_id: string;
  value: number;
  world_rank: number;
};

type Data = Record<string, { single: Entry[]; average: Entry[] }>;
type Kind = 'single' | 'average';

// Old-style multi-blind uses a different packed encoding we don't decode.
const HIDDEN_EVENTS = new Set(['333mbo']);

export default function BestWithoutTable({ file }: { file: string }) {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [event, setEvent] = useState('333');
  const [kind, setKind] = useState<Kind>('single');

  useEffect(() => {
    fetch(`/data/${file}`)
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, [file]);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const events = sortEvents(Object.keys(data).filter((e) => !HIDDEN_EVENTS.has(e)));
  const hasAvg = (data[event]?.average.length ?? 0) > 0;
  const activeKind: Kind = hasAvg ? kind : 'single';
  const rows = data[event]?.[activeKind] ?? [];
  const fmt = (v: number) =>
    event === '333mbf'
      ? formatMbldSingle(v)
      : activeKind === 'single'
        ? formatSingle(v, event)
        : formatAverage(v, event);

  return (
    <>
      <div className="toolbar">
        <div className="toggle-group" style={{ flexWrap: 'wrap' }}>
          {events.map((ev) => (
            <button
              key={ev}
              className={event === ev ? 'active' : ''}
              onClick={() => setEvent(ev)}
              title={EVENT_NAMES[ev]}
            >
              {ev}
            </button>
          ))}
        </div>
      </div>
      <div className="toolbar">
        <div className="toggle-group">
          <button className={activeKind === 'single' ? 'active' : ''} onClick={() => setKind('single')}>
            Single
          </button>
          {hasAvg && (
            <button className={activeKind === 'average' ? 'active' : ''} onClick={() => setKind('average')}>
              Average
            </button>
          )}
        </div>
      </div>

      {rows.length === 0 ? (
        <div className="empty">No data for this selection.</div>
      ) : (
        <div style={{ overflowX: 'auto' }}>
          <table>
            <thead>
              <tr>
                <th className="rank-col">#</th>
                <th>Person</th>
                <th>Country</th>
                <th className="value-col" style={{ textAlign: 'right' }}>PB</th>
                <th style={{ textAlign: 'right' }}>World rank</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((e) => (
                <tr key={e.person_id}>
                  <td className="rank-col">{e.rank}</td>
                  <td>
                    <a
                      href={`https://www.worldcubeassociation.org/persons/${e.person_id}?event=${event}`}
                      target="_blank"
                      rel="noreferrer"
                    >
                      {e.person_name}
                    </a>
                  </td>
                  <td className="muted">{e.country_id}</td>
                  <td className="value-col" style={{ textAlign: 'right' }}>{fmt(e.value)}</td>
                  <td className="muted" style={{ textAlign: 'right' }}>{e.world_rank}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
