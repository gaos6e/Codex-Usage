import { describe, expect, it } from 'vitest';
import { isInsideRange, resolveTimeRange } from '../../src/shared/timeRange';

describe('time ranges', () => {
  const now = new Date(2026, 3, 21, 13, 30);

  it('treats Today as the local calendar day', () => {
    const range = resolveTimeRange({ preset: 'today' }, undefined, undefined, now);
    expect(range.start.toISOString()).toBe(new Date(2026, 3, 21).toISOString());
    expect(range.end.toISOString()).toBe(new Date(2026, 3, 22).toISOString());
    expect(isInsideRange(new Date(2026, 3, 21, 23, 59), range)).toBe(true);
    expect(isInsideRange(new Date(2026, 3, 22, 0, 0), range)).toBe(false);
  });

  it('makes rolling ranges inclusive of today', () => {
    const range = resolveTimeRange({ preset: 'last7' }, undefined, undefined, now);
    expect(range.start.toISOString()).toBe(new Date(2026, 3, 15).toISOString());
    expect(range.end.toISOString()).toBe(new Date(2026, 3, 22).toISOString());
  });

  it('uses weekly aggregation for long all-time ranges by default', () => {
    const range = resolveTimeRange(
      { preset: 'all' },
      new Date(2026, 0, 1),
      new Date(2026, 3, 21),
      now,
    );
    expect(range.aggregation).toBe('weekly');
  });
});
