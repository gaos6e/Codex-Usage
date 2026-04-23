import type { TokenBreakdown } from './contracts';
import type { ResolvedRange } from './timeRange';

const DAY_MS = 24 * 60 * 60 * 1000;

export function calendarDaysInRange(range: ResolvedRange): number {
  return Math.max(1, Math.ceil((range.end.getTime() - range.start.getTime()) / DAY_MS));
}

export function averagePerDay(total: number, calendarDays: number): number {
  return total / Math.max(1, calendarDays);
}

export function cacheHitRateForBreakdown(breakdown: TokenBreakdown | undefined): number | undefined {
  const input = breakdown?.input || 0;
  if (input <= 0) {
    return undefined;
  }
  return (breakdown?.cached || 0) / input;
}
