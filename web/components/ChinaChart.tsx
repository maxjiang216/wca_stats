'use client';

import { useRef, useState, useMemo } from 'react';

export type ChartPoint = { rank: number; china: number; usa: number };

type Props = {
  total: number;
  points: ChartPoint[];
};

const VW = 860, VH = 420;
const ML = 55, MR = 22, MT = 30, MB = 46;
const CW = VW - ML - MR;
const CH = VH - MT - MB;
const Y_MAX = 100;

function toX(rank: number, maxRank: number) {
  if (maxRank <= 1) return ML;
  return ML + (Math.log(rank) / Math.log(maxRank)) * CW;
}

function toY(pct: number) {
  return MT + CH * (1 - pct / Y_MAX);
}

function xToRank(svgX: number, maxRank: number): number {
  const t = Math.max(0, Math.min(1, (svgX - ML) / CW));
  return Math.max(1, Math.round(Math.pow(maxRank, t)));
}

function fmtRank(rank: number): string {
  if (rank >= 100_000) return `${Math.round(rank / 1000)}k`;
  if (rank >= 10_000)  return `${Math.round(rank / 1000)}k`;
  if (rank >= 1_000)   return `${(rank / 1000).toFixed(1)}k`.replace('.0k', 'k');
  return String(rank);
}

const X_TICKS = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000,
                 10000, 20000, 50000, 100000, 200000];
const Y_TICKS = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100];

const CHINA_COLOR = '#dc2626';
const USA_COLOR   = '#2563eb';

