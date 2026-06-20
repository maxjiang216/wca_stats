'use client';

import { useEffect, useState } from 'react';
import { formatSingle } from '@/lib/format';
import { EVENT_NAMES, sortEvents } from '@/lib/stats';

type RankEntry = {
  rank: number;
  pid: string;
  name: string;
  cid: string;
  est: number; // centiseconds (projected current mean single)
  level_sd: number;
  velocity_pct_wk: number;
  cv: number;
  dnf_rate: number;
  n_weeks: number;
  last_date: string;
  e_ao5: number; // centiseconds
  p_sub: Record<string, number>;
  p_wr_single: number;
  p_wr_ao5: number;
};

type EventSkill = {
  event: string;
  week_start: string;
  phi: number;
  q_eta: number;
  q_xi: number;
  wr_single: number;
  wr_ao5: number;
  rankings: RankEntry[];
};

type KalmanData = Record<string, EventSkill>;

const LIMITS = [100, 1000] as const;
const pct = (x: number) => `${(x * 100).toFixed(1)}%`;

export default function KalmanSkillTable() {
  const [data, setData] = useState<KalmanData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [event, setEvent] = useState<string>('333');
  const [limit, setLimit] = useState<100 | 1000>(100);

  useEffect(() => {
    fetch('/data/kalman_skill.json')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then((d: KalmanData) => {
        setData(d);
        if (!d['333'] && Object.keys(d).length > 0) setEvent(sortEvents(Object.keys(d))[0]);
      })
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const events = sortEvents(Object.keys(data));
  const ev = data[event];
  if (!ev) return null;
  const rows = ev.rankings.slice(0, limit);
  const trendHalfLife = Math.round(Math.LN2 / Math.max(1 - ev.phi, 1e-6));

  return (
    <div>
      <div className="tabs">
        {events.map((e) => (
          <button
            key={e}
            onClick={() => { setEvent(e); setLimit(100); }}
            className={e === event ? 'active' : ''}
          >
            {EVENT_NAMES[e] ?? e}
          </button>
        ))}
      </div>

      <div className="table-meta">
        Weeks start <strong>{ev.week_start}</strong>
        &nbsp;·&nbsp;trend damping φ = {ev.phi.toFixed(3)} ({trendHalfLife}-wk half-life)
        &nbsp;·&nbsp;WR single {formatSingle(ev.wr_single, event)}, WR ao5 {formatSingle(ev.wr_ao5, event)}
      </div>

      <table>
        <thead>
          <tr>
            <th>#</th>
            <th>Name</th>
            <th>Country</th>
            <th className="value-col">Skill (single)</th>
            <th className="value-col">E[ao5]</th>
            <th className="value-col">Trend %/wk</th>
            <th className="value-col">CV</th>
            <th className="value-col">DNF</th>
            <th className="value-col">P(WR ao5)</th>
            <th className="value-col">Wks</th>
            <th className="value-col">Last</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((e) => (
            <tr key={e.pid}>
              <td>{e.rank}</td>
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
              <td className="value-col">{formatSingle(Math.round(e.est), event)}</td>
              <td className="value-col">{formatSingle(Math.round(e.e_ao5), event)}</td>
              <td className="value-col" style={{ color: e.velocity_pct_wk < 0 ? '#2a8' : undefined }}>
                {e.velocity_pct_wk >= 0 ? '+' : ''}{e.velocity_pct_wk.toFixed(2)}
              </td>
              <td className="value-col">{pct(e.cv)}</td>
              <td className="value-col">{pct(e.dnf_rate)}</td>
              <td className="value-col">{pct(e.p_wr_ao5)}</td>
              <td className="value-col">{e.n_weeks}</td>
              <td className="value-col">{e.last_date}</td>
            </tr>
          ))}
        </tbody>
      </table>

      {ev.rankings.length > limit && (
        <div style={{ marginTop: '12px' }}>
          {LIMITS.filter((l) => l !== limit && l <= ev.rankings.length).map((l) => (
            <button key={l} onClick={() => setLimit(l)}>
              Show top {l}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
