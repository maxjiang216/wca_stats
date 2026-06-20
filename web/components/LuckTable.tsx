'use client';

import { useEffect, useState } from 'react';
import { formatSingle } from '@/lib/format';
import { EVENT_NAMES, sortEvents } from '@/lib/stats';

type LuckEntry = {
  ao5_rank: number;
  skill_rank: number;
  pid: string;
  name: string;
  cid: string;
  comp_id: string;
  date: string;
  ao5: number; // centiseconds (record average)
  skill: number; // centiseconds (leave-one-out skill at that comp)
  sigmas: number;
  luck_prob: number;
};

type LuckData = Record<string, LuckEntry[]>;
type SortKey = 'ao5' | 'skill' | 'luck';

const luckPct = (p: number) => {
  if (!(p >= 0)) return '—';
  const v = p * 100;
  return v < 1 ? `${v.toFixed(2)}%` : v < 10 ? `${v.toFixed(1)}%` : `${Math.round(v)}%`;
};

export default function LuckTable() {
  const [data, setData] = useState<LuckData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [event, setEvent] = useState<string>('333');
  const [sort, setSort] = useState<SortKey>('ao5');

  useEffect(() => {
    fetch('/data/luck.json')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then((d: LuckData) => {
        setData(d);
        if (!d['333'] && Object.keys(d).length > 0) setEvent(sortEvents(Object.keys(d))[0]);
      })
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const events = sortEvents(Object.keys(data));
  const entries = data[event] ?? [];
  const rows = [...entries].sort((a, b) => {
    if (sort === 'ao5') return a.ao5_rank - b.ao5_rank;
    if (sort === 'skill') return a.skill_rank - b.skill_rank;
    return a.luck_prob - b.luck_prob; // luckiest first
  });

  // Misplacement: ao5 rank far better than skill rank ⇒ propped up by one lucky average.
  const gapColor = (gap: number) =>
    gap <= -20 ? '#d9534f' : gap <= -8 ? '#e0a800' : gap >= 8 ? '#2a8' : undefined;

  return (
    <div>
      <div className="tabs">
        {events.map((e) => (
          <button key={e} onClick={() => setEvent(e)} className={e === event ? 'active' : ''}>
            {EVENT_NAMES[e] ?? e}
          </button>
        ))}
      </div>

      <div className="table-meta">
        Sort:&nbsp;
        {(['ao5', 'skill', 'luck'] as SortKey[]).map((k) => (
          <button
            key={k}
            onClick={() => setSort(k)}
            className={sort === k ? 'active' : ''}
            style={{ marginRight: 6 }}
          >
            {k === 'ao5' ? 'ao5 rank' : k === 'skill' ? 'skill rank' : 'luckiest'}
          </button>
        ))}
      </div>

      <table>
        <thead>
          <tr>
            <th>ao5 #</th>
            <th>Skill #</th>
            <th>Δ</th>
            <th>Name</th>
            <th>Country</th>
            <th>Competition</th>
            <th className="value-col">Date</th>
            <th className="value-col">Record ao5</th>
            <th className="value-col">Skill (LOO)</th>
            <th className="value-col">σ</th>
            <th className="value-col">Career %</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((e) => {
            const gap = e.skill_rank - e.ao5_rank; // positive ⇒ lucky/overrated
            return (
              <tr key={e.pid}>
                <td>{e.ao5_rank}</td>
                <td>{e.skill_rank}</td>
                <td style={{ color: gapColor(-gap), fontWeight: Math.abs(gap) >= 8 ? 600 : undefined }}>
                  {gap > 0 ? `+${gap}` : gap}
                </td>
                <td>
                  <a
                    href={`https://www.worldcubeassociation.org/persons/${e.pid}`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {e.name}
                  </a>
                </td>
                <td>{e.cid}</td>
                <td>
                  <a
                    href={`https://www.worldcubeassociation.org/competitions/${e.comp_id}`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {e.comp_id}
                  </a>
                </td>
                <td className="value-col">{e.date}</td>
                <td className="value-col">{formatSingle(Math.round(e.ao5), event)}</td>
                <td className="value-col">{formatSingle(Math.round(e.skill), event)}</td>
                <td className="value-col">{e.sigmas.toFixed(2)}</td>
                <td className="value-col">{luckPct(e.luck_prob)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