export default function ChinaChart({ total, points }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);

  const { chinaPolyline, usaPolyline, chinaArea, usaArea } = useMemo(() => {
    const bottom = MT + CH;
    const chinaStr = points.map(p =>
      `${toX(p.rank, total).toFixed(2)},${toY(p.china).toFixed(2)}`).join(' ');
    const usaStr   = points.map(p =>
      `${toX(p.rank, total).toFixed(2)},${toY(p.usa).toFixed(2)}`).join(' ');

    function makeArea(key: 'china' | 'usa') {
      const first = points[0], last = points[points.length - 1];
      return [
        `M ${toX(first.rank, total).toFixed(2)},${bottom}`,
        ...points.map(p => `L ${toX(p.rank, total).toFixed(2)},${toY(p[key]).toFixed(2)}`),
        `L ${toX(last.rank, total).toFixed(2)},${bottom}`,
        'Z',
      ].join(' ');
    }

    return {
      chinaPolyline: chinaStr,
      usaPolyline:   usaStr,
      chinaArea:     makeArea('china'),
      usaArea:       makeArea('usa'),
    };
  }, [points, total]);

  function getHoverIdx(e: React.MouseEvent<SVGSVGElement>): number | null {
    const svg = svgRef.current!;
    const rect = svg.getBoundingClientRect();
    const svgX = ((e.clientX - rect.left) / rect.width) * VW;
    if (svgX < ML || svgX > ML + CW) return null;
    const rank = xToRank(svgX, total);
    let lo = 0, hi = points.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (points[mid].rank < rank) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0 && Math.abs(points[lo - 1].rank - rank) < Math.abs(points[lo].rank - rank)) lo--;
    return lo;
  }

  function handleMouseMove(e: React.MouseEvent<SVGSVGElement>) {
    setHoverIdx(getHoverIdx(e));
  }

  const hp = hoverIdx !== null ? points[hoverIdx] : null;
  const hx = hp ? toX(hp.rank, total) : null;
  const tooltipLeft = hx !== null && hx > ML + CW * 0.65;
  const tooltipX = hx !== null ? (tooltipLeft ? hx - 158 : hx + 10) : 0;
  const tooltipY = hp ? Math.min(toY(Math.max(hp.china, hp.usa)) - 10, MT + CH - 75) : 0;

  return (
    <>
      {/* Legend */}
      <div style={{ display: 'flex', gap: '1.5rem', marginBottom: '0.5rem', fontSize: '0.85rem' }}>
        <span style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          <svg width="28" height="4"><rect width="28" height="3" y="0.5" fill={CHINA_COLOR} rx="1.5" /></svg>
          China
        </span>
        <span style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          <svg width="28" height="4"><rect width="28" height="3" y="0.5" fill={USA_COLOR} rx="1.5" /></svg>
          USA
        </span>
      </div>

      <svg
        ref={svgRef}
        viewBox={`0 0 ${VW} ${VH}`}
        style={{ width: '100%', maxWidth: VW, display: 'block' }}
        onMouseMove={handleMouseMove}
        onMouseLeave={() => setHoverIdx(null)}
      >
        {/* Y grid + labels */}
        {Y_TICKS.map(t => (
          <g key={t}>
            <line x1={ML} y1={toY(t)} x2={ML + CW} y2={toY(t)}
              stroke={t === 0 ? '#bbb' : '#e8e8e8'} strokeWidth="1" />
            <text x={ML - 6} y={toY(t)} textAnchor="end" dominantBaseline="middle"
              fontSize="11" fill="#888">{t}%</text>
          </g>
        ))}

        {/* X grid + labels */}
        {X_TICKS.filter(t => t <= total).map(t => (
          <g key={t}>
            <line x1={toX(t, total)} y1={MT} x2={toX(t, total)} y2={MT + CH}
              stroke="#e8e8e8" strokeWidth="1" />
            <text x={toX(t, total)} y={MT + CH + 14} textAnchor="middle"
              fontSize="10" fill="#888">{fmtRank(t)}</text>
          </g>
        ))}

        {/* Border */}
        <rect x={ML} y={MT} width={CW} height={CH} fill="none" stroke="#ccc" strokeWidth="1" />

        {/* Area fills */}
        <path d={usaArea}   fill={`${USA_COLOR}18`} />
        <path d={chinaArea} fill={`${CHINA_COLOR}18`} />

        {/* Lines */}
        <polyline points={usaPolyline}   fill="none" stroke={USA_COLOR}   strokeWidth="2" strokeLinejoin="round" />
        <polyline points={chinaPolyline} fill="none" stroke={CHINA_COLOR} strokeWidth="2" strokeLinejoin="round" />

        {/* Hover crosshair */}
        {hp && hx !== null && (
          <>
            <line x1={hx} y1={MT} x2={hx} y2={MT + CH}
              stroke="#555" strokeWidth="1" strokeDasharray="4 3" />
            <circle cx={hx} cy={toY(hp.china)} r="4" fill={CHINA_COLOR} />
            <circle cx={hx} cy={toY(hp.usa)}   r="4" fill={USA_COLOR}   />

            {/* Tooltip */}
            <rect x={tooltipX} y={tooltipY} width={148} height={66}
              rx="4" fill="white" stroke="#ddd" strokeWidth="1" />
            <text x={tooltipX + 74} y={tooltipY + 15}
              textAnchor="middle" fontSize="11" fill="#555">
              Top {hp.rank.toLocaleString()}
            </text>
            <rect x={tooltipX + 10} y={tooltipY + 24} width="10" height="10"
              rx="2" fill={CHINA_COLOR} />
            <text x={tooltipX + 26} y={tooltipY + 33} fontSize="12" fill={CHINA_COLOR} fontWeight="600">
              {hp.china.toFixed(1)}% China
            </text>
            <rect x={tooltipX + 10} y={tooltipY + 42} width="10" height="10"
              rx="2" fill={USA_COLOR} />
            <text x={tooltipX + 26} y={tooltipY + 51} fontSize="12" fill={USA_COLOR} fontWeight="600">
              {hp.usa.toFixed(1)}% USA
            </text>
          </>
        )}

        {/* Axis labels */}
        <text x={ML + CW / 2} y={VH - 4} textAnchor="middle" fontSize="12" fill="#666">
          Rank (log scale)
        </text>
        <text x={-(MT + CH / 2)} y={13} textAnchor="middle" fontSize="12" fill="#666"
          transform="rotate(-90)">
          % of top N
        </text>
      </svg>
    </>
  );
}
