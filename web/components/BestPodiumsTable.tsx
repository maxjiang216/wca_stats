'use client';

import { useEffect, useState } from 'react';
import { formatAverage, formatSingle } from '@/lib/format';
import { EVENT_NAMES, sortEvents } from '@/lib/stats';

type Member = {
  person_id: string;
  person_name: string;
  country_id: string;
  pos: number;
  value: number;
};

type Podium = {
  rank: number;
  competition_id: string;
  competition_name: string;
  date: string;
  format_id: string;
  total: number;
  members: Member[];
};

type Group = 'bo' | 'mo3' | 'ao5';
type Data = Partial<Record<Group, Record<string, Podium[]>>>;

const GROUPS: { id: Group; label: string }[] = [
  { id: 'bo', label: 'Best of X' },
  { id: 'mo3', label: 'Mean of 3' },
  { id: 'ao5', label: 'Average of 5' },
];

const FORMAT_LABELS: Record<string, string> = { '1': 'Bo1', '2': 'Bo2', '3': 'Bo3', '5': 'Bo5' };

export default function BestPodiumsTable() {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [event, setEvent] = useState('333');
  const [group, setGroup] = useState<Group>('ao5');

  useEffect(() => {
    fetch('/data/best_podiums.json')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const events = sortEvents([
    ...new Set(GROUPS.flatMap((g) => Object.keys(data[g.id] ?? {}))),
  ]);
  const groupsFor = (ev: string) => GROUPS.filter((g) => data[g.id]?.[ev]?.length);
  const available = groupsFor(event);
  // Fall back to the first format this event has if the chosen one is absent.
  const activeGroup = available.some((g) => g.id === group) ? group : available[0]?.id;
  const rows = activeGroup ? data[activeGroup]?.[event] ?? [] : [];
  const fmt = (v: number) =>
    activeGroup === 'bo' ? formatSingle(v, event) : formatAverage(v, event);

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
          {available.map((g) => (
            <button
              key={g.id}
              className={activeGroup === g.id ? 'active' : ''}
              onClick={() => setGroup(g.id)}
            >
              {g.label}
            </button>
          ))}
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
                <th className="value-col" style={{ textAlign: 'right' }}>Sum</th>
                <th>1st</th>
                <th>2nd</th>
                <th>3rd</th>
                <th>Competition</th>
                <th>Date</th>
                {activeGroup === 'bo' && <th>Format</th>}
              </tr>
            </thead>
            <tbody>
              {rows.map((p) => (
                <tr key={p.competition_id}>
                  <td className="rank-col">{p.rank}</td>
                  <td className="value-col" style={{ textAlign: 'right' }}>{fmt(p.total)}</td>
                  {p.members.map((m) => (
                    <td key={m.person_id}>
                      <a
                        href={`https://www.worldcubeassociation.org/persons/${m.person_id}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {m.person_name}
                      </a>
                      <span className="muted"> {fmt(m.value)}</span>
                    </td>
                  ))}
                  <td>
                    <a
                      href={`https://www.worldcubeassociation.org/competitions/${p.competition_id}/results/podiums`}
                      target="_blank"
                      rel="noreferrer"
                    >
                      {p.competition_name}
                    </a>
                  </td>
                  <td className="muted">{p.date}</td>
                  {activeGroup === 'bo' && (
                    <td className="muted">{FORMAT_LABELS[p.format_id] ?? p.format_id}</td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
