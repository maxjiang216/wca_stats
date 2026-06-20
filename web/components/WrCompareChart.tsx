'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { formatSingle, formatAverage } from '@/lib/format';

type WrEntry = { name: string; pid: string; value: number; date: string };
type Data = {
  events: string[];
  single: Record<string, WrEntry[]>;
  average: Record<string, WrEntry[]>;
};

// Events to compare and their line colours.
const SERIES = [
  { event: '333', label: '3×3', color: '#60a5fa' },
  { event: 'sq1', label: 'Square-1', color: '#f59e0b' },
];

const VW = 860,
  VH = 440;
const ML = 70,
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

function fmtSeconds(cs: number): string {
  const s = cs / 100;
  if (s >= 60) {
    const m = Math.floor(s / 60);
    return `${m}:${String(Math.round(s - m * 60)).padStart(2, '0')}`;
  }
  return `${s.toFixed(s < 10 ? 2 : 1)}s`;
}

export default function WrCompareChart() {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<'single' | 'average'>('single');
  const [hoverFrac, setHoverFrac] = useState<number | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    fetch('/data/wr_longevity.json')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, []);

  const chart = useMemo(() => {
    if (!data) return null;
    const table = tab === 'single' ? data.single : data.average;

    const today = new Date();
    const todayFrac = today.getFullYear() + (today.getMonth()) / 12 + today.getDate() / 365.25;

    const series = SERIES.map((s) => {
      const raw = (table[s.event] ?? []).slice().sort((a, b) => a.date.localeCompare(b.date));
      const pts = raw.map((e) => ({ ...e, frac: dateToFrac(e.date) }));
      return { ...s, pts };
    }).filter((s) => s.pts.length > 0);

    if (series.length === 0) return null;

    const allFracs = series.flatMap((s) => s.pts.map((p) => p.frac));
    const allVals = series.flatMap((s) => s.pts.map((p) => p.value));
    const xMin = Math.min(...allFracs);
    const xMax = Math.max(todayFrac, ...allFracs);
    const vMin = Math.min(...allVals);
    const vMax = Math.max(...allVals);
    const logMin = Math.log10(vMin);
    const logMax = Math.log10(vMax);

    const toX = (f: number) => ML + ((f - xMin) / (xMax - xMin || 1)) * CW;
    const toY = (v: number) =>
      MT + (1 - (Math.log10(v) - logMin) / (logMax - logMin || 1)) * CH;

    // Step paths (a WR holds until the next one, extended to today).
    const paths = series.map((s) => {
      let d = '';
      s.pts.forEach((p, i) => {
        const x = toX(p.frac);
        const y = toY(p.value);
        if (i === 0) d += `M ${x.toFixed(1)} ${y.toFixed(1)}`;
        else d += ` H ${x.toFixed(1)} V ${y.toFixed(1)}`;
      });
      d += ` H ${toX(xMax).toFixed(1)}`;
      return { ...s, d };
    });

    // Y ticks: nice log-spaced seconds.
    const yTicks: number[] = [];
    for (const cs of [200, 300, 400, 500, 600, 800, 1000, 1500, 2000, 3000, 4000, 6000])
      if (cs >= vMin * 0.95 && cs <= vMax * 1.05) yTicks.push(cs);

    const yearTicks: number[] = [];
    for (let y = Math.ceil(xMin / 5) * 5; y <= Math.floor(xMax / 5) * 5; y += 5)
      yearTicks.push(y);

    return { series, paths, toX, toY, xMin, xMax, yTicks, yearTicks };
  }, [data, tab]);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data || !chart) return <div className="loading">Loading…</div>;

  function activeAt(pts: { frac: number; value: number; date: string; name: string }[], frac: number) {
    let lo = -1;
    for (let i = 0; i < pts.length; i++) if (pts[i].frac <= frac) lo = i;
    return lo >= 0 ? pts[lo] : null;
  }

  function onMove(e: React.MouseEvent<SVGSVGElement>) {
    const rect = svgRef.current!.getBoundingClientRect();
    const svgX = ((e.clientX - rect.left) / rect.width) * VW;
    if (svgX < ML || svgX > ML + CW) {
      setHoverFrac(null);
      return;
    }
    setHoverFrac(chart!.xMin + ((svgX - ML) / CW) * (chart!.xMax - chart!.xMin));
  }

  const hx = hoverFrac !== null ? chart.toX(hoverFrac) : null;
  const fmt = tab === 'single' ? formatSingle : formatAverage;

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

      <div style={{ display: 'flex', gap: 16, margin: '8px 0' }}>
        {chart.series.map((s) => (
          <span key={s.event} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13 }}>
            <span style={{ width: 14, height: 3, background: s.color, display: 'inline-block' }} />
            {s.label}
          </span>
        ))}
      </div>

      <svg
        ref={svgRef}
        viewBox={`0 0 ${VW} ${VH}`}
        style={{ width: '100%', maxWidth: VW, display: 'block' }}
        onMouseMove={onMove}
        onMouseLeave={() => setHoverFrac(null)}
      >
        {chart.yTicks.map((cs) => {
          const y = chart.toY(cs);
          return (
            <g key={cs}>
              <line x1={ML} y1={y} x2={ML + CW} y2={y} stroke="#2a2a2a" strokeWidth={1} strokeDasharray="3 4" />
              <text x={ML - 6} y={y} textAnchor="end" dominantBaseline="middle" fontSize="11" fill="#666">
                {fmtSeconds(cs)}
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
          <path key={s.event} d={s.d} fill="none" stroke={s.color} strokeWidth="1.8" strokeLinejoin="round" />
        ))}

        {hx !== null && hoverFrac !== null && (
          <>
            <line x1={hx} y1={MT} x2={hx} y2={MT + CH} stroke="#666" strokeWidth="1" strokeDasharray="4 3" />
            {chart.series.map((s) => {
              const p = activeAt(s.pts, hoverFrac);
              if (!p) return null;
              return <circle key={s.event} cx={hx} cy={chart.toY(p.value)} r="4" fill={s.color} />;
            })}
            {(() => {
              const left = hx > ML + CW * 0.6;
              const tx = left ? hx - 184 : hx + 10;
              const ty = MT + 6;
              return (
                <g>
                  <rect x={tx} y={ty} width={174} height={20 + chart.series.length * 18} rx="4" fill="#111" stroke="#444" />
                  <text x={tx + 87} y={ty + 14} textAnchor="middle" fontSize="11" fill="#aaa">
                    {Math.floor(hoverFrac)}
                  </text>
                  {chart.series.map((s, i) => {
                    const p = activeAt(s.pts, hoverFrac);
                    return (
                      <text key={s.event} x={tx + 8} y={ty + 32 + i * 18} fontSize="12" fill={s.color}>
                        {s.label}: {p ? fmt(p.value, s.event) : '—'}
                      </text>
                    );
                  })}
                </g>
              );
            })()}
          </>
        )}

        <text x={ML + CW / 2} y={VH - 4} textAnchor="middle" fontSize="12" fill="#777">Year</text>
        <text x={-(MT + CH / 2)} y={14} textAnchor="middle" fontSize="12" fill="#777" transform="rotate(-90)">
          World record {tab} (log)
        </text>
      </svg>
    </div>
  );
}
