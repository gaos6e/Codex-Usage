import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import i18n from '../i18n';
import type { UsageFilters } from '../types';
import { ActivityPage } from './activity/ActivityPage';
import { DataPage } from './data/DataPage';
import { ModelsPage } from './models/ModelsPage';
import { ProjectsPage } from './projects/ProjectsPage';
import { SessionsPage } from './sessions/SessionsPage';

const api = vi.hoisted(() => ({
  getBootstrapStatus: vi.fn(),
  getWorkspaces: vi.fn(),
  updateWorkspaceSettings: vi.fn(),
  getSessions: vi.fn(),
  getSessionDetail: vi.fn(),
  getUsageEvents: vi.fn(),
  getModels: vi.fn(),
  listModelPrices: vi.fn(),
  saveModelPrice: vi.fn(),
  deleteModelPrice: vi.fn(),
  restoreBuiltinPrice: vi.fn(),
  previewPriceUpdate: vi.fn(),
  applyPriceUpdate: vi.fn(),
  getHeatmap: vi.fn(),
  getTools: vi.fn(),
  getDiagnostics: vi.fn(),
  startSync: vi.fn(),
  clearAnalysis: vi.fn(),
}));

vi.mock('../api', () => ({
  ...api,
  isTauriRuntime: () => true,
}));

const filters: UsageFilters = {
  range: { preset: 'last30_days', liveEnd: false },
  archived: 'all',
};

function renderPage(node: React.ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
}

