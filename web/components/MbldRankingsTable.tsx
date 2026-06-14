'use client';

import { useEffect, useState } from 'react';
import { formatMbldTime } from '@/lib/format';

interface Entry {
  rank: number;
  person_id: string;
  person_name: string;
  country_id: string;
  competition_id: string;
  solved: number;
  attempted: number;
  time_s: number;
}

interface Data {
  by_points: Record<string, Record<string, Entry[]>>;
  by_perfect: Record<string, Record<string, Entry[]>>;
  points_list: Record<string, number[]>;
  perfect_list: Record<string, number[]>;
}

type Tab = 'points' | 'perfect';
type Limit = 100 | 1000;

const EVENTS = ['333mbf', '333mbo'];
const EVENT_LABELS: Record<string, string> = {
  '333mbf': '3x3 Multi-Blind',
  '333mbo': 'Multi-Blind (Old)',
};

export default function MbldRankingsTable() {
  const [data, setData] = useState<Data | null>(null);
  const [tab, setTab] = useState<Tab>('points');
  const [event, setEvent] = useState('333mbf');
  const [n, setN] = useState<number | null>(null);
  const [limit, setLimit] = useState<Limit>(100);

  useEffect(() => {
    fetch('/data/mbld_rankings.json')
      .then((r) => r.json())
      .then((d: Data) => {
        setData(d);
        const list = d.points_list['333mbf'] ?? d.points_list['333mbo'] ?? [];
        if (list.length > 0) setN(list[0]);
      });
  }, []);

  if (!data) return <div className="loading">Loading…</div>;

  const availableEvents = EVENTS.filter(
    (e) => data.points_list[e] || data.perfect_list[e],
  );

  const nList = tab === 'points'
    ? (data.points_list[event] ?? [])
    : (data.perfect_list[event] ?? []);

  const activeN = n !== null && nList.includes(n) ? n : (nList[0] ?? null);

  const table = tab === 'points' ? data.by_points : data.by_perfect;
  const rows: Entry[] = activeN !== null
    ? (table[event]?.[String(activeN)] ?? []).filter((r) => r.rank <= limit)
    : [];
  const totalRows: Entry[] = activeN !== null
    ? (table[event]?.[String(activeN)] ?? [])
    : [];

  const switchTab = (t: Tab) => {
    setTab(t);
    const newList = t === 'points'
      ? (data.points_list[event] ?? [])
      : (data.perfect_list[event] ?? []);
    if (newList.length > 0 && (n === null || !newList.includes(n))) {
      setN(newList[0]);
    }
  };

  const switchEvent = (e: string) => {
    setEvent(e);
    const newList = tab === 'points'
      ? (data.points_list[e] ?? [])
      : (data.perfect_list[e] ?? []);
    if (newList.length > 0 && (n === null || !newList.includes(n))) {
      setN(newList[0]);
    }
  };

  return (
    <div>
      <div className="tabs">
        <button className={tab === 'points' ? 'active' : ''} onClick={() => switchTab('points')}>
          By Points (N pts)
        </button>
        <button className={tab === 'perfect' ? 'active' : ''} onClick={() => switchTab('perfect')}>
          By N/N (clean only)
        </button>
      </div>

      {availableEvents.length > 1 && (
        <div className="tabs">
          {availableEvents.map((e) => (
            <button
              key={e}
              className={e === event ? 'active' : ''}
              onClick={() => switchEvent(e)}
            >
              {EVENT_LABELS[e] ?? e}
            </button>
          ))}
        </div>
      )}

      <div className="toolbar">
        <label>
          {tab === 'points' ? 'Points' : 'N'}
          <select
            value={activeN ?? ''}
            onChange={(ev) => setN(Number(ev.target.value))}
          >
            {nList.map((v) => (
              <option key={v} value={v}>
                {tab === 'points' ? `${v} pts` : `${v}/${v}`}
              </option>
            ))}
          </select>
        </label>

        <div className="toggle-group">
          <button className={limit === 100 ? 'active' : ''} onClick={() => setLimit(100)}>
            Top 100
          </button>
          <button className={limit === 1000 ? 'active' : ''} onClick={() => setLimit(1000)}>
            Top 1000
          </button>
        </div>

        <span className="muted">
          {rows.length.toLocaleString()} of {totalRows.length.toLocaleString()} shown
        </span>
      </div>

      <table>
        <thead>
          <tr>
            <th className="rank-col">#</th>
            <th>Person</th>
            <th>Country</th>
            {tab === 'points' && <th>Solved/Attempted</th>}
            <th>Time</th>
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
              {tab === 'points' && (
                <td className="value-col">
                  {r.solved}/{r.attempted}
                </td>
              )}
              <td className="value-col">{formatMbldTime(r.time_s)}</td>
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
    </div>
  );
}
