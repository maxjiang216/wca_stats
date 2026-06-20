'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { EVENT_NAMES } from '@/lib/stats';
import { formatSingle, formatAverage } from '@/lib/format';

type GraphPoint = { date: string; value: number; rank: number; pool: number };
type TableRow = {
  date: string;
  name: string;
  pid: string;
  comp: string;
  value: number;
  rank: number;
  pool: number;
};
type Comparison = {
  id: string;
  group: string; // "avg_single" | "nxn" | "cross"
  title: string;
  subject_event: string;
  subject_type: string; // "single" | "average"
  ref_event: string;
  ref_type: string;
  graph: GraphPoint[];
  table: TableRow[];
};
type Data = { comparisons: Comparison[] };

const GROUPS: { id: string; label: string }[] = [
  { id: 'avg_single', label: 'Average vs Singles (same event)' },
  { id: 'nxn', label: 'NxN vs (N-1)x(N-1)' },
  { id: 'cross', label: 'Other cross-event' },
];

function tabLabel(c: Comparison): string {
  const t = c.subject_type === 'average' ? 'avg' : 'single';
  if (c.group === 'avg_single') return c.subject_event;
  return `${c.subject_event}→${c.ref_event} (${t})`;
}

function fmt(value: number, ev: string, type: string): string {
  return type === 'average' ? formatAverage(value, ev) : formatSingle(value, ev);
}

// ── Chart geometry ──────────────────────────────────────────────────────────
const VW = 860,
  VH = 420;
const ML = 64,
  MR = 16,
  MT = 24,
  MB = 44;
const CW = VW - ML - MR;
const CH = VH - MT - MB;
const LINE_COLOR = '#60a5fa';

function dateToFrac(date: string): number {
  const y = +date.slice(0, 4);
  const m = +date.slice(5, 7);
  const d = +date.slice(8, 10);
  return y + (m - 1) / 12 + (d - 1) / 365.25;
}

