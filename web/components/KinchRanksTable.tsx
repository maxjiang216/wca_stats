'use client';

import { useEffect, useMemo, useState } from 'react';
import type { AllRanksData } from '@/lib/stats';

type Limit = 100 | 1000;

const SHORT: Record<string, string> = {
  '222': '2x2', '333': '3x3', '444': '4x4', '555': '5x5',
  '666': '6x6', '777': '7x7', '333oh': 'OH', '333bf': '3BF',
  '333fm': 'FM', '333mbf': 'MBLD', '444bf': '4BF', '555bf': '5BF',
  clock: 'Clk', minx: 'Mega', pyram: 'Pyra', skewb: 'Skwb', sq1: 'SQ-1',
};

// Events that use single rank only for Kinch.
const SINGLE_ONLY = new Set(['444bf', '555bf']);
// Events where Kinch = max(single, average).
const BEST_OF = new Set(['333bf', '333fm']);

function decodeMbld(value: number): { points: number; time_s: number } | null {
  if (value <= 0) return null;
  const points = 99 - Math.floor(value / 10_000_000);
  const time_s = Math.floor(value / 100) % 100000;
  if (points <= 0) return null;
  return { points, time_s };
}

function mbldAdj(value: number): number {
  const d = decodeMbld(value);
  if (!d) return 0;
  return d.points + (3600 - d.time_s) / 3600;
}

type Row = { idx: number; total: number; rank: number; scores: number[] };

export default function KinchRanksTable() {
  const [data, setData] = useState<AllRanksData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [limit, setLimit] = useState<Limit>(100);
  const [selected, setSelected] = useState<boolean[]>([]);

  useEffect(() => {
    fetch('/data/all_ranks.json')
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then((d: AllRanksData) => {
        setData(d);
        setSelected(new Array(d.events.length).fill(true));
      })
      .catch(e => setError(String(e)));
  }, []);

  const ranked = useMemo((): Row[] => {
    if (!data || selected.length === 0) return [];
    const { events, wr_s, wr_a, persons } = data;
    const n = events.length;

    const scored = persons.map((p, idx) => {
      let sum = 0;
      let count = 0;
      const scores = new Array<number>(n).fill(0);

      for (let i = 0; i < n; i++) {
        if (!selected[i]) continue;
        count++;
        const ev = events[i];
        let kinch = 0;

        if (ev === '333mbf') {
          const wrAdj = mbldAdj(wr_s[i]);
          const pbAdj = mbldAdj(p.ps[i]);
          if (wrAdj > 0 && pbAdj > 0) kinch = (pbAdj / wrAdj) * 100;
        } else if (SINGLE_ONLY.has(ev)) {
          if (wr_s[i] > 0 && p.ps[i] > 0) kinch = (wr_s[i] / p.ps[i]) * 100;
        } else if (BEST_OF.has(ev)) {
          let ks = 0, ka = 0;
          if (wr_s[i] > 0 && p.ps[i] > 0) ks = (wr_s[i] / p.ps[i]) * 100;
          if (wr_a[i] > 0 && p.pa[i] > 0) ka = (wr_a[i] / p.pa[i]) * 100;
          kinch = Math.max(ks, ka);
        } else {
          if (wr_a[i] > 0 && p.pa[i] > 0) kinch = (wr_a[i] / p.pa[i]) * 100;
        }

        scores[i] = kinch;
        sum += kinch;
      }

      const total = count > 0 ? sum / count : 0;
      return { idx, total, scores, rank: 0 };
    });

    scored.sort((a, b) => b.total - a.total || a.idx - b.idx);

    let rank = 1;
    for (let i = 0; i < scored.length; i++) {
      if (i > 0 && scored[i].total === scored[i - 1].total) {
        scored[i].rank = scored[i - 1].rank;
      } else {
        scored[i].rank = rank;
      }
      rank++;
    }
    return scored;
  }, [data, selected]);

  const rows = ranked.filter(r => r.rank <= limit);

  function toggleEvent(i: number) {
    setSelected(s => { const n = [...s]; n[i] = !n[i]; return n; });
  }
  function selectAll() { setSelected(s => s.map(() => true)); }
  function selectNone() { setSelected(s => s.map(() => false)); }

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data || selected.length === 0) return <div className="loading">Loading…</div>;

  const { events, persons } = data;

  return (
    <>
      <div className="toolbar">
        <div className="toggle-group">
          <button className={limit === 100 ? 'active' : ''} onClick={() => setLimit(100)}>Top 100</button>
          <button className={limit === 1000 ? 'active' : ''} onClick={() => setLimit(1000)}>Top 1000</button>
        </div>
        <span className="muted">{rows.length.toLocaleString()} shown</span>
      </div>

      <div className="event-checkboxes">
        {events.map((e, i) => (
          <label key={e} className={selected[i] ? 'checked' : ''}>
            <input type="checkbox" checked={selected[i]} onChange={() => toggleEvent(i)} />
            {SHORT[e] ?? e}
          </label>
        ))}
        <button
          style={{ marginLeft: 8, background: 'none', border: '1px solid #333', color: '#aaa', borderRadius: 4, padding: '3px 8px', cursor: 'pointer', fontSize: 12 }}
          onClick={selectAll}
        >All</button>
        <button
          style={{ background: 'none', border: '1px solid #333', color: '#aaa', borderRadius: 4, padding: '3px 8px', cursor: 'pointer', fontSize: 12 }}
          onClick={selectNone}
        >None</button>
      </div>

      <div style={{ overflowX: 'auto' }}>
        <table>
          <thead>
            <tr>
              <th className="rank-col">#</th>
              <th>Person</th>
              <th>Country</th>
              {events.map((e, i) => selected[i] ? (
                <th key={e} title={e} style={{ minWidth: 48, textAlign: 'right' }}>{SHORT[e] ?? e}</th>
              ) : null)}
              <th style={{ textAlign: 'right', minWidth: 72 }}>Kinch</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(r => {
              const p = persons[r.idx];
              return (
                <tr key={p.id}>
                  <td className="rank-col">{r.rank}</td>
                  <td>
                    <a href={`https://www.worldcubeassociation.org/persons/${p.id}`} target="_blank" rel="noreferrer">
                      {p.n}
                    </a>
                  </td>
                  <td className="muted">{p.c}</td>
                  {events.map((e, i) => {
                    if (!selected[i]) return null;
                    const k = r.scores[i];
                    return (
                      <td key={e} className="muted" style={{ textAlign: 'right', fontSize: 11 }}>
                        {k > 0 ? k.toFixed(2) : '—'}
                      </td>
                    );
                  })}
                  <td className="value-col" style={{ textAlign: 'right' }}>{r.total.toFixed(2)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </>
  );
}
