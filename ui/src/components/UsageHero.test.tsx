import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { UsageHero } from './UsageHero';
import type { HeroMetrics } from '../types';

const hero: HeroMetrics = {
  realTotalTokens: 1200,
  inputTokens: 1000,
  freshInputTokens: 600,
  cachedInputTokens: 400,
  outputTokens: 200,
  reasoningTokens: 50,
  cacheHitRate: .4,
  estimatedCostMicrousd: 1_250_000,
  unpricedEventCount: 2,
  sessionCount: 3,
  activeMs: 90 * 60_000,
  activeDays: 2,
  averageTokensPerDay: 600,
  averageCostMicrousdPerDay: 625_000,
  averageSessionsPerDay: 1.5,
  averageActiveMsPerDay: 45 * 60_000,
  peakDay: '2026-07-10',
  peakDayTokens: 1200,
  longestActiveStreakDays: 2,
};

describe('UsageHero', () => {
  it('renders exact totals, cache rate, pricing and partial state without inventing values', () => {
    render(<UsageHero hero={hero} partial />);
    expect(screen.getByText('1,200')).toHaveAttribute('title', '1,200');
    expect(screen.getByText('40.0%')).toBeInTheDocument();
    expect(screen.getByText('$1.25')).toBeInTheDocument();
    expect(screen.getByText('部分未定价')).toBeInTheDocument();
    expect(screen.getByText('1h 30m')).toBeInTheDocument();
  });

  it('shows unpriced instead of a fake zero-dollar cost', () => {
    render(<UsageHero hero={{ ...hero, estimatedCostMicrousd: undefined }} partial={false} />);
    expect(screen.getByText('未定价')).toBeInTheDocument();
    expect(screen.queryByText('$0.00')).not.toBeInTheDocument();
  });
});
