'use client';

import { useEffect, useState } from 'react';
import { formatSingle } from '@/lib/format';
import { EVENT_NAMES, sortEvents } from '@/lib/stats';

type RankEntry = {
  pid: string;
  name: string;
  cid: string;
  est: number;
  n_comps: number;
  last_date: string;
};

type EventSkill = {
  lambda_per_day: number;
  rankings: RankEntry[];
};

type SkillData = Record<string, EventSkill>;

const LIMITS = [100, 1000] as const;

export default function SkillEstimatorTable() {
  const [data, setData] = useState<SkillData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [event, setEvent] = useState<string>('333');
  const [limit, setLimit] = useState<100 | 1000>(100);

  useEffect(() => {
    fetch('/data/skill_estimator.json')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then((d: SkillData) => {
        setData(d);
        if (!d['333'] && Object.keys(d).length > 0) {
          setEvent(sortEvents(Object.keys(d))[0]);
        }
      })
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const events = sortEvents(Object.keys(data));
  const skill = data[event];
  if (!skill) return null;

  const rows = skill.rankings.slice(0, limit);
  const halfLifeDays = Math.round(Math.LN2 / skill.lambda_per_day);

  return (
    <div>
      {/* Event tabs */}
      <div className="tabs">
        {events.map((ev) => (
          <button
            key={ev}
            onClick={() => { setEvent(ev); setLimit(100); }}
            className={ev === event ? 'active' : ''}
          >
            {EVENT_NAMES[ev] ?? ev}
          </button>
        ))}
      </div>

      <div className="table-meta">
        Decay half-life: <strong>{halfLifeDays} days</strong>
        &nbsp;·&nbsp;λ = {skill.lambda_per_day.toFixed(5)}/day
      </div>

      <table>
        <thead>
          <tr>
            <th>#</th>
            <th>Name</th>
            <th>Country</th>
            <th className="value-col">Estimate</th>
            <th className="value-col">Comps</th>
            <th className="value-col">Last</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((entry, i) => (
            <tr key={entry.pid}>
              <td>{i + 1}</td>
              <td>
                <a
                  href={`https://www.worldcubeassociation.org/persons/${entry.pid}`}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  {entry.name}
                </a>
              </td>
              <td>{entry.cid}</td>
              <td className="value-col">{formatSingle(Math.round(entry.est), event)}</td>
              <td className="value-col">{entry.n_comps}</td>
              <td className="value-col">{entry.last_date}</td>
            </tr>
          ))}
        </tbody>
      </table>

      {skill.rankings.length > limit && (
        <div style={{ marginTop: '12px' }}>
          {LIMITS.filter((l) => l !== limit && l <= skill.rankings.length).map((l) => (
            <button key={l} onClick={() => setLimit(l)}>
              Show top {l}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