export default function WrCrossRank() {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [group, setGroup] = useState<string>('avg_single');
  const [sel, setSel] = useState<string>('av_333');
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    fetch('/data/wr_cross_rank.json')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, []);

  const inGroup = useMemo(
    () => (data ? data.comparisons.filter((c) => c.group === group) : []),
    [data, group]
  );

  const comp = useMemo(
    () => inGroup.find((c) => c.id === sel) ?? inGroup[0],
    [inGroup, sel]
  );

  const chart = useMemo(() => {
    if (!comp || comp.graph.length === 0) return null;
    const g = comp.graph;
    const fracs = g.map((p) => dateToFrac(p.date));
    const xMin = fracs[0];
    const xMax = fracs[fracs.length - 1];
    const toX = (f: number) => ML + ((f - xMin) / (xMax - xMin || 1)) * CW;

    const maxRank = Math.max(10, ...g.map((p) => p.rank));
    const yMax = Math.log10(maxRank);
    const toY = (rank: number) => MT + (Math.log10(Math.max(rank, 1)) / yMax) * CH;

    const polyline = g
      .map((p, i) => `${toX(fracs[i]).toFixed(1)},${toY(p.rank).toFixed(1)}`)
      .join(' ');

    // Power-of-ten y ticks within range (rank 1 at top).
    const yTicks: number[] = [];
    for (let e = 0; Math.pow(10, e) <= maxRank * 1.0001; e++) yTicks.push(Math.pow(10, e));

    // ~5-year x ticks.
    const yearTicks: number[] = [];
    for (let y = Math.ceil(xMin / 5) * 5; y <= Math.floor(xMax / 5) * 5; y += 5)
      yearTicks.push(y);

    return { g, fracs, xMin, xMax, toX, toY, polyline, yTicks, yearTicks };
  }, [comp]);

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data || !comp) return <div className="loading">Loading…</div>;

  const switchGroup = (gid: string) => {
    setGroup(gid);
    const first = data.comparisons.find((c) => c.group === gid);
    if (first) setSel(first.id);
    setHoverIdx(null);
  };

  function getHoverIdx(e: React.MouseEvent<SVGSVGElement>): number | null {
    if (!chart) return null;
    const rect = svgRef.current!.getBoundingClientRect();
    const svgX = ((e.clientX - rect.left) / rect.width) * VW;
    if (svgX < ML || svgX > ML + CW) return null;
    const frac = chart.xMin + ((svgX - ML) / CW) * (chart.xMax - chart.xMin);
    let lo = 0,
      hi = chart.fracs.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (chart.fracs[mid] < frac) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0 && Math.abs(chart.fracs[lo - 1] - frac) < Math.abs(chart.fracs[lo] - frac))
      lo--;
    return lo;
  }

  const hp = chart && hoverIdx !== null ? chart.g[hoverIdx] : null;
  const hx = hp && chart && hoverIdx !== null ? chart.toX(chart.fracs[hoverIdx]) : null;
  const tooltipLeft = hx !== null && hx > ML + CW * 0.6;
  const tooltipX = hx !== null ? (tooltipLeft ? hx - 182 : hx + 10) : 0;
  const tooltipY =
    hp && chart ? Math.max(MT + 2, Math.min(chart.toY(hp.rank) - 40, MT + CH - 78)) : 0;

  return (
    <div>
      <div className="tabs">
        {GROUPS.filter((gr) => data.comparisons.some((c) => c.group === gr.id)).map((gr) => (
          <button
            key={gr.id}
            className={gr.id === group ? 'tab active' : 'tab'}
            onClick={() => switchGroup(gr.id)}
          >
            {gr.label}
          </button>
        ))}
      </div>

      <div className="tabs">
        {inGroup.map((c) => (
          <button
            key={c.id}
            className={c.id === comp.id ? 'tab active' : 'tab'}
            onClick={() => {
              setSel(c.id);
              setHoverIdx(null);
            }}
            title={c.title}
          >
            {tabLabel(c)}
          </button>
        ))}
      </div>

      <p className="desc" style={{ marginTop: 8 }}>
        {comp.title}. The graph plots the rank over time of the then-current world record{' '}
        {comp.subject_type} for {EVENT_NAMES[comp.subject_event] ?? comp.subject_event} if it were
        placed in the {comp.ref_type} rankings of{' '}
        {EVENT_NAMES[comp.ref_event] ?? comp.ref_event}. The table lists every historical world
        record and the rank it would have held the week it was set (it can only fall from there).
      </p>

      {chart && (
        <svg
          ref={svgRef}
          viewBox={`0 0 ${VW} ${VH}`}
          style={{ width: '100%', maxWidth: VW, display: 'block' }}
          onMouseMove={(e) => setHoverIdx(getHoverIdx(e))}
          onMouseLeave={() => setHoverIdx(null)}
        >
          {chart.yTicks.map((rk) => {
            const y = chart.toY(rk);
            return (
              <g key={rk}>
                <line
                  x1={ML}
                  y1={y}
                  x2={ML + CW}
                  y2={y}
                  stroke="#2a2a2a"
                  strokeWidth={1}
                  strokeDasharray="3 4"
                />
                <text
                  x={ML - 6}
                  y={y}
                  textAnchor="end"
                  dominantBaseline="middle"
                  fontSize="11"
                  fill="#666"
                >
                  {rk >= 1000 ? `${rk / 1000}k` : rk}
                </text>
              </g>
            );
          })}

          {chart.yearTicks.map((y) => (
            <g key={y}>
              <line
                x1={chart.toX(y)}
                y1={MT}
                x2={chart.toX(y)}
                y2={MT + CH}
                stroke="#2a2a2a"
                strokeWidth="1"
                strokeDasharray="3 4"
              />
              <text x={chart.toX(y)} y={MT + CH + 14} textAnchor="middle" fontSize="10" fill="#666">
                {y}
              </text>
            </g>
          ))}

          <rect x={ML} y={MT} width={CW} height={CH} fill="none" stroke="#444" strokeWidth="1" />

          <polyline
            points={chart.polyline}
            fill="none"
            stroke={LINE_COLOR}
            strokeWidth="1.5"
            strokeLinejoin="round"
          />

          {hp && hx !== null && (
            <>
              <line
                x1={hx}
                y1={MT}
                x2={hx}
                y2={MT + CH}
                stroke="#666"
                strokeWidth="1"
                strokeDasharray="4 3"
              />
              <circle cx={hx} cy={chart.toY(hp.rank)} r="4" fill={LINE_COLOR} />
              <rect
                x={tooltipX}
                y={tooltipY}
                width={172}
                height={70}
                rx="4"
                fill="#111"
                stroke="#444"
                strokeWidth="1"
              />
              <text x={tooltipX + 86} y={tooltipY + 15} textAnchor="middle" fontSize="11" fill="#aaa">
                {hp.date}
              </text>
              <text
                x={tooltipX + 86}
                y={tooltipY + 34}
                textAnchor="middle"
                fontSize="14"
                fill={LINE_COLOR}
                fontWeight="600"
              >
                rank #{hp.rank.toLocaleString()}
              </text>
              <text x={tooltipX + 86} y={tooltipY + 51} textAnchor="middle" fontSize="10" fill="#777">
                {fmt(hp.value, comp.subject_event, comp.subject_type)} · of{' '}
                {hp.pool.toLocaleString()}
              </text>
            </>
          )}

          <text x={ML + CW / 2} y={VH - 4} textAnchor="middle" fontSize="12" fill="#777">
            Year
          </text>
          <text
            x={-(MT + CH / 2)}
            y={14}
            textAnchor="middle"
            fontSize="12"
            fill="#777"
            transform="rotate(-90)"
          >
            Rank if ranked as {comp.ref_type} (log, best on top)
          </text>
        </svg>
      )}

      <table className="ranking-table" style={{ tableLayout: 'fixed', width: '100%', marginTop: 16 }}>
        <thead>
          <tr>
            <th style={{ width: 36 }}>#</th>
            <th>Record holder</th>
            <th style={{ width: 90 }}>Result</th>
            <th style={{ width: 96 }}>Date</th>
            <th style={{ width: 150 }}>Rank as {comp.ref_type}</th>
          </tr>
        </thead>
        <tbody>
          {comp.table.map((r, i) => ({ r, n: comp.table.length - i })).reverse().map(({ r, n }) => (
            <tr key={n}>
              <td className="rank-col">{n}</td>
              <td>
                <a
                  href={`https://www.worldcubeassociation.org/persons/${r.pid}`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {r.name}
                </a>
              </td>
              <td className="value-col">{fmt(r.value, comp.subject_event, comp.subject_type)}</td>
              <td className="muted">{r.date}</td>
              <td>
                #{r.rank.toLocaleString()}{' '}
                <span className="muted">of {r.pool.toLocaleString()}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
