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
type CountryEntry = { country: string; pair: PairEntry };
type ChallengeData = { events: string[]; pairs: PairEntry[]; countries: CountryEntry[] };
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

function PairRows({ pairs, mode }: { pairs: PairEntry[]; mode: 'global' | 'country' }) {
  return (
    <tbody>
      {pairs.map((p, i) => (
        <tr key={`${p.a.id}-${p.b.id}`}>
          <td className="rank-col">{i + 1}</td>
          {mode === 'country' ? null : (
            <>
              <td style={{ minWidth: 160 }}>
                <PersonLink p={p.a} />
                <div className="muted" style={{ fontSize: 11 }}>{p.a.country}</div>
              </td>
              <td style={{ minWidth: 160 }}>
                <PersonLink p={p.b} />
                <div className="muted" style={{ fontSize: 11 }}>{p.b.country}</div>
              </td>
            </>
          )}
          <td className="muted" style={{ fontSize: 12, minWidth: 140 }}>
            {fmtEvList(p.events_a)}
          </td>
          <td style={{ textAlign: 'right' }}>{formatAverage(p.time_a, '333')}</td>
          <td className="muted" style={{ fontSize: 12, minWidth: 140 }}>
            {fmtEvList(p.events_b)}
          </td>
          <td style={{ textAlign: 'right' }}>{formatAverage(p.time_b, '333')}</td>
          <td className="value-col" style={{ textAlign: 'right' }}>
            {formatAverage(p.time_cs, '333')}
          </td>
        </tr>
      ))}
    </tbody>
  );
}

function CountryRows({ countries }: { countries: CountryEntry[] }) {
  return (
    <tbody>
      {countries.map((c, i) => {
        const p = c.pair;
        return (
          <tr key={c.country}>
            <td className="rank-col">{i + 1}</td>
            <td style={{ fontWeight: 500, minWidth: 80 }}>{c.country}</td>
            <td style={{ minWidth: 150 }}>
              <PersonLink p={p.a} />
            </td>
            <td style={{ minWidth: 150 }}>
              <PersonLink p={p.b} />
            </td>
            <td className="muted" style={{ fontSize: 12, minWidth: 130 }}>
              {fmtEvList(p.events_a)}
            </td>
            <td style={{ textAlign: 'right' }}>{formatAverage(p.time_a, '333')}</td>
            <td className="muted" style={{ fontSize: 12, minWidth: 130 }}>
              {fmtEvList(p.events_b)}
            </td>
            <td style={{ textAlign: 'right' }}>{formatAverage(p.time_b, '333')}</td>
            <td className="value-col" style={{ textAlign: 'right' }}>
              {formatAverage(p.time_cs, '333')}
            </td>
          </tr>
        );
      })}
    </tbody>
  );
}

type View = 'global' | 'country';

export default function TwoManTable() {
  const [data, setData] = useState<TwoManData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [challenge, setChallenge] = useState<'mini' | 'guild'>('mini');
  const [view, setView] = useState<View>('global');
  const [limit, setLimit] = useState<50 | 100>(50);

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
        <div className="toggle-group">
          <button className={view === 'global' ? 'active' : ''} onClick={() => setView('global')}>
            Top Pairs
          </button>
          <button className={view === 'country' ? 'active' : ''} onClick={() => setView('country')}>
            By Country
          </button>
        </div>
        {view === 'global' && (
          <div className="toggle-group">
            <button className={limit === 50 ? 'active' : ''} onClick={() => setLimit(50)}>Top 50</button>
            <button className={limit === 100 ? 'active' : ''} onClick={() => setLimit(100)}>Top 100</button>
          </div>
        )}
      </div>

      <div className="muted" style={{ marginBottom: 12, fontSize: 12 }}>
        Events: {ch.events.map(e => SHORT[e] ?? e).join(' · ')}
        {' · '}
        Team time = max(person A total, person B total). Split is optimised over all {(1 << ch.events.length).toLocaleString()} possible assignments.
      </div>

      <div style={{ overflowX: 'auto' }}>
        <table>
          <thead>
            <tr>
              <th className="rank-col">#</th>
              {view === 'global' ? (
                <>
                  <th>Person A</th>
                  <th>Person B</th>
                </>
              ) : (
                <>
                  <th>Country</th>
                  <th>Person A</th>
                  <th>Person B</th>
                </>
              )}
              <th>A&apos;s Events</th>
              <th style={{ textAlign: 'right' }}>A&apos;s Time</th>
              <th>B&apos;s Events</th>
              <th style={{ textAlign: 'right' }}>B&apos;s Time</th>
              <th style={{ textAlign: 'right', minWidth: 88 }}>Team Time</th>
            </tr>
          </thead>
          {view === 'global' ? (
            <PairRows pairs={ch.pairs.slice(0, limit)} mode="global" />
          ) : (
            <CountryRows countries={ch.countries} />
          )}
        </table>
      </div>
    </>
  );
}