describe('feature pages', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage('zh-CN');
    api.updateWorkspaceSettings.mockResolvedValue(undefined);
    api.getBootstrapStatus.mockResolvedValue({
      appVersion: '2.1.0', platform: 'windows',
      dataDirectory: 'C:\\Users\\tester\\AppData\\Local\\Chronolume\\v2',
      databasePath: 'C:\\Users\\tester\\AppData\\Local\\Chronolume\\v2\\chronolume-v2.sqlite3',
      schemaVersion: 1, databaseSizeBytes: 1024,
    });
    api.saveModelPrice.mockResolvedValue({ prices: [], reprice: {} });
    api.deleteModelPrice.mockResolvedValue({ prices: [], reprice: {} });
    api.restoreBuiltinPrice.mockResolvedValue({ prices: [], reprice: {} });
    api.startSync.mockResolvedValue({ phase: 'completed' });
    api.clearAnalysis.mockResolvedValue(undefined);
  });

  it('lists, edits, and opens workspace analytics', async () => {
    api.getWorkspaces.mockResolvedValue({
      page: 0, pageSize: 25, total: 1,
      items: [{
        id: 'workspace-1', label: 'Alpha', normalizedPath: 'D:/work/alpha', ignored: false,
        sessionCount: 4, totalTokens: 12_000, estimatedCostMicrousd: 750_000,
        unpricedEventCount: 0, activeMs: 60_000, activeDays: 2, lastActivityAtMs: 1_788_000_000_000,
      }],
    });
    const open = vi.fn();
    renderPage(<ProjectsPage filters={filters} onOpenWorkspace={open} />);

    expect(await screen.findByText('Alpha')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '打开工作区统计' }));
    expect(open).toHaveBeenCalledWith('workspace-1');
    fireEvent.click(screen.getByRole('button', { name: '编辑工作区' }));
    fireEvent.change(screen.getByLabelText('别名'), { target: { value: 'Alpha alias' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(api.updateWorkspaceSettings).toHaveBeenCalledWith('workspace-1', 'Alpha alias', false));
  });

  it('opens structured session details and server-paged token events', async () => {
    const session = {
      id: 'session-1', title: '2026-07-10 · abc123', workspaceId: 'workspace-1', workspaceLabel: 'Alpha',
      startedAtMs: 1_788_000_000_000, endedAtMs: 1_788_000_060_000,
      activeMs: 60_000, activeMethod: 'lifecycle', activeIsEstimate: false,
      modelProvider: 'openai', latestModel: 'gpt-5.6-sol', totalTokens: 120,
      inputTokens: 90, freshInputTokens: 70, cachedInputTokens: 20, outputTokens: 30,
      reasoningTokens: 10, estimatedCostMicrousd: 5_000, unpricedEventCount: 0,
      archived: false, integrityStatus: 'complete',
    };
    api.getSessions.mockResolvedValue({ page: 0, pageSize: 30, total: 1, items: [session] });
    api.getSessionDetail.mockResolvedValue({
      session,
      parsing: { sourceKind: 'session', sourceStatus: 'ready', parserVersion: 3, warningCount: 0 },
      modelSegments: [{
        segmentIndex: 0, startedAtMs: session.startedAtMs, provider: 'openai', model: 'gpt-5.6-sol',
        inputTokens: 90, cachedInputTokens: 20, outputTokens: 30, reasoningTokens: 10,
        estimatedCostMicrousd: 5_000, unpricedEventCount: 0,
      }],
      activitySegments: [{
        segmentIndex: 0, startedAtMs: session.startedAtMs, endedAtMs: session.endedAtMs,
        activeMs: 60_000, method: 'lifecycle', isEstimate: false,
      }],
      tools: [{ toolName: 'apply_patch', category: 'edit', operationKind: 'mutating', callCount: 2, sessionCount: 1 }],
      recentUsageEvents: [], retainedEventCount: 1,
    });
    api.getUsageEvents.mockResolvedValue({
      page: 0, pageSize: 100, total: 1,
      items: [{
        id: 1, occurredAtMs: session.startedAtMs, provider: 'openai', model: 'gpt-5.6-sol',
        inputTokens: 90, cachedInputTokens: 20, outputTokens: 30, reasoningTokens: 10,
        totalTokens: 120, estimatedCostMicrousd: 5_000, integrityStatus: 'complete',
      }],
    });
    renderPage(<SessionsPage filters={filters} />);

    fireEvent.click(await screen.findByRole('button', { name: '查看会话详情' }));
    expect(await screen.findByText('仅显示结构化统计；不读取或显示对话正文。')).toBeInTheDocument();
    expect(screen.getByText('apply_patch · edit')).toBeInTheDocument();
    expect(within(screen.getByLabelText('会话结构化详情')).getByText(/生命周期/)).toBeInTheDocument();
    expect(api.getUsageEvents).toHaveBeenCalledWith('session-1', 0);
  });

  it('shows unpriced usage and supports price add, edit, delete, and restore', async () => {
    api.getModels.mockResolvedValue({
      page: 0, pageSize: 30, total: 1,
      items: [{
        model: 'gpt-5.6-custom', sessionCount: 1,
        inputTokens: 100, freshInputTokens: 80, cachedInputTokens: 20, outputTokens: 10,
        reasoningTokens: 5, totalTokens: 115, cacheHitRate: .2,
        estimatedCostMicrousd: undefined, unpricedEventCount: 1,
      }],
    });
    api.listModelPrices.mockResolvedValue([{
      provider: 'openai', pricingId: 'gpt-5.6-sol', displayName: 'GPT-5.6 Sol',
      inputPerMillionUsd: '1.25', outputPerMillionUsd: '10', cacheReadPerMillionUsd: '0.125',
      isBuiltin: true, isOverridden: true, isDeleted: false, revision: 2,
    }]);
    renderPage(<ModelsPage filters={filters} />);

    expect((await screen.findAllByText('未定价')).length).toBeGreaterThan(0);
    expect(api.getModels).toHaveBeenCalledWith(expect.objectContaining({ sort: 'name', descending: true }));
    expect(screen.getByRole('columnheader', { name: '平均每百万 Token 成本' })).toBeInTheDocument();
    expect(screen.queryByText('$0.00')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('tab', { name: '价格表' }));
    expect(await screen.findByText('GPT-5.6 Sol')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '编辑价格' }));
    fireEvent.change(screen.getByLabelText('输出'), { target: { value: '11' } });
    fireEvent.click(screen.getByRole('button', { name: '保存并重算' }));
    await waitFor(() => expect(api.saveModelPrice).toHaveBeenCalledWith(expect.objectContaining({ pricingId: 'gpt-5.6-sol', outputPerMillionUsd: '11' })));

    fireEvent.click(screen.getByRole('button', { name: '删除价格' }));
    fireEvent.click(screen.getByRole('button', { name: '恢复内置价格' }));
    await waitFor(() => {
      expect(api.deleteModelPrice).toHaveBeenCalledWith('openai', 'gpt-5.6-sol');
      expect(api.restoreBuiltinPrice).toHaveBeenCalledWith('openai', 'gpt-5.6-sol');
    });

    fireEvent.click(screen.getByRole('button', { name: '添加价格' }));
    fireEvent.change(screen.getByLabelText('计价 ID'), { target: { value: 'gpt-custom' } });
    fireEvent.change(screen.getByLabelText('显示名称'), { target: { value: 'Custom' } });
    fireEvent.change(screen.getByLabelText('输入'), { target: { value: '1' } });
    fireEvent.change(screen.getByLabelText('缓存读取'), { target: { value: '.1' } });
    fireEvent.change(screen.getByLabelText('输出'), { target: { value: '8' } });
    fireEvent.click(screen.getByRole('button', { name: '保存并重算' }));
    await waitFor(() => expect(api.saveModelPrice).toHaveBeenCalledWith(expect.objectContaining({ pricingId: 'gpt-custom', cacheWritePerMillionUsd: undefined })));
  });

  it('renders heatmap/tool analytics and exposes repair index', async () => {
    api.getHeatmap.mockResolvedValue({
      metric: 'sessions', span: 'year', startDate: '2026-07-09', endDate: '2026-07-10', maxValue: 2,
      points: [{ date: '2026-07-10', value: 2, sessionCount: 2, totalTokens: 500, activeMs: 60_000 }],
    });
    api.getTools.mockResolvedValue({
      totalCalls: 3, uniqueTools: 2,
      topTools: [{ toolName: 'apply_patch', category: 'edit', operationKind: 'mutating', callCount: 2, sessionCount: 1 }],
      categories: [{ category: 'edit', callCount: 2 }], trend: [{ date: '2026-07-10', callCount: 3 }],
    });
    api.getDiagnostics.mockResolvedValue({
      databaseSizeBytes: 1024, databaseIntegrityOk: true, schemaVersion: 1, parserVersion: 3,
      indexedSessions: 1, retainedUsageEvents: 1, retainedToolEvents: 1, sources: [], recentRuns: [], generatedAtMs: Date.now(),
    });

    const { unmount } = renderPage(<ActivityPage filters={filters} />);
    expect(await screen.findByText('apply_patch')).toBeInTheDocument();
    expect(screen.getByText('3')).toBeInTheDocument();
    unmount();

    renderPage(<DataPage />);
    fireEvent.click(await screen.findByRole('button', { name: '修复索引' }));
    await waitFor(() => expect(api.startSync).toHaveBeenCalledWith('repair'));
  });

  it('renders the backend-provided analytics directory for Windows and macOS', async () => {
    api.getDiagnostics.mockResolvedValue({
      databaseSizeBytes: 1024, databaseIntegrityOk: true, schemaVersion: 1, parserVersion: 3,
      indexedSessions: 1, retainedUsageEvents: 1, retainedToolEvents: 1, sources: [], recentRuns: [], generatedAtMs: Date.now(),
    });
    const { unmount } = renderPage(<DataPage />);
    expect(await screen.findByText('C:\\Users\\tester\\AppData\\Local\\Chronolume\\v2')).toBeInTheDocument();
    unmount();

    api.getBootstrapStatus.mockResolvedValue({
      appVersion: '2.1.0', platform: 'macos',
      dataDirectory: '/Users/tester/Library/Application Support/Chronolume/v2',
      databasePath: '/Users/tester/Library/Application Support/Chronolume/v2/chronolume-v2.sqlite3',
      schemaVersion: 1, databaseSizeBytes: 1024,
    });
    renderPage(<DataPage />);
    expect(await screen.findByText('/Users/tester/Library/Application Support/Chronolume/v2')).toBeInTheDocument();
    expect(screen.queryByText(/LOCALAPPDATA/)).not.toBeInTheDocument();
  });
});
