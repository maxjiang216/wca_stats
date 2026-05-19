'use client';

import { useEffect, useRef, useState, useMemo } from 'react';

type DataPoint = { date: string; half_life: number; n_wr: number };

const VW = 860, VH = 440;
const ML = 54, MR = 16, MT = 24, MB = 44;
const CW = VW - ML - MR;
const CH = VH - MT - MB;

// Log-scale Y: 7 days to 10000 days.
const Y_MIN_LOG = Math.log10(7);
const Y_MAX_LOG = Math.log10(10000);

function toY(days: number): number {
  const t = (Math.log10(Math.max(days, 7)) - Y_MIN_LOG) / (Y_MAX_LOG - Y_MIN_LOG);
  return MT + CH * (1 - t);
}

// Parse "YYYY-MM-DD" to a fractional year for x positioning.
function dateToFrac(date: string): number {
  const y = +date.slice(0, 4);
  const m = +date.slice(5, 7);
  const d = +date.slice(8, 10);
  return y + (m - 1) / 12 + (d - 1) / 365.25;
}

function fmtDays(d: number): string {
  if (d < 14)   return `${Math.round(d)} day${d === 1 ? '' : 's'}`;
  if (d < 60)   return `${(d / 7).toFixed(1)} weeks`;
  if (d < 730)  return `${(d / 30.44).toFixed(1)} months`;
  return `${(d / 365.25).toFixed(1)} years`;
}

const Y_TICKS: { days: number; label: string }[] = [
  { days: 7,    label: '1w'  },
  { days: 30,   label: '1mo' },
  { days: 90,   label: '3mo' },
  { days: 180,  label: '6mo' },
  { days: 365,  label: '1yr' },
  { days: 730,  label: '2yr' },
  { days: 1825, label: '5yr' },
  { days: 3650, label: '10yr'},
  { days: 7300, label: '20yr'},
];

const LINE_COLOR = '#60a5fa';
const ANNO_COLOR = '#94a3b8';

