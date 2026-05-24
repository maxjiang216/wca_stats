'use client';

import { useEffect, useState } from 'react';
import { EVENT_NAMES, sortEvents } from '@/lib/stats';
import { formatSingle, formatAverage } from '@/lib/format';

interface FirstRecord {
  name: string;
  pid: string;
  date: string;
  comp: string;
  record: 'WR' | 'CR';
  value: number;
}

type EventMap = Record<string, FirstRecord>;
type CountryMap = Record<string, EventMap>;

interface Data {
  events: string[];
  countries: Record<string, string>;
  single: CountryMap;
  average: CountryMap;
}

export default function FirstRecordsTable() {
  const [data, setData] = useState<Data | null>(null);
  const [tab, setTab] = useState<'single' | 'average'>('single');

  useEffect(() => {
    fetch('/data/first_records.json')
      .then((r) => r.json())
      .then(setData);
  }, []);

  if (!data) return <div className="loading">Loading…</div>;

  const table = tab === 'single' ? data.single : data.average;
  const events = data.events;

  // Countries that have at least one record in the current tab.
  const activeCountries = Object.keys(table).sort((a, b) => {
    const na = data.countries[a] ?? a;
    const nb = data.countries[b] ?? b;
    return na.localeCompare(nb);
  });

  function fmt(rec: FirstRecord, event: string): string {
    if (tab === 'single') return formatSingle(rec.value, event);
    return formatAverage(rec.value, event);
  }

  const shortEvent = (e: string) => EVENT_NAMES[e]?.replace(/x\dx\d Cube/, '')
    .replace('3x3x3 ', '')
    .replace('x3x3x3 ', '') ?? e;

  return (
    <div className="first-records-wrap">
      <div className="tabs">
        <button
          className={tab === 'single' ? 'tab active' : 'tab'}
          onClick={() => setTab('single')}
        >
          Single
        </button>
        <button
          className={tab === 'average' ? 'tab active' : 'tab'}
          onClick={() => setTab('average')}
        >
          Average
        </button>
      </div>

      <div className="first-records-scroll">
        <table className="first-records-table">
          <thead>
            <tr>
              <th className="country-col">Country</th>
              {events.map((e) => (
                <th key={e} title={EVENT_NAMES[e] ?? e}>
                  {e}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {activeCountries.map((cid) => {
              const eventMap = table[cid] ?? {};
              return (
                <tr key={cid}>
                  <td className="country-col">{data.countries[cid] ?? cid}</td>
                  {events.map((e) => {
                    const rec = eventMap[e];
                    if (!rec) return <td key={e} className="empty">—</td>;
                    return (
                      <td key={e} className={`cell ${rec.record === 'WR' ? 'wr' : 'cr'}`}>
                        <a
                          href={`https://www.worldcubeassociation.org/persons/${rec.pid}`}
                          target="_blank"
                          rel="noreferrer"
                          title={`${rec.name} · ${rec.date} · ${rec.comp}`}
                        >
                          {rec.name}
                        </a>
                        <span className="cell-value">{fmt(rec, e)}</span>
                        <span className={`badge ${rec.record}`}>{rec.record}</span>
                      </td>
                    );
                  })}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
