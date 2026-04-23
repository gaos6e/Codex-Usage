import { describe, expect, it } from 'vitest';
import type { ResolvedRange } from '../../src/shared/timeRange';
import { averagePerDay, cacheHitRateForBreakdown, calendarDaysInRange } from '../../src/shared/usageMath';

function range(start: Date, end: Date): ResolvedRange {
  return {
    start,
    end,
    label: 'Test range',
    aggregation: 'daily',
  };
}

describe('usage math', () => {
  it('counts today as one calendar day', () => {
    const target = range(new Date(2026, 3, 22), new Date(2026, 3, 23));
    expect(calendarDaysInRange(target)).toBe(1);
    expect(averagePerDay(120, calendarDaysInRange(target))).toBe(120);
  });

  it('uses natural calendar days for rolling and custom ranges', () => {
    const last7 = range(new Date(2026, 3, 16), new Date(2026, 3, 23));
    const custom = range(new Date(2026, 3, 1), new Date(2026, 3, 11));
    expect(calendarDaysInRange(last7)).toBe(7);
    expect(averagePerDay(700, calendarDaysInRange(last7))).toBe(100);
    expect(calendarDaysInRange(custom)).toBe(10);
  });

  it('returns Token cache hit rate only when input Tokens are available', () => {
    expect(cacheHitRateForBreakdown({ input: 100, cached: 25 })).toBe(0.25);
    expect(cacheHitRateForBreakdown({ input: 100, cached: 0 })).toBe(0);
    expect(cacheHitRateForBreakdown({ input: 0, cached: 20 })).toBeUndefined();
    expect(cacheHitRateForBreakdown(undefined)).toBeUndefined();
  });
});
