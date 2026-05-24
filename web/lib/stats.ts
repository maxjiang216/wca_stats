export type StatGroup = 'ao5' | 'mo3' | 'mbld' | 'relay';

export type StatDef = {
  id: string;
  title: string;
  description: string;
  group: StatGroup;
  /** Ordered event IDs for relay / challenge stats. */
  relayEvents?: string[];
};

export const STATS: StatDef[] = [
  {
    id: 'ao5_2nd',
    title: 'Best 2nd solve (ao5)',
    description: 'Fastest counting solve in an average of 5. The best solve is dropped; this is the next-fastest.',
    group: 'ao5',
  },
  {
    id: 'ao5_3rd',
    title: 'Best 3rd solve (ao5)',
    description: 'Fastest middle (median) solve in an average of 5.',
    group: 'ao5',
  },
  {
    id: 'ao5_4th',
    title: 'Best 4th solve (ao5)',
    description: 'Fastest worst-counting solve — the slowest of the 3 solves that count.',
    group: 'ao5',
  },
  {
    id: 'ao5_5th',
    title: 'Best 5th solve (ao5)',
    description: 'Best "worst solve" — the fastest dropped-worst across all ao5 attempts.',
    group: 'ao5',
  },
  {
    id: 'mo3_2nd',
    title: 'Best 2nd solve (mo3)',
    description: 'Fastest middle solve in a mean of 3.',
    group: 'mo3',
  },
  {
    id: 'mo3_3rd',
    title: 'Best 3rd solve (mo3)',
    description: 'Fastest worst solve in a mean of 3.',
    group: 'mo3',
  },
  {
    id: 'mbld_mean',
    title: 'MBLD Mean',
    description:
      'Mean points across 3 valid MBLD attempts in a Bo3 round. All three attempts must be non-DNF/DNS. Tiebreak by total time.',
    group: 'mbld',
  },
  {
    id: 'mbld_perfect',
    title: 'MBLD Perfect (x/x)',
    description:
      'Best single MBLD attempt where all cubes were solved (x/x, zero misses). Ranked by solved count, tiebreak by time.',
    group: 'mbld',
  },
  {
    id: 'mbld_solved',
    title: 'MBLD Most Solved',
    description:
      'Best single MBLD attempt ranked by cubes solved only — no penalty for unsolved cubes. Tiebreak by time.',
    group: 'mbld',
  },
  // ── Relays & Challenges ──────────────────────────────────────────────────
  {
    id: 'relay_2_4',
    title: '2–4 Relay',
    description: 'Sum of personal best ao5 averages for 2×2, 3×3, and 4×4.',
    group: 'relay',
    relayEvents: ['222', '333', '444'],
  },
  {
    id: 'relay_2_5',
    title: '2–5 Relay',
    description: 'Sum of personal best ao5 averages for 2×2 through 5×5.',
    group: 'relay',
    relayEvents: ['222', '333', '444', '555'],
  },
  {
    id: 'relay_2_6',
    title: '2–6 Relay',
    description: 'Sum of personal best ao5/mo3 averages for 2×2 through 6×6.',
    group: 'relay',
    relayEvents: ['222', '333', '444', '555', '666'],
  },
  {
    id: 'relay_2_7',
    title: '2–7 Relay',
    description: 'Sum of personal best ao5/mo3 averages for 2×2 through 7×7.',
    group: 'relay',
    relayEvents: ['222', '333', '444', '555', '666', '777'],
  },
  {
    id: 'mini_guildford',
    title: 'Mini Guildford',
    description:
      'Sum of PB averages for the 2–5 relay (2×2–5×5) plus Clock, Mega, Skewb, SQ-1, Pyra, and 3OH.',
    group: 'relay',
    relayEvents: ['222', '333', '444', '555', 'clock', 'minx', 'skewb', 'sq1', 'pyram', '333oh'],
  },
  {
    id: 'guildford',
    title: 'Guildford Challenge',
    description: 'Mini Guildford plus 6×6 and 7×7 — 12 events total.',
    group: 'relay',
    relayEvents: ['222', '333', '444', '555', 'clock', 'minx', 'skewb', 'sq1', 'pyram', '333oh', '666', '777'],
  },
];

export function getStat(id: string): StatDef | undefined {
  return STATS.find((s) => s.id === id);
}

export type RankingEntry = {
  rank: number;
  person_id: string;
  person_name: string;
  country_id: string;
  competition_id: string;
  value_cs: number;
  single_cs: number;
  average_cs: number;
};

export type StatData = Record<string, RankingEntry[]>;

/** Entry for mbld_perfect and mbld_solved. */
export type MbldSingleEntry = {
  rank: number;
  person_id: string;
  person_name: string;
  country_id: string;
  competition_id: string;
  solved: number;
  attempted: number;
  time_s: number;
};

/** Entry for mbld_mean. */
export type MbldMeanEntry = {
  rank: number;
  person_id: string;
  person_name: string;
  country_id: string;
  competition_id: string;
  points_sum: number;
  time_total_s: number;
};

export type MbldSingleData = Record<string, MbldSingleEntry[]>;
export type MbldMeanData = Record<string, MbldMeanEntry[]>;

/** Entry for relay / challenge stats (relay_2_4, guildford, etc.). */
export type RelayEntry = {
  rank: number;
  person_id: string;
  person_name: string;
  country_id: string;
  total_cs: number;
  event_avgs: number[];
};

export type PersonRankData = {
  id: string;
  n: string;
  c: string;
  /** Single world rank per event (EVENTS order). 0 = not done. */
  sr: number[];
  /** Average world rank per event. 0 = not done or no avg ranking. */
  ar: number[];
  /** Single PB value per event. 0 = not done. */
  ps: number[];
  /** Average PB value per event. 0 = not done or no avg ranking. */
  pa: number[];
};

export type AllRanksData = {
  events: string[];
  total_s: number[];
  total_a: number[];
  wr_s: number[];
  wr_a: number[];
  persons: PersonRankData[];
};

export const EVENT_NAMES: Record<string, string> = {
  '222': '2x2x2 Cube',
  '333': '3x3x3 Cube',
  '333bf': '3x3x3 Blindfolded',
  '333fm': '3x3x3 Fewest Moves',
  '333ft': '3x3x3 With Feet',
  '333mbf': '3x3x3 Multi-Blind',
  '333mbo': '3x3x3 Multi-Blind (Old)',
  '333oh': '3x3x3 One-Handed',
  '444': '4x4x4 Cube',
  '444bf': '4x4x4 Blindfolded',
  '555': '5x5x5 Cube',
  '555bf': '5x5x5 Blindfolded',
  '666': '6x6x6 Cube',
  '777': '7x7x7 Cube',
  clock: 'Clock',
  magic: 'Magic',
  mmagic: 'Master Magic',
  minx: 'Megaminx',
  pyram: 'Pyraminx',
  skewb: 'Skewb',
  sq1: 'Square-1',
};

// Stable ordering for event tabs (mirrors WCA's display order).
export const EVENT_ORDER: string[] = [
  '333', '222', '444', '555', '666', '777',
  '333bf', '333fm', '333oh', 'clock', 'minx', 'pyram', 'skewb', 'sq1',
  '444bf', '555bf', '333mbf', '333mbo', '333ft', 'magic', 'mmagic',
];

export function sortEvents(events: string[]): string[] {
  const order = new Map(EVENT_ORDER.map((e, i) => [e, i]));
  return [...events].sort((a, b) => (order.get(a) ?? 999) - (order.get(b) ?? 999));
}
