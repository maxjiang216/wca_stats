/** Format a WCA result value for display. */
export function formatValue(value: number, eventId: string): string {
  if (value < 0) return 'DNF';
  if (value === 0) return '—';

  if (eventId === '333fm') {
    // Individual FMC attempts are raw move counts.
    return `${value} moves`;
  }

  if (eventId === '333mbf') {
    // Packed: 99 - difference encodes points, see WCA export README.
    // Not currently a stat we compute, but format defensively.
    return String(value);
  }

  // Standard timed events: centiseconds.
  return formatCentiseconds(value);
}

/** Format an `average` column value (FMC averages are moves×100). */
export function formatAverage(value: number, eventId: string): string {
  if (value < 0) return 'DNF';
  if (value === 0) return '—';

  if (eventId === '333fm') {
    // Average column is moves * 100 (centimoves).
    return (value / 100).toFixed(2);
  }

  return formatCentiseconds(value);
}

/** Format a `best` (single) column value. */
export function formatSingle(value: number, eventId: string): string {
  if (value < 0) return 'DNF';
  if (value === 0) return '—';

  if (eventId === '333fm') {
    // Single column for FMC is raw moves.
    return `${value} moves`;
  }

  return formatCentiseconds(value);
}

/** Format an MBLD time (seconds) as H:MM:SS or M:SS. */
export function formatMbldTime(time_s: number): string {
  const h = Math.floor(time_s / 3600);
  const m = Math.floor((time_s % 3600) / 60);
  const s = time_s % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  if (h > 0) return `${h}:${pad(m)}:${pad(s)}`;
  return `${m}:${pad(s)}`;
}

/** Format MBLD mean points from a 3-attempt sum (e.g. 169 → "56.33"). */
export function formatMbldMean(points_sum: number): string {
  return (points_sum / 3).toFixed(2);
}

/** Decode and format an MBLD single encoded value as "solved/attempted time". */
export function formatMbldSingle(value: number): string {
  if (value <= 0) return '—';
  const points = 99 - Math.floor(value / 10_000_000);
  const time_s = Math.floor(value / 100) % 100_000;
  const missed = value % 100;
  const solved = points + missed;
  const attempted = solved + missed;
  return `${solved}/${attempted} ${formatMbldTime(time_s)}`;
}

function formatCentiseconds(cs: number): string {
  const totalSec = cs / 100;
  if (totalSec >= 3600) {
    const h = Math.floor(totalSec / 3600);
    const m = Math.floor((totalSec % 3600) / 60);
    const s = totalSec % 60;
    return `${h}:${String(m).padStart(2, '0')}:${s.toFixed(2).padStart(5, '0')}`;
  }
  if (totalSec >= 60) {
    const min = Math.floor(totalSec / 60);
    const sec = totalSec - min * 60;
    return `${min}:${sec.toFixed(2).padStart(5, '0')}`;
  }
  return totalSec.toFixed(2);
}
