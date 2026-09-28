'use client';

import { useEffect, useState } from 'react';
import { formatAverage } from '@/lib/format';

type PersonRef = { id: string; name: string; country: string };
type PairEntry = {
  a: PersonRef;
  b: PersonRef;
  time_cs: number;
  time_a: number;
  time_b: number;
  events_a: string[];
  events_b: string[];
};
type RegionEntry = { id: string; name: string; pair: PairEntry };
type ChallengeData = {
  events: string[];
  global: PairEntry | null;
  continents: RegionEntry[];
  countries: RegionEntry[];
};
type TwoManData = { mini: ChallengeData; guild: ChallengeData };

const SHORT: Record<string, string> = {
  '222': '2x2', '333': '3x3', '444': '4x4', '555': '5x5',
  '666': '6x6', '777': '7x7', '333oh': '3OH', '333bf': '3BF',
  clock: 'Clk', minx: 'Mega', pyram: 'Pyra', skewb: 'Skwb', sq1: 'SQ-1',
};

function fmtEvList(evs: string[]): string {
  return evs.map(e => SHORT[e] ?? e).join(' ');
}

function PersonLink({ p }: { p: PersonRef }) {
  return (
    <a
      href={`https://www.worldcubeassociation.org/persons/${p.id}`}
      target="_blank"
      rel="noreferrer"
    >
      {p.name}
    </a>
  );
}

function PairCells({ pair }: { pair: PairEntry }) {
  return (
    <>
      <td style={{ minWidth: 160 }}>
        <PersonLink p={pair.a} />
        <div className="muted" style={{ fontSize: 11 }}>{pair.a.country}</div>
      </td>
      <td style={{ minWidth: 160 }}>
        <PersonLink p={pair.b} />
        <div className="muted" style={{ fontSize: 11 }}>{pair.b.country}</div>
      </td>
      <td className="muted" style={{ fontSize: 12, minWidth: 140 }}>
        {fmtEvList(pair.events_a)}
      </td>
      <td style={{ textAlign: 'right' }}>{formatAverage(pair.time_a, '333')}</td>
      <td className="muted" style={{ fontSize: 12, minWidth: 140 }}>
        {fmtEvList(pair.events_b)}
      </td>
      <td style={{ textAlign: 'right' }}>{formatAverage(pair.time_b, '333')}</td>
      <td className="value-col" style={{ textAlign: 'right' }}>
        {formatAverage(pair.time_cs, '333')}
      </td>
    </>
  );
}

function RegionTable({ regions, labelHeader }: { regions: RegionEntry[]; labelHeader: string }) {
  if (regions.length === 0) {
    return <div className="empty">No qualifying pair found.</div>;
  }
  return (
    <div style={{ overflowX: 'auto' }}>
      <table>
        <thead>
          <tr>
            <th className="rank-col">#</th>
            <th>{labelHeader}</th>
            <th>Person A</th>
            <th>Person B</th>
            <th>A&apos;s Events</th>
            <th style={{ textAlign: 'right' }}>A&apos;s Time</th>
            <th>B&apos;s Events</th>
            <th style={{ textAlign: 'right' }}>B&apos;s Time</th>
            <th style={{ textAlign: 'right', minWidth: 88 }}>Team Time</th>
          </tr>
        </thead>
        <tbody>
          {regions.map((r, i) => (
            <tr key={r.id}>
              <td className="rank-col">{i + 1}</td>
              <td style={{ fontWeight: 500, minWidth: 100 }}>{r.name}</td>
              <PairCells pair={r.pair} />
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function GlobalCard({ pair }: { pair: PairEntry | null }) {
  if (!pair) return <div className="empty">No qualifying pair found.</div>;
  return (
    <div style={{ overflowX: 'auto' }}>
      <table>
        <thead>
          <tr>
            <th>Person A</th>
            <th>Person B</th>
            <th>A&apos;s Events</th>
            <th style={{ textAlign: 'right' }}>A&apos;s Time</th>
            <th>B&apos;s Events</th>
            <th style={{ textAlign: 'right' }}>B&apos;s Time</th>
            <th style={{ textAlign: 'right', minWidth: 88 }}>Team Time</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <PairCells pair={pair} />
          </tr>
        </tbody>
      </table>
    </div>
  );
}

export default function TwoManTable() {
  const [data, setData] = useState<TwoManData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [challenge, setChallenge] = useState<'mini' | 'guild'>('mini');

  useEffect(() => {
    fetch('/data/two_man.json')
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch(e => setError(String(e)));
  }, []);

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
        Events: {ch.events.map(e => SHORT[e] ?? e).join(' · ')}
        {' · '}
        Team time = max(person A total, person B total).
      </div>

      <h2 style={{ marginTop: 24, marginBottom: 8 }}>Global Best</h2>
      <GlobalCard pair={ch.global} />

      <h2 style={{ marginTop: 32, marginBottom: 8 }}>Best by Continent</h2>
      <RegionTable regions={ch.continents} labelHeader="Continent" />

      <h2 style={{ marginTop: 32, marginBottom: 8 }}>Best by Country</h2>
      <div className="muted" style={{ marginBottom: 8, fontSize: 12 }}>
        Only countries with a qualifying pair (two people who together cover every event) are listed.
      </div>
      <RegionTable regions={ch.countries} labelHeader="Country" />
    </>
  );
}
