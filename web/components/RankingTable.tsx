'use client';

import { useEffect, useState } from 'react';
import { type StatData, sortEvents, EVENT_NAMES } from '@/lib/stats';
import { formatValue, formatSingle, formatAverage } from '@/lib/format';

type Props = {
  statId: string;
};

export default function RankingTable({ statId }: Props) {
  const [data, setData] = useState<StatData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [eventId, setEventId] = useState<string | null>(null);

  useEffect(() => {
    setData(null);
    setError(null);
    setEventId(null);
    fetch(`/data/${statId}.json`)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((d: StatData) => {
        setData(d);
        const events = sortEvents(Object.keys(d));
        setEventId(events[0] ?? null);
      })
      .catch((e) => setError(String(e)));
  }, [statId]);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const events = sortEvents(Object.keys(data));
  if (events.length === 0) return <div className="empty">No data.</div>;

  const active = eventId ?? events[0];
  const rows = data[active] ?? [];

  return (
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

      <table>
        <thead>
          <tr>
            <th className="rank-col">#</th>
            <th>Person</th>
            <th>Country</th>
            <th>Value</th>
            <th>Single</th>
            <th>Average</th>
            <th>Competition</th>
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
              <td className="value-col">{formatValue(r.value_cs, active)}</td>
              <td>{formatSingle(r.single_cs, active)}</td>
              <td>{formatAverage(r.average_cs, active)}</td>
              <td className="muted">
                <a
                  href={`https://www.worldcubeassociation.org/competitions/${r.competition_id}`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {r.competition_id}
                </a>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}
