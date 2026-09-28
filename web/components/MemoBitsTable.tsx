'use client';

import { useEffect, useState } from 'react';

interface Entry {
  rank: number;
  person_id: string;
  person_name: string;
  country_id: string;
  competition_id: string;
  competition_name: string;
  date: string;
  days: number;
  total_bits: number;
  per_day_bits: number;
  bf3_solves: number;
  bf4_solves: number;
  bf5_solves: number;
  mbf_solved: number;
}

interface Data {
  bits_3bf: number;
  bits_4bf: number;
  bits_5bf: number;
  by_total: Entry[];
  by_day: Entry[];
}

type Tab = 'by_total' | 'by_day';

function fmtBits(n: number): string {
  return n.toLocaleString(undefined, { maximumFractionDigits: 1 });
}

export default function MemoBitsTable() {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>('by_total');
  const [limit, setLimit] = useState<100 | 1000>(100);

  useEffect(() => {
    fetch('/data/memo_bits.json')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const rows = data[tab].slice(0, limit);

  return (
    <>
      <div className="muted" style={{ marginBottom: 12, fontSize: 12 }}>
        Bits per solved blindfold solve: 3x3 = {fmtBits(data.bits_3bf)},
        {' '}4x4 = {fmtBits(data.bits_4bf)}, 5x5 = {fmtBits(data.bits_5bf)}.
        A 333mbf cube counts as one 3x3 solve.
      </div>

      <div className="toolbar">
        <div className="toggle-group">
          <button className={tab === 'by_total' ? 'active' : ''} onClick={() => setTab('by_total')}>
            Most in One Competition
          </button>
          <button className={tab === 'by_day' ? 'active' : ''} onClick={() => setTab('by_day')}>
            Most per Day
          </button>
        </div>
        <div className="toggle-group">
          <button className={limit === 100 ? 'active' : ''} onClick={() => setLimit(100)}>Top 100</button>
          <button className={limit === 1000 ? 'active' : ''} onClick={() => setLimit(1000)}>Top 1000</button>
        </div>
      </div>

      <div style={{ overflowX: 'auto' }}>
        <table>
          <thead>
            <tr>
              <th className="rank-col">#</th>
              <th>Person</th>
              <th>Country</th>
              <th>Competition</th>
              <th>Date</th>
              <th style={{ textAlign: 'right' }}>Days</th>
              <th style={{ textAlign: 'right' }}>3BF</th>
              <th style={{ textAlign: 'right' }}>4BF</th>
              <th style={{ textAlign: 'right' }}>5BF</th>
              <th style={{ textAlign: 'right' }}>MBF cubes</th>
              <th className="value-col" style={{ textAlign: 'right' }}>Total bits</th>
              <th className="value-col" style={{ textAlign: 'right' }}>Bits/day</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={`${r.competition_id}-${r.person_id}`}>
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
                <td>
                  <a
                    href={`https://www.worldcubeassociation.org/competitions/${r.competition_id}`}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {r.competition_name}
                  </a>
                </td>
                <td className="muted">{r.date}</td>
                <td style={{ textAlign: 'right' }}>{r.days}</td>
                <td style={{ textAlign: 'right' }}>{r.bf3_solves || ''}</td>
                <td style={{ textAlign: 'right' }}>{r.bf4_solves || ''}</td>
                <td style={{ textAlign: 'right' }}>{r.bf5_solves || ''}</td>
                <td style={{ textAlign: 'right' }}>{r.mbf_solved || ''}</td>
                <td className="value-col" style={{ textAlign: 'right' }}>{fmtBits(r.total_bits)}</td>
                <td className="value-col" style={{ textAlign: 'right' }}>{fmtBits(r.per_day_bits)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