export default function WrHalfLifeChart() {
  const svgRef = useRef<SVGSVGElement>(null);
  const [data, setData] = useState<DataPoint[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);

  useEffect(() => {
    fetch('/data/wr_half_life.json')
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch(e => setError(String(e)));
  }, []);

  const computed = useMemo(() => {
    if (!data || data.length === 0) return null;
    const fracs = data.map(p => dateToFrac(p.date));
    const xMin = fracs[0];
    const xMax = fracs[fracs.length - 1];

    const toX = (frac: number) => ML + ((frac - xMin) / (xMax - xMin)) * CW;

    const polyline = data
      .map((p, i) => `${toX(fracs[i]).toFixed(1)},${toY(p.half_life).toFixed(1)}`)
      .join(' ');

    // Year ticks — every 5 years for readability over the ~43-year span.
    const startYear = Math.ceil(xMin / 5) * 5;
    const endYear = Math.floor(xMax / 5) * 5;
    const yearTicks: number[] = [];
    for (let y = startYear; y <= endYear; y += 5) yearTicks.push(y);

    // Annotations: WCA modern era (2003), COVID (2020).
    const annos = [
      { frac: 2003, label: 'WCA era' },
      { frac: 2020, label: 'COVID' },
    ];

    return { fracs, xMin, xMax, toX, polyline, yearTicks, annos };
  }, [data]);

  function getHoverIdx(e: React.MouseEvent<SVGSVGElement>): number | null {
    if (!computed || !data) return null;
    const svg = svgRef.current!;
    const rect = svg.getBoundingClientRect();
    const svgX = ((e.clientX - rect.left) / rect.width) * VW;
    if (svgX < ML || svgX > ML + CW) return null;
    const frac = computed.xMin + ((svgX - ML) / CW) * (computed.xMax - computed.xMin);
    let lo = 0, hi = computed.fracs.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (computed.fracs[mid] < frac) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0 &&
        Math.abs(computed.fracs[lo - 1] - frac) < Math.abs(computed.fracs[lo] - frac)) {
      lo--;
    }
    return lo;
  }

  if (error) return <div className="empty">Failed to load: {error}</div>;
  if (!data || !computed) return <div className="loading">Loading…</div>;

  const hp = hoverIdx !== null ? data[hoverIdx] : null;
  const hx = hp !== null && hoverIdx !== null ? computed.toX(computed.fracs[hoverIdx]) : null;
  const tooltipLeft = hx !== null && hx > ML + CW * 0.6;
  const tooltipX = hx !== null ? (tooltipLeft ? hx - 172 : hx + 10) : 0;
  const tooltipY = hp ? Math.max(MT + 2, Math.min(toY(hp.half_life) - 35, MT + CH - 70)) : 0;

  return (
    <svg
      ref={svgRef}
      viewBox={`0 0 ${VW} ${VH}`}
      style={{ width: '100%', maxWidth: VW, display: 'block' }}
      onMouseMove={e => setHoverIdx(getHoverIdx(e))}
      onMouseLeave={() => setHoverIdx(null)}
    >
      {/* Y grid + labels */}
      {Y_TICKS.map(({ days, label }) => {
        const y = toY(days);
        const is1yr = days === 365;
        return (
          <g key={label}>
            <line x1={ML} y1={y} x2={ML + CW} y2={y}
              stroke={is1yr ? '#555' : '#2a2a2a'}
              strokeWidth={is1yr ? 1.5 : 1}
              strokeDasharray={is1yr ? undefined : '3 4'} />
            <text x={ML - 6} y={y} textAnchor="end" dominantBaseline="middle"
              fontSize="11" fill={is1yr ? '#aaa' : '#666'}>{label}</text>
          </g>
        );
      })}

      {/* X grid + year labels */}
      {computed.yearTicks.map(y => (
        <g key={y}>
          <line x1={computed.toX(y)} y1={MT} x2={computed.toX(y)} y2={MT + CH}
            stroke="#2a2a2a" strokeWidth="1" strokeDasharray="3 4" />
          <text x={computed.toX(y)} y={MT + CH + 14} textAnchor="middle"
            fontSize="10" fill="#666">{y}</text>
        </g>
      ))}

      {/* Annotation verticals */}
      {computed.annos.map(({ frac, label }) => {
        const ax = computed.toX(frac);
        return (
          <g key={label}>
            <line x1={ax} y1={MT} x2={ax} y2={MT + CH}
              stroke={ANNO_COLOR} strokeWidth="1" strokeDasharray="5 3" opacity="0.5" />
            <text x={ax + 4} y={MT + 12} fontSize="10" fill={ANNO_COLOR} opacity="0.8">{label}</text>
          </g>
        );
      })}

      {/* Border */}
      <rect x={ML} y={MT} width={CW} height={CH} fill="none" stroke="#444" strokeWidth="1" />

      {/* Half-life line */}
      <polyline
        points={computed.polyline}
        fill="none"
        stroke={LINE_COLOR}
        strokeWidth="1.5"
        strokeLinejoin="round"
      />

      {/* Hover crosshair */}
      {hp && hx !== null && (
        <>
          <line x1={hx} y1={MT} x2={hx} y2={MT + CH}
            stroke="#666" strokeWidth="1" strokeDasharray="4 3" />
          <circle cx={hx} cy={toY(hp.half_life)} r="4" fill={LINE_COLOR} />

          <rect x={tooltipX} y={tooltipY} width={162} height={62}
            rx="4" fill="#111" stroke="#444" strokeWidth="1" />
          <text x={tooltipX + 81} y={tooltipY + 15}
            textAnchor="middle" fontSize="11" fill="#aaa">{hp.date}</text>
          <text x={tooltipX + 81} y={tooltipY + 34}
            textAnchor="middle" fontSize="14" fill={LINE_COLOR} fontWeight="600">
            {fmtDays(hp.half_life)}
          </text>
          <text x={tooltipX + 81} y={tooltipY + 50}
            textAnchor="middle" fontSize="10" fill="#666">
            {hp.n_wr} WR{hp.n_wr === 1 ? '' : 's'} tracked
          </text>
        </>
      )}

      {/* Axis labels */}
      <text x={ML + CW / 2} y={VH - 4} textAnchor="middle" fontSize="12" fill="#777">Year</text>
      <text x={-(MT + CH / 2)} y={13} textAnchor="middle" fontSize="12" fill="#777"
        transform="rotate(-90)">Half-life (log scale)</text>
    </svg>
  );
}
