'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { STATS } from '@/lib/stats';

const CHARTS = [
  { href: '/two-man', label: '2-Man Guildford' },
  { href: '/nations-cup', label: 'Nations Cup Dream Team' },
  { href: '/sub-x', label: 'Sub-X Rankings' },
  { href: '/wr-half-life', label: 'WR Half-Life' },
  { href: '/wr-longevity', label: 'WR Longevity' },
  { href: '/wr-cross-rank', label: 'WR Cross-Rank' },
  { href: '/wr-compare', label: 'WR Compare: 3×3 vs SQ1' },
  { href: '/china', label: 'China vs USA by event' },
  { href: '/first-records', label: 'First CR/WR by Country' },
];

const MBLD_EXTRA = [
  { href: '/mbld-rankings', label: 'MBLD N-Point Rankings' },
];

const ALL_ROUND = [
  { href: '/sum-of-ranks', label: 'Sum of Ranks' },
  { href: '/kinch-ranks', label: 'KinchRanks' },
  { href: '/skill-estimator', label: 'Skill Estimator' },
  { href: '/kalman-skill', label: 'Kalman Skill' },
  { href: '/luck', label: 'Luckiest Averages' },
];

export default function Sidebar() {
  const pathname = usePathname();

  const ao5   = STATS.filter((s) => s.group === 'ao5');
  const mo3   = STATS.filter((s) => s.group === 'mo3');
  const mbld  = STATS.filter((s) => s.group === 'mbld');
  const relay = STATS.filter((s) => s.group === 'relay');

  return (
    <aside className="sidebar">
      <h1>WCA Stats</h1>
      <div className="subtitle">Derived statistics from WCA results</div>

      <Link href="/" className={pathname === '/' ? 'active' : ''}>
        Home
      </Link>

      <h2>Average of 5</h2>
      {ao5.map((s) => (
        <Link
          key={s.id}
          href={`/stats/${s.id}`}
          className={pathname === `/stats/${s.id}` ? 'active' : ''}
        >
          {s.title.replace(' (ao5)', '')}
        </Link>
      ))}

      <h2>Mean of 3</h2>
      {mo3.map((s) => (
        <Link
          key={s.id}
          href={`/stats/${s.id}`}
          className={pathname === `/stats/${s.id}` ? 'active' : ''}
        >
          {s.title.replace(' (mo3)', '')}
        </Link>
      ))}

      <h2>Multi-Blind</h2>
      {mbld.map((s) => (
        <Link
          key={s.id}
          href={`/stats/${s.id}`}
          className={pathname === `/stats/${s.id}` ? 'active' : ''}
        >
          {s.title}
        </Link>
      ))}

      <h2>Relays &amp; Challenges</h2>
      {relay.map((s) => (
        <Link
          key={s.id}
          href={`/stats/${s.id}`}
          className={pathname === `/stats/${s.id}` ? 'active' : ''}
        >
          {s.title}
        </Link>
      ))}

      <h2>Multi-Blind</h2>
      {MBLD_EXTRA.map((c) => (
        <Link
          key={c.href}
          href={c.href}
          className={pathname === c.href ? 'active' : ''}
        >
          {c.label}
        </Link>
      ))}

      <h2>All-Round Rankings</h2>
      {ALL_ROUND.map((c) => (
        <Link
          key={c.href}
          href={c.href}
          className={pathname === c.href ? 'active' : ''}
        >
          {c.label}
        </Link>
      ))}

      <h2>Charts</h2>
      {CHARTS.map((c) => (
        <Link
          key={c.href}
          href={c.href}
          className={pathname === c.href ? 'active' : ''}
        >
          {c.label}
        </Link>
      ))}
    </aside>
  );
}
