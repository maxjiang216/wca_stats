'use client';

import { useEffect, useState } from 'react';
import { sortEvents } from '@/lib/stats';
import { formatAverage, formatMbldSingle } from '@/lib/format';

type Member = {
  name: string;
  person_id: string;
  avg_cs: number;
  comp: string;
};

type CountryEntry = {
  country: string;
  total: number;
  members: Member[];
};

type NationsCupData = Record<string, CountryEntry[]>;

const SHORT: Record<string, string> = {
  '222': '2x2', '333': '3x3', '444': '4x4', '555': '5x5', '666': '6x6', '777': '7x7',
  '333oh': '3OH', '333bf': '3BF', '333fm': 'FM', '333mbf': 'MBLD',
  '444bf': '4BF', '555bf': '5BF', clock: 'Clock', minx: 'Mega',
  pyram: 'Pyra', skewb: 'Skwb', sq1: 'SQ-1',
};

function formatValue(value: number, eventId: string): string {
  if (eventId === '333mbf') return formatMbldSingle(value);
  return formatAverage(value, eventId);
}

export default function NationsCupTable() {
  const [data, setData] = useState<NationsCupData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [active, setActive] = useState('333');

  useEffect(() => {
    fetch('/data/nations_cup.json')
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then((d: NationsCupData) => {
        setData(d);
        if (!d['333'] && Object.keys(d).length > 0) {
          setActive(sortEvents(Object.keys(d))[0]);
        }
      })
      .catch(e => setError(String(e)));
  }, []);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const events = sortEvents(Object.keys(data));
  const rows = data[active] ?? [];
  const isMbld = active === '333mbf';

  return (
    <>
      <div className="tabs">
        {events.map(e => (
          <button key={e} className={e === active ? 'active' : ''} onClick={() => setActive(e)}>
            {SHORT[e] ?? e}
          </button>
        ))}
      </div>

      <div className="muted" style={{ marginBottom: 12, fontSize: 12 }}>
        {rows.length} countries qualify (≥ 3 ranked competitors in this event)
      </div>

      <div style={{ overflowX: 'auto' }}>
        <table>
          <thead>
            <tr>
              <th className="rank-col">#</th>
              <th>Country</th>
              <th>1st place</th>
              <th>2nd place</th>
              <th>3rd place</th>
              {!isMbld && <th style={{ textAlign: 'right', minWidth: 96 }}>Team Total</th>}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={r.country}>
                <td className="rank-col">{i + 1}</td>
                <td style={{ fontWeight: 500 }}>{r.country}</td>
                {r.members.map((m, j) => (
                  <td key={j} style={{ minWidth: 160 }}>
                    <div style={{ fontWeight: 600, fontSize: 13 }}>
                      {formatValue(m.avg_cs, active)}
                    </div>
                    <div style={{ fontSize: 11, color: '#888', marginTop: 1 }}>
                      <a
                        href={`https://www.worldcubeassociation.org/persons/${m.person_id}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {m.name}
                      </a>
                      {m.comp && (
                        <>
                          {' @ '}
                          <a
                            href={`https://www.worldcubeassociation.org/competitions/${m.comp}`}
                            target="_blank"
                            rel="noreferrer"
                          >
                            {m.comp}
                          </a>
                        </>
                      )}
                    </div>
                  </td>
                ))}
                {!isMbld && (
                  <td className="value-col" style={{ textAlign: 'right' }}>
                    {formatAverage(r.total, active)}
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
