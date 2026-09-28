'use client';

import { useEffect, useState } from 'react';
import { formatAverage } from '@/lib/format';

type Member = { id: string; name: string; country: string; time_cs: number; events: string[] };
type Team = { time_cs: number; members: Member[] };
type RegionEntry = { id: string; name: string; team: Team };
type ChallengeData = {
  events: string[];
  global: Team | null;
  continents: RegionEntry[];
  countries: RegionEntry[];
};
type TeamData = { mini: ChallengeData; guild: ChallengeData };

const SHORT: Record<string, string> = {
  '222': '2x2', '333': '3x3', '444': '4x4', '555': '5x5',
  '666': '6x6', '777': '7x7', '333oh': '3OH',
  clock: 'Clk', minx: 'Mega', pyram: 'Pyra', skewb: 'Skwb', sq1: 'SQ-1',
};

function MemberLine({ m }: { m: Member }) {
  return (
    <div style={{ display: 'flex', gap: 8, alignItems: 'baseline', flexWrap: 'wrap' }}>
      <a href={`https://www.worldcubeassociation.org/persons/${m.id}`} target="_blank" rel="noreferrer">
        {m.name}
      </a>
      <span className="muted" style={{ fontSize: 12 }}>
        {m.events.length ? m.events.map((e) => SHORT[e] ?? e).join(' ') : 'no events'}
      </span>
      {m.events.length > 0 && (
        <span className="muted" style={{ fontSize: 12 }}>· {formatAverage(m.time_cs, '333')}</span>
      )}
    </div>
  );
}

function GlobalTeam({ team }: { team: Team | null }) {
  if (!team) return <div className="empty">No qualifying team found.</div>;
  return (
    <div style={{ overflowX: 'auto' }}>
      <table>
        <thead>
          <tr>
            <th>Person</th>
            <th>Country</th>
            <th>Events</th>
            <th style={{ textAlign: 'right' }}>Time</th>
          </tr>
        </thead>
        <tbody>
          {team.members.map((m) => (
            <tr key={m.id}>
              <td>
                <a href={`https://www.worldcubeassociation.org/persons/${m.id}`} target="_blank" rel="noreferrer">
                  {m.name}
                </a>
              </td>
              <td className="muted">{m.country}</td>
              <td className="muted" style={{ fontSize: 12 }}>
                {m.events.length ? m.events.map((e) => SHORT[e] ?? e).join(' ') : 'no events'}
              </td>
              <td style={{ textAlign: 'right' }}>{formatAverage(m.time_cs, '333')}</td>
            </tr>
          ))}
          <tr>
            <td colSpan={3} style={{ fontWeight: 500 }}>Team time</td>
            <td className="value-col" style={{ textAlign: 'right' }}>{formatAverage(team.time_cs, '333')}</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}

function RegionTable({ regions, label }: { regions: RegionEntry[]; label: string }) {
  if (regions.length === 0) return <div className="empty">No qualifying team found.</div>;
  return (
    <div style={{ overflowX: 'auto' }}>
      <table>
        <thead>
          <tr>
            <th className="rank-col">#</th>
            <th>{label}</th>
            <th>Team</th>
            <th style={{ textAlign: 'right', minWidth: 88 }}>Team Time</th>
          </tr>
        </thead>
        <tbody>
          {regions.map((r, i) => (
            <tr key={r.id}>
              <td className="rank-col">{i + 1}</td>
              <td style={{ fontWeight: 500, minWidth: 100 }}>{r.name}</td>
              <td>
                {r.team.members.map((m) => (
                  <MemberLine key={m.id} m={m} />
                ))}
              </td>
              <td className="value-col" style={{ textAlign: 'right' }}>{formatAverage(r.team.time_cs, '333')}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default function TeamTable({ size }: { size: number }) {
  const [data, setData] = useState<TeamData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [challenge, setChallenge] = useState<'mini' | 'guild'>('mini');

  useEffect(() => {
    fetch(`/data/team_${size}.json`)
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, [size]);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  const ch = data[challenge];

  return (
    <>
      <div className="toolbar">
        <div className="toggle-group">
          <button className={challenge === 'mini' ? 'active' : ''} onClick={() => setChallenge('mini')}>
            Mini Guildford
          </button>
          <button className={challenge === 'guild' ? 'active' : ''} onClick={() => setChallenge('guild')}>
            Guildford Challenge
          </button>
        </div>
      </div>

      <div className="muted" style={{ marginBottom: 12, fontSize: 12 }}>
        Events: {ch.events.map((e) => SHORT[e] ?? e).join(' · ')}
        {' · '}
        Team time = the slowest member&apos;s total.
      </div>

      <h2 style={{ marginTop: 24, marginBottom: 8 }}>Global Best</h2>
      <GlobalTeam team={ch.global} />

      <h2 style={{ marginTop: 32, marginBottom: 8 }}>Best by Continent</h2>
      <RegionTable regions={ch.continents} label="Continent" />

      <h2 style={{ marginTop: 32, marginBottom: 8 }}>Best by Country</h2>
      <div className="muted" style={{ marginBottom: 8, fontSize: 12 }}>
        Only countries with {size} competitors who together cover every event are listed.
      </div>
      <RegionTable regions={ch.countries} label="Country" />
    </>
  );
}
