import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { cascadeFilter, DashboardFilters } from './DashboardFilters';
import type { UsageFilters } from '../types';

const filters: UsageFilters = {
  range: { preset: 'last30_days', liveEnd: false },
  workspaceId: 'workspace-a',
  modelProvider: 'openai',
  model: 'gpt-5.6-codex',
  archived: 'all',
};

describe('cascadeFilter', () => {
  it('clears provider and model when the workspace changes', () => {
    expect(cascadeFilter(filters, 'workspaceId', 'workspace-b')).toMatchObject({
      workspaceId: 'workspace-b',
      modelProvider: undefined,
      model: undefined,
    });
  });

  it('clears only the model when the provider changes', () => {
    expect(cascadeFilter(filters, 'modelProvider', 'azure')).toMatchObject({
      workspaceId: 'workspace-a',
      modelProvider: 'azure',
      model: undefined,
    });
  });

  it('offers every time preset and supports a live-ended custom range', () => {
    const onChange = vi.fn();
    render(<DashboardFilters filters={filters} refreshing={false} onChange={onChange} onRefresh={vi.fn()} />);
    expect(screen.getAllByRole('button', { pressed: false }).length).toBeGreaterThanOrEqual(7);
    fireEvent.click(screen.getByRole('button', { name: '自定义' }));
    const custom = onChange.mock.calls.at(-1)?.[0] as UsageFilters;
    expect(custom.range).toMatchObject({ preset: 'custom', liveEnd: false });

    render(<DashboardFilters filters={{ ...custom, range: { ...custom.range, liveEnd: true } }} refreshing={false} onChange={onChange} onRefresh={vi.fn()} />);
    expect(screen.getByLabelText('结束时间跟随现在')).toBeChecked();
    expect(screen.getByLabelText('结束')).toBeDisabled();
  });

  it('opens workspace management without applying the sentinel as a filter', () => {
    const onChange = vi.fn();
    const onManageWorkspaces = vi.fn();
    render(<DashboardFilters
      filters={{ ...filters, workspaceId: undefined }}
      options={{ workspaces: [], providers: [], models: [] }}
      refreshing={false}
      onChange={onChange}
      onRefresh={vi.fn()}
      onManageWorkspaces={onManageWorkspaces}
    />);

    fireEvent.change(screen.getByLabelText('工作区'), { target: { value: '__manage_workspaces__' } });
    expect(onManageWorkspaces).toHaveBeenCalledOnce();
    expect(onChange).not.toHaveBeenCalled();
  });
});
