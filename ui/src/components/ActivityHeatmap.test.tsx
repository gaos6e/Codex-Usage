import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ActivityHeatmap } from './ActivityHeatmap';

describe('ActivityHeatmap', () => {
  it('maps values to accessible cells and switches metrics and spans', () => {
    const onMetric = vi.fn();
    const onSpan = vi.fn();
    render(<ActivityHeatmap
      metric="tokens"
      span="month"
      loading={false}
      onMetric={onMetric}
      onSpan={onSpan}
      snapshot={{
        metric: 'tokens', span: 'month', startDate: '2026-07-01', endDate: '2026-07-02',
        maxValue: 100, points: [
          { date: '2026-07-01', value: 0, sessionCount: 0, totalTokens: 0, activeMs: 0 },
          { date: '2026-07-02', value: 100, sessionCount: 2, totalTokens: 100, activeMs: 60_000 },
        ],
      }}
    />);
    expect(screen.getByLabelText('2026-07-02，100 Token')).toHaveClass('level-4');
    expect(screen.getByRole('option', { name: '周' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: '月' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: '年' })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('热力图指标'), { target: { value: 'active_time' } });
    fireEvent.change(screen.getByLabelText('热力图范围'), { target: { value: 'year' } });
    expect(onMetric).toHaveBeenCalledWith('active_time');
    expect(onSpan).toHaveBeenCalledWith('year');
  });

  it('opens a scrollable heatmap at the latest dates', () => {
    const scrollWidth = vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get').mockReturnValue(500);
    const clientWidth = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(200);
    try {
      render(<ActivityHeatmap
        metric="tokens"
        span="year"
        loading={false}
        onMetric={vi.fn()}
        onSpan={vi.fn()}
        snapshot={{
          metric: 'tokens', span: 'year', startDate: '2025-07-12', endDate: '2026-07-11',
          maxValue: 1, points: [{ date: '2026-07-11', value: 1, sessionCount: 1, totalTokens: 1, activeMs: 1 }],
        }}
      />);
      expect(screen.getByRole('region', { name: /拖动查看更早日期/ })).toHaveProperty('scrollLeft', 300);
    } finally {
      scrollWidth.mockRestore();
      clientWidth.mockRestore();
    }
  });

  it('uses metric-specific styling and quantile buckets for skewed token values', () => {
    render(<ActivityHeatmap
      metric="tokens"
      span="week"
      loading={false}
      onMetric={vi.fn()}
      onSpan={vi.fn()}
      snapshot={{
        metric: 'tokens', span: 'week', startDate: '2026-07-01', endDate: '2026-07-04',
        maxValue: 100, points: [1, 2, 3, 100].map((value, index) => ({
          date: `2026-07-0${index + 1}`, value, sessionCount: 1, totalTokens: value, activeMs: value,
        })),
      }}
    />);

    const cells = screen.getAllByRole('img');
    expect(cells.map((cell) => [...cell.classList])).toEqual([
      expect.arrayContaining(['metric-tokens', 'level-1']),
      expect.arrayContaining(['metric-tokens', 'level-2']),
      expect.arrayContaining(['metric-tokens', 'level-3']),
      expect.arrayContaining(['metric-tokens', 'level-4']),
    ]);
  });
});
