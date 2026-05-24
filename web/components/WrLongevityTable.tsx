'use client';

import { useEffect, useState } from 'react';
import { EVENT_NAMES, sortEvents } from '@/lib/stats';
import { formatSingle, formatAverage } from '@/lib/format';

interface WrEntry {
  name: string;
  pid: string;
  value: number;
  date: string;
  comp: string;
  top10_days: number;
  top10_current: boolean;
  top100_days: number;
  top100_current: boolean;
}

interface Data {
  events: string[];
  single: Record<string, WrEntry[]>;
  average: Record<string, WrEntry[]>;
}

function fmtDays(days: number, current: boolean): string {
  if (days < 30) return `${days}d`;
  if (days < 365) return `${Math.round(days / 30.4)}mo`;
  const y = days / 365.25;
  return y >= 10 ? `${Math.floor(y)}y` : `${y.toFixed(1)}y`;
}

export default function WrLongevityTable() {
  const [data, setData] = useState<Data | null>(null);
  const [tab, setTab] = useState<'single' | 'average'>('single');
  const [event, setEvent] = useState<string>('333');

  useEffect(() => {
    fetch('/data/wr_longevity.json')
      .then((r) => r.json())
      .then((d: Data) => {
        setData(d);
        const table = tab === 'single' ? d.single : d.average;
        const evs = d.events.filter((e) => table[e]);
        if (evs.length > 0 && !table[event]) setEvent(evs[0]);
      });
  }, []);

  if (!data) return <div className="loading">Loading…</div>;

  const table = tab === 'single' ? data.single : data.average;
  const eventsInTab = data.events.filter((e) => table[e]);
  const rows = table[event] ?? [];

  const formatValue = (v: number, e: string) =>
    tab === 'single' ? formatSingle(v, e) : formatAverage(v, e);

  const switchTab = (t: 'single' | 'average') => {
    setTab(t);
    const newTable = t === 'single' ? data.single : data.average;
    const evs = data.events.filter((e) => newTable[e]);
    if (evs.length > 0 && !newTable[event]) setEvent(evs[0]);
  };

  const maxTop10 = Math.max(...rows.map((r) => r.top10_days));
  const maxTop100 = Math.max(...rows.map((r) => r.top100_days));

  return (
    <div>
      <div className="tabs">
        <button className={tab === 'single' ? 'tab active' : 'tab'} onClick={() => switchTab('single')}>
          Single
        </button>
        <button className={tab === 'average' ? 'tab active' : 'tab'} onClick={() => switchTab('average')}>
          Average
        </button>
      </div>

      <div className="tabs">
        {eventsInTab.map((e) => (
          <button
            key={e}
            className={e === event ? 'tab active' : 'tab'}
            onClick={() => setEvent(e)}
            title={EVENT_NAMES[e] ?? e}
          >
            {e}
          </button>
        ))}
      </div>

      <table className="ranking-table" style={{ tableLayout: 'fixed', width: '100%' }}>
        <thead>
          <tr>
            <th style={{ width: 36 }}>#</th>
            <th>Person</th>
            <th style={{ width: 80 }}>Result</th>
            <th style={{ width: 96 }}>Date</th>
            <th style={{ width: 140 }}>Days in top 10</th>
            <th style={{ width: 140 }}>Days in top 100</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={i}>
              <td className="rank-col">{i + 1}</td>
              <td>
                <a
                  href={`https://www.worldcubeassociation.org/persons/${r.pid}`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {r.name}
                </a>
              </td>
              <td className="value-col">{formatValue(r.value, event)}</td>
              <td className="muted">{r.date}</td>
              <td>
                <DurationBar
                  days={r.top10_days}
                  current={r.top10_current}
                  max={maxTop10}
                />
              </td>
              <td>
                <DurationBar
                  days={r.top100_days}
                  current={r.top100_current}
                  max={maxTop100}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function DurationBar({
  days,
  current,
  max,
}: {
  days: number;
  current: boolean;
  max: number;
}) {
  const pct = max > 0 ? (days / max) * 100 : 0;
  const label = fmtDays(days, current);

  return (
    <div className="dur-wrap">
      <div
        className={`dur-bar ${current ? 'dur-current' : 'dur-done'}`}
        style={{ width: `${pct}%` }}
      />
      <span className={`dur-label ${current ? 'dur-label-current' : ''}`}>
        {label}
        {current && <span className="dur-now"> now</span>}
      </span>
    </div>
  );
}
