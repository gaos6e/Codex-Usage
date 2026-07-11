import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';
import i18n from './i18n';
import type { DashboardSnapshot } from './types';

const api = vi.hoisted(() => ({
  getBootstrapStatus: vi.fn(),
  getDashboard: vi.fn(),
  getSyncStatus: vi.fn(),
  startSync: vi.fn(),
  cancelSync: vi.fn(),
  getAppPreferences: vi.fn(),
  getWorkspaceCatalog: vi.fn(),
  saveAppPreferences: vi.fn(),
  getHeatmap: vi.fn(),
  exportData: vi.fn(),
  writeChartPng: vi.fn(),
}));

const nativeRuntime = vi.hoisted(() => ({
  enabled: false,
  listeners: new Map<string, () => void>(),
}));

vi.mock('./api', () => ({
  ...api,
  isTauriRuntime: () => nativeRuntime.enabled,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: () => void) => {
    nativeRuntime.listeners.set(event, handler);
    return () => nativeRuntime.listeners.delete(event);
  }),
}));
vi.mock('./components/UsageTrendChart', () => ({
  UsageTrendChart: () => <div data-testid="trend-chart" />,
}));

const baseSnapshot: DashboardSnapshot = {
  resolvedRange: {
    startMs: 1_787_000_000_000,
    endMs: 1_788_000_000_000,
    startLocalDate: '2026-06-10',
    endLocalDate: '2026-07-10',
    calendarDays: 30,
    granularity: 'day',
  },
  hero: {
    realTotalTokens: 0, inputTokens: 0, freshInputTokens: 0, cachedInputTokens: 0,
    outputTokens: 0, reasoningTokens: 0, unpricedEventCount: 0, sessionCount: 0,
    activeMs: 0, activeDays: 0, averageTokensPerDay: 0, averageSessionsPerDay: 0,
    averageActiveMsPerDay: 0, peakDayTokens: 0, longestActiveStreakDays: 0,
  },
  trend: [],
  filterOptions: { workspaces: [], providers: [], models: [] },
  dataState: 'complete',
  generatedAtMs: Date.now(),
};

function renderApp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><App /></QueryClientProvider>);
}

describe('App data states', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    nativeRuntime.enabled = false;
    nativeRuntime.listeners.clear();
    await i18n.changeLanguage('zh-CN');
    api.getBootstrapStatus.mockResolvedValue({
      appVersion: '2.1.0', platform: 'windows', dataDirectory: 'C:/Users/test/AppData/Local/Chronolume/v2',
      databasePath: '', schemaVersion: 1, databaseSizeBytes: 0,
    });
    api.getSyncStatus.mockResolvedValue({
      phase: 'idle', filesTotal: 0, filesCompleted: 0, bytesTotal: 0, bytesRead: 0,
      recordsWritten: 0, recordsSkipped: 0, parseFailures: 0, fileErrors: 0,
      speedBytesPerSecond: 0, updatedAtMs: Date.now(), cancelRequested: false,
    });
    api.getAppPreferences.mockResolvedValue({ idleGapMinutes: 30, visibleWorkspaceIds: [] });
    api.getWorkspaceCatalog.mockResolvedValue([]);
    api.saveAppPreferences.mockImplementation(async (preferences) => preferences);
    api.getHeatmap.mockResolvedValue({
      metric: 'tokens', span: 'year', startDate: '2026-07-01', endDate: '2026-07-10',
      maxValue: 0, points: [],
    });
    api.startSync.mockResolvedValue({ phase: 'completed' });
  });

  it('renders the explicit empty state without synthetic usage', async () => {
    api.getDashboard.mockResolvedValue({ ...baseSnapshot, dataState: 'empty' });
    renderApp();
    expect(await screen.findByText('这个范围内还没有用量')).toBeInTheDocument();
    expect(screen.getByLabelText('用量汇总').querySelector('[title="0"]')).toHaveTextContent('0');
    expect(screen.getByText('活跃热力图')).toBeInTheDocument();
    expect(api.getHeatmap).toHaveBeenCalledWith(expect.anything(), 'tokens', 'year');
    const analytics = screen.getByTestId('overview-analytics-grid');
    const heatmap = within(analytics).getByText('活跃热力图').closest('section');
    const trend = await within(analytics).findByTestId('trend-chart');
    expect(analytics.firstElementChild).toBe(heatmap);
    expect(analytics.lastElementChild).toContainElement(trend);
  });

  it('does not show a partial-data notice but still labels stale snapshots', async () => {
    api.getDashboard.mockResolvedValue({ ...baseSnapshot, dataState: 'partial' });
    const { unmount } = renderApp();
    expect(await screen.findByText('真实总 Token')).toBeInTheDocument();
    expect(screen.queryByText('部分会话缺少完整 Token 或生命周期记录；当前仅展示可验证的真实数据。')).not.toBeInTheDocument();
    unmount();

    api.getDashboard.mockResolvedValue({ ...baseSnapshot, generatedAtMs: Date.now() - 10 * 60_000 });
    renderApp();
    expect(await screen.findByText('展示的是上次生成的快照，后台正在刷新。')).toBeInTheDocument();
  });

  it('shows a retryable error instead of silently substituting data', async () => {
    api.getDashboard.mockRejectedValue(new Error('synthetic query failure'));
    renderApp();
    expect(await screen.findByRole('alert')).toHaveTextContent('synthetic query failure');
    expect(screen.getByRole('button', { name: '重试查询' })).toBeInTheDocument();
  });

  it('opens settings when the native macOS Settings menu event is emitted', async () => {
    nativeRuntime.enabled = true;
    api.getDashboard.mockResolvedValue(baseSnapshot);
    const { unmount } = renderApp();

    await waitFor(() => expect(nativeRuntime.listeners.has('chronolume-open-settings')).toBe(true));
    act(() => nativeRuntime.listeners.get('chronolume-open-settings')?.());

    expect(await screen.findByRole('heading', { name: '外观与偏好' })).toBeInTheDocument();
    unmount();
  });
});
