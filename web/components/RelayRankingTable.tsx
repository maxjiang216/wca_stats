'use client';

import { useEffect, useState } from 'react';
import { type RelayEntry, getStat } from '@/lib/stats';
import { formatAverage } from '@/lib/format';

const SHORT: Record<string, string> = {
  '222': '2x2',
  '333': '3x3',
  '444': '4x4',
  '555': '5x5',
  '666': '6x6',
  '777': '7x7',
  '333oh': '3OH',
  clock: 'Clock',
  minx: 'Mega',
  pyram: 'Pyra',
  skewb: 'Skewb',
  sq1: 'SQ-1',
};

type Props = { statId: string };

export default function RelayRankingTable({ statId }: Props) {
  const relayEvents = getStat(statId)?.relayEvents ?? [];

  const [data, setData] = useState<RelayEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setData(null);
    setError(null);
    fetch(`/data/${statId}.json`)
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, [statId]);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data)  return <div className="loading">Loading…</div>;

  const rows = data;

  return (
    <>
      <div style={{ overflowX: 'auto' }}>
        <table>
          <thead>
            <tr>
              <th className="rank-col">#</th>
              <th>Person</th>
              <th>Country</th>
              {relayEvents.map((e) => (
                <th key={e} title={e}>{SHORT[e] ?? e}</th>
              ))}
              <th className="value-col">Total</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={`${r.person_id}-${i}`}>
                <td className="rank-col">{r.rank}</td>
                <td>
                  <a
                    href={`https://www.worldcubeassociation.org/persons/${r.person_id}`}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {r.person_name}
                  </a>
                </td>
                <td className="muted">{r.country_id}</td>
                {r.event_avgs.map((v, j) => (
                  <td key={j}>{formatAverage(v, relayEvents[j] ?? '333')}</td>
                ))}
                <td className="value-col">{formatAverage(r.total_cs, '333')}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
