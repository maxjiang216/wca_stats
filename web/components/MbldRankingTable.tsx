'use client';

import { useEffect, useState } from 'react';
import {
  type MbldSingleData,
  type MbldMeanData,
  sortEvents,
  EVENT_NAMES,
} from '@/lib/stats';
import { formatMbldTime, formatMbldMean } from '@/lib/format';

type Limit = 100 | 1000;

type Props = {
  statId: string;
};

export default function MbldRankingTable({ statId }: Props) {
  const isMean = statId === 'mbld_mean';

  const [singleData, setSingleData] = useState<MbldSingleData | null>(null);
  const [meanData, setMeanData] = useState<MbldMeanData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [eventId, setEventId] = useState<string | null>(null);
  const [limit, setLimit] = useState<Limit>(100);

  useEffect(() => {
    setSingleData(null);
    setMeanData(null);
    setError(null);
    setEventId(null);
    fetch(`/data/${statId}.json`)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((d) => {
        if (isMean) {
          setMeanData(d as MbldMeanData);
          setEventId(sortEvents(Object.keys(d))[0] ?? null);
        } else {
          setSingleData(d as MbldSingleData);
          setEventId(sortEvents(Object.keys(d))[0] ?? null);
        }
      })
      .catch((e) => setError(String(e)));
  }, [statId, isMean]);

  if (error) return <div className="empty">Failed to load: {error}</div>;

  const data = isMean ? meanData : singleData;
  if (!data) return <div className="loading">Loading…</div>;

  const events = sortEvents(Object.keys(data));
  if (events.length === 0) return <div className="empty">No data.</div>;

  const active = eventId ?? events[0];

  if (isMean) {
    const rows = ((meanData![active] ?? []) as import('@/lib/stats').MbldMeanEntry[]).filter(
      (r) => r.rank <= limit,
    );
    const all = (meanData![active] ?? []) as import('@/lib/stats').MbldMeanEntry[];
    return (
      <>
        <div className="toolbar">
          <div className="toggle-group">
            <button className={limit === 100 ? 'active' : ''} onClick={() => setLimit(100)}>
              Top 100
            </button>
            <button className={limit === 1000 ? 'active' : ''} onClick={() => setLimit(1000)}>
              Top 1000
            </button>
          </div>
          <span className="muted">
            {rows.length.toLocaleString()} of {all.length.toLocaleString()} shown
          </span>
        </div>

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
              <th>Mean Pts</th>
              <th>Total Time</th>
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
                <td className="value-col">{formatMbldMean(r.points_sum)}</td>
                <td>{formatMbldTime(r.time_total_s)}</td>
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

  // single attempt stats (mbld_perfect, mbld_solved)
  const allSingle = (singleData![active] ?? []) as import('@/lib/stats').MbldSingleEntry[];
  const rowsSingle = allSingle.filter((r) => r.rank <= limit);

  return (
    <>
      <div className="toolbar">
        <div className="toggle-group">
          <button className={limit === 100 ? 'active' : ''} onClick={() => setLimit(100)}>
            Top 100
          </button>
          <button className={limit === 1000 ? 'active' : ''} onClick={() => setLimit(1000)}>
            Top 1000
          </button>
        </div>
        <span className="muted">
          {rowsSingle.length.toLocaleString()} of {allSingle.length.toLocaleString()} shown
        </span>
      </div>

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
            <th>Solved</th>
            <th>Attempted</th>
            <th>Time</th>
            <th>Competition</th>
          </tr>
        </thead>
        <tbody>
          {rowsSingle.map((r, i) => (
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
              <td className="value-col">{r.solved}</td>
              <td>{r.attempted}</td>
              <td>{formatMbldTime(r.time_s)}</td>
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
