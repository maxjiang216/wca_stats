'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { EVENT_NAMES } from '@/lib/stats';

type Point = { date: string; top1: number; top2: number; country: number };
type Data = {
  events: string[];
  single: Record<string, Point[]>;
  average: Record<string, Point[]>;
};

// The three tracked series and their colours.
const SERIES = [
  { key: 'top1' as const, label: '#1 person', color: '#60a5fa' },
  { key: 'top2' as const, label: 'Top 2 people', color: '#a78bfa' },
  { key: 'country' as const, label: 'One country', color: '#f59e0b' },
];

const VW = 860,
  VH = 440;
const ML = 54,
  MR = 16,
  MT = 24,
  MB = 44;
const CW = VW - ML - MR;
const CH = VH - MT - MB;

function dateToFrac(date: string): number {
  const y = +date.slice(0, 4);
  const m = +date.slice(5, 7);
  const d = +date.slice(8, 10);
  return y + (m - 1) / 12 + (d - 1) / 365.25;
}

export default function DominanceChart() {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<'single' | 'average'>('single');
  const [event, setEvent] = useState<string>('333');
  const [hoverFrac, setHoverFrac] = useState<number | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    fetch('/data/dominance.json')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, []);

  const table = data ? (tab === 'single' ? data.single : data.average) : {};
  const eventsInTab = data ? data.events.filter((e) => table[e]) : [];
  const activeEvent = table[event] ? event : eventsInTab[0];

  const chart = useMemo(() => {
    if (!data || !activeEvent) return null;
    const raw = table[activeEvent] ?? [];
    if (raw.length === 0) return null;

    const pts = raw.map((p) => ({ ...p, frac: dateToFrac(p.date) }));
    const xMin = pts[0].frac;
    const xMax = pts[pts.length - 1].frac;
    const yMax = Math.max(1, ...pts.flatMap((p) => [p.top1, p.top2, p.country]));

    const toX = (f: number) => ML + ((f - xMin) / (xMax - xMin || 1)) * CW;
    const toY = (v: number) => MT + (1 - v / yMax) * CH;

    // Step paths — each value holds until the next sample.
    const paths = SERIES.map((s) => {
      let d = '';
      pts.forEach((p, i) => {
        const x = toX(p.frac);
        const y = toY(p[s.key]);
        if (i === 0) d += `M ${x.toFixed(1)} ${y.toFixed(1)}`;
        else d += ` H ${x.toFixed(1)} V ${y.toFixed(1)}`;
      });
      return { ...s, d };
    });

    // Y ticks: ~5 nice round steps.
    const step = niceStep(yMax / 5);
    const yTicks: number[] = [];
    for (let v = 0; v <= yMax + 1e-9; v += step) yTicks.push(v);

    const yearTicks: number[] = [];
    for (let y = Math.ceil(xMin / 5) * 5; y <= Math.floor(xMax / 5) * 5; y += 5)
      yearTicks.push(y);

    return { pts, paths, toX, toY, xMin, xMax, yMax, yTicks, yearTicks };
  }, [data, tab, activeEvent]);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data) return <div className="loading">Loading…</div>;

  function activeAt(pts: (Point & { frac: number })[], frac: number) {
    let lo = -1;
    for (let i = 0; i < pts.length; i++) if (pts[i].frac <= frac) lo = i;
    return lo >= 0 ? pts[lo] : null;
  }

  function onMove(e: React.MouseEvent<SVGSVGElement>) {
    if (!chart) return;
    const rect = svgRef.current!.getBoundingClientRect();
    const svgX = ((e.clientX - rect.left) / rect.width) * VW;
    if (svgX < ML || svgX > ML + CW) {
      setHoverFrac(null);
      return;
    }
    setHoverFrac(chart.xMin + ((svgX - ML) / CW) * (chart.xMax - chart.xMin));
  }

  const hx = chart && hoverFrac !== null ? chart.toX(hoverFrac) : null;
  const hp = chart && hoverFrac !== null ? activeAt(chart.pts, hoverFrac) : null;

  return (
    <div>
      <div className="tabs">
        <button className={tab === 'single' ? 'tab active' : 'tab'} onClick={() => setTab('single')}>
          Single
        </button>
        <button className={tab === 'average' ? 'tab active' : 'tab'} onClick={() => setTab('average')}>
          Average
        </button>
      </div>

      <div className="tabs">
        {eventsInTab.map((e) => (
          <button
            key={e}
            className={e === activeEvent ? 'tab active' : 'tab'}
            onClick={() => setEvent(e)}
            title={EVENT_NAMES[e] ?? e}
          >
            {e}
          </button>
        ))}
      </div>

      <div style={{ display: 'flex', gap: 16, margin: '8px 0', flexWrap: 'wrap' }}>
        {SERIES.map((s) => (
          <span key={s.key} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13 }}>
            <span style={{ width: 14, height: 3, background: s.color, display: 'inline-block' }} />
            {s.label}
          </span>
        ))}
      </div>

      {!chart ? (
        <div className="empty">No data for this event.</div>
      ) : (
        <svg
          ref={svgRef}
          viewBox={`0 0 ${VW} ${VH}`}
          style={{ width: '100%', maxWidth: VW, display: 'block' }}
          onMouseMove={onMove}
          onMouseLeave={() => setHoverFrac(null)}
        >
          {chart.yTicks.map((v) => {
            const y = chart.toY(v);
            return (
              <g key={v}>
                <line x1={ML} y1={y} x2={ML + CW} y2={y} stroke="#2a2a2a" strokeWidth={1} strokeDasharray="3 4" />
                <text x={ML - 6} y={y} textAnchor="end" dominantBaseline="middle" fontSize="11" fill="#666">
                  {v}
                </text>
              </g>
            );
          })}

          {chart.yearTicks.map((y) => (
            <g key={y}>
              <line x1={chart.toX(y)} y1={MT} x2={chart.toX(y)} y2={MT + CH} stroke="#2a2a2a" strokeWidth="1" strokeDasharray="3 4" />
              <text x={chart.toX(y)} y={MT + CH + 14} textAnchor="middle" fontSize="10" fill="#666">
                {y}
              </text>
            </g>
          ))}

          <rect x={ML} y={MT} width={CW} height={CH} fill="none" stroke="#444" strokeWidth="1" />

          {chart.paths.map((s) => (
            <path key={s.key} d={s.d} fill="none" stroke={s.color} strokeWidth="1.8" strokeLinejoin="round" />
          ))}

          {hx !== null && hoverFrac !== null && hp && (
            <>
              <line x1={hx} y1={MT} x2={hx} y2={MT + CH} stroke="#666" strokeWidth="1" strokeDasharray="4 3" />
              {SERIES.map((s) => (
                <circle key={s.key} cx={hx} cy={chart.toY(hp[s.key])} r="4" fill={s.color} />
              ))}
              {(() => {
                const left = hx > ML + CW * 0.6;
                const tx = left ? hx - 158 : hx + 10;
                const ty = MT + 6;
                return (
                  <g>
                    <rect x={tx} y={ty} width={148} height={20 + SERIES.length * 18} rx="4" fill="#111" stroke="#444" />
                    <text x={tx + 74} y={ty + 14} textAnchor="middle" fontSize="11" fill="#aaa">
                      {hp.date}
                    </text>
                    {SERIES.map((s, i) => (
                      <text key={s.key} x={tx + 8} y={ty + 32 + i * 18} fontSize="12" fill={s.color}>
                        {s.label}: {hp[s.key]}
                      </text>
                    ))}
                  </g>
                );
              })()}
            </>
          )}

          <text x={ML + CW / 2} y={VH - 4} textAnchor="middle" fontSize="12" fill="#777">Year</text>
          <text x={-(MT + CH / 2)} y={14} textAnchor="middle" fontSize="12" fill="#777" transform="rotate(-90)">
            # of top results
          </text>
        </svg>
      )}
    </div>
  );
}

function niceStep(x: number): number {
  if (x <= 0) return 1;
  const p = Math.pow(10, Math.floor(Math.log10(x)));
  const n = x / p;
  const m = n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10;
  return m * p;
}
