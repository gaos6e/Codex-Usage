import { describe, expect, it } from 'vitest';
import {
  buildChartData,
  deserializeSeriesVisibility,
  trendAnimationEnabled,
  toggleSeries,
  type TrendSeriesKey,
} from './UsageTrendChart';
import type { TrendPoint } from '../types';

const point: TrendPoint = {
  key: '2026-07-10',
  timestampMs: Date.UTC(2026, 6, 10, 8),
  inputTokens: 120,
  freshInputTokens: 80,
  cachedInputTokens: 40,
  outputTokens: 30,
  reasoningTokens: 10,
  totalTokens: 160,
  estimatedCostMicrousd: 275_000,
  unpricedEventCount: 0,
  sessionCount: 2,
  activeMs: 60_000,
};

describe('UsageTrendChart data mapping', () => {
  it('preserves exact token and microdollar fields', () => {
    const [datum] = buildChartData([point], 'day');
    expect(datum).toMatchObject({
      freshInputTokens: 80,
      cachedInputTokens: 40,
      outputTokens: 30,
      reasoningTokens: 10,
      estimatedCostMicrousd: 275_000,
    });
    expect(datum.label.length).toBeGreaterThan(0);
  });

  it('restores valid persisted legend keys and ignores unknown keys', () => {
    expect([...deserializeSeriesVisibility('["outputTokens","unknown"]')]).toEqual(['outputTokens']);
    expect(deserializeSeriesVisibility('not-json').size).toBe(5);
  });

  it('never lets the legend hide its final visible series', () => {
    const onlyOutput = new Set<TrendSeriesKey>(['outputTokens']);
    expect(toggleSeries(onlyOutput, 'outputTokens')).toEqual(onlyOutput);
    expect(toggleSeries(onlyOutput, 'freshInputTokens')).toEqual(
      new Set<TrendSeriesKey>(['outputTokens', 'freshInputTokens']),
    );
  });

  it('disables chart animation when reduced motion is requested', () => {
    expect(trendAnimationEnabled(() => ({ matches: true }))).toBe(false);
    expect(trendAnimationEnabled(() => ({ matches: false }))).toBe(true);
  });
});
