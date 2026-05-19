'use client';

import { useEffect, useMemo, useState } from 'react';
import type { AllRanksData } from '@/lib/stats';

type Mode = 'single' | 'average' | 'both';
type Limit = 100 | 1000;

const SHORT: Record<string, string> = {
  '222': '2x2', '333': '3x3', '444': '4x4', '555': '5x5',
  '666': '6x6', '777': '7x7', '333oh': 'OH', '333bf': '3BF',
  '333fm': 'FM', '333mbf': 'MBLD', '444bf': '4BF', '555bf': '5BF',
  clock: 'Clk', minx: 'Mega', pyram: 'Pyra', skewb: 'Skwb', sq1: 'SQ-1',
};

type Row = { idx: number; total: number; rank: number; contrib: number[] };

export default function SumOfRanksTable() {
  const [data, setData] = useState<AllRanksData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>('average');
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
    const { total_s, total_a, persons } = data;
    const n = data.events.length;

    const scored = persons.map((p, idx) => {
      let total = 0;
      const contrib = new Array<number>(n).fill(0);
      for (let i = 0; i < n; i++) {
        if (!selected[i]) continue;
        const hasAvg = total_a[i] > 0;
        if (mode === 'single' || mode === 'both') {
          const r = p.sr[i] > 0 ? p.sr[i] : total_s[i] + 1;
          total += r;
          contrib[i] += mode === 'single' ? (p.sr[i] > 0 ? p.sr[i] : 0) : (p.sr[i] > 0 ? p.sr[i] : 0);
        }
        if ((mode === 'average' || mode === 'both') && hasAvg) {
          const r = p.ar[i] > 0 ? p.ar[i] : total_a[i] + 1;
          total += r;
          if (mode === 'average') contrib[i] = p.ar[i] > 0 ? p.ar[i] : 0;
        }
      }
      return { idx, total, contrib, rank: 0 };
    });

    scored.sort((a, b) => a.total - b.total || a.idx - b.idx);

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
  }, [data, selected, mode]);

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
          <button className={mode === 'single' ? 'active' : ''} onClick={() => setMode('single')}>Single</button>
          <button className={mode === 'average' ? 'active' : ''} onClick={() => setMode('average')}>Average</button>
          <button className={mode === 'both' ? 'active' : ''} onClick={() => setMode('both')}>Both</button>
        </div>
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
                <th key={e} title={e} style={{ minWidth: 44, textAlign: 'right' }}>{SHORT[e] ?? e}</th>
              ) : null)}
              <th style={{ textAlign: 'right', minWidth: 80 }}>Total</th>
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
                    let display: string;
                    if (mode === 'single') {
                      display = p.sr[i] > 0 ? p.sr[i].toLocaleString() : '—';
                    } else if (mode === 'average') {
                      display = p.ar[i] > 0 ? p.ar[i].toLocaleString() : '—';
                    } else {
                      const s = p.sr[i] > 0 ? p.sr[i] : null;
                      const a = p.ar[i] > 0 ? p.ar[i] : null;
                      if (s !== null && a !== null) display = `${s.toLocaleString()}/${a.toLocaleString()}`;
                      else if (s !== null) display = s.toLocaleString();
                      else display = '—';
                    }
                    return (
                      <td key={e} className="muted" style={{ textAlign: 'right', fontSize: 11 }}>
                        {display}
                      </td>
                    );
                  })}
                  <td className="value-col" style={{ textAlign: 'right' }}>{r.total.toLocaleString()}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </>
  );
}
