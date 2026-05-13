import fs from 'fs';
import os from 'os';
import path from 'path';
import { afterEach, describe, expect, it } from 'vitest';
import type { DiagnosticsSnapshot, MetricCard, RunRecord, UsageFilters, WorkspaceSummary } from '../../src/shared/contracts';
import { ALL_WORKSPACES_ID } from '../../src/shared/pathUtils';
import { UsageService } from '../../src/main/services/usageService';

const tempDirs: string[] = [];

function createTempDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'codex-usage-cards-'));
  tempDirs.push(dir);
  return dir;
}

afterEach(() => {
  while (tempDirs.length) {
    const dir = tempDirs.pop();
    if (dir) {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  }
});

function createRun(id: string, start: Date, totalTokens: number, input?: number, output?: number): RunRecord {
  return {
    id,
    title: `Run ${id}`,
    workspaceId: 'workspace-1',
    workspaceName: 'Project',
    workspacePath: 'D:\\Project',
    startTime: start.toISOString(),
    endTime: new Date(start.getTime() + 60 * 60 * 1000).toISOString(),
    durationMs: 60 * 60 * 1000,
    durationMethod: 'jsonl-events',
    model: 'gpt',
    modelProvider: 'openai',
    totalTokens,
    tokenBreakdown: input === undefined && output === undefined
      ? { unavailableReason: 'Breakdown unavailable for this run.' }
      : { input, output, cached: input === undefined ? undefined : Math.floor(input / 10) },
    archived: false,
  };
}

function createServiceWithCache(runs: RunRecord[]): UsageService {
  const baseDir = createTempDir();
  const cacheDir = path.join(baseDir, 'cache');
  fs.mkdirSync(cacheDir, { recursive: true });
  const cachePath = path.join(cacheDir, 'summary-cache.json');
  const workspace: WorkspaceSummary = {
    id: 'workspace-1',
    displayName: 'Project',
    normalizedPath: 'D:\\Project',
    rawPath: 'D:\\Project',
    runs: runs.length,
    tokens: runs.reduce((sum, run) => sum + run.totalTokens, 0),
    agentTimeMs: runs.reduce((sum, run) => sum + run.durationMs, 0),
    activeDays: 2,
    hidden: false,
  };
  const diagnostics: DiagnosticsSnapshot = {
    codexDir: 'C:\\Users\\me\\.codex',
    generatedAt: new Date(2026, 3, 23, 12).toISOString(),
    parseDurationMs: 1,
    sources: [],
    warnings: [],
    cacheStatus: 'rebuilt',
    appDataDir: baseDir,
    logFilePath: path.join(baseDir, 'logs', 'main.log'),
  };

  fs.writeFileSync(cachePath, JSON.stringify({
    version: 3,
    fingerprint: 'test',
    data: {
      generatedAt: diagnostics.generatedAt,
      threads: [],
      runs,
      workspaces: [workspace],
      diagnostics,
      allTimeStart: new Date(2026, 3, 21).toISOString(),
      allTimeEnd: new Date(2026, 3, 23, 23).toISOString(),
    },
  }), 'utf8');

  return new UsageService({
    baseDir,
    settingsPath: path.join(baseDir, 'settings.json'),
    cacheDir,
    cachePath,
    logsDir: path.join(baseDir, 'logs'),
    logFilePath: path.join(baseDir, 'logs', 'main.log'),
    exportsDir: path.join(baseDir, 'exports'),
  }, () => {
    throw new Error('settings not needed');
  });
}

function customFilters(view: 'time' | 'tokens'): UsageFilters {
  return {
    workspaceId: ALL_WORKSPACES_ID,
    view,
    range: { preset: 'custom', startDate: '2026-04-21', endDate: '2026-04-22', aggregation: 'daily' },
  };
}

function detailValues(card: MetricCard): Array<[string, string | undefined]> {
  return card.detailItems?.map((item) => [item.labelKey, item.value ?? item.valueKey]) || [];
}

describe('usage metric cards', () => {
  it('orders time cards for the dashboard', () => {
    const service = createServiceWithCache([
      createRun('1', new Date(2026, 3, 21, 10), 1000, 700, 300),
    ]);

    const snapshot = service.getCachedSnapshot(customFilters('time'));

    expect(snapshot?.cards.map((card) => card.id)).toEqual([
      'selected-time',
      'avg-day-time',
      'active-days',
      'runs',
      'all-time',
    ]);
    expect(snapshot?.cards[4]).toMatchObject({
      sublabelKey: 'metric.calendarDays',
      sublabelArgs: { count: 3 },
    });
  });

  it('orders token cards and reports input/output using each card scope', () => {
    const service = createServiceWithCache([
      createRun('1', new Date(2026, 3, 21, 10), 1000, 700, 300),
      createRun('2', new Date(2026, 3, 22, 10), 500, 400, 100),
      createRun('3', new Date(2026, 3, 23, 10), 2000, 1000, 1000),
    ]);

    const snapshot = service.getCachedSnapshot(customFilters('tokens'));

    expect(snapshot?.cards.map((card) => card.id)).toEqual([
      'selected-tokens',
      'avg-day-tokens',
      'all-tokens',
      'avg-tokens',
      'token-cache-hit-rate',
    ]);
    expect(detailValues(snapshot?.cards[0] as MetricCard)).toEqual([
      ['metric.inputTokens', '1.1K'],
      ['metric.outputTokens', '400'],
    ]);
    expect(detailValues(snapshot?.cards[1] as MetricCard)).toEqual([
      ['metric.inputTokens', '550'],
      ['metric.outputTokens', '200'],
    ]);
    expect(detailValues(snapshot?.cards[2] as MetricCard)).toEqual([
      ['metric.inputTokens', '2.1K'],
      ['metric.outputTokens', '1.4K'],
    ]);
    expect(snapshot?.cards[2]).toMatchObject({
      sublabelKey: 'metric.calendarDays',
      sublabelArgs: { count: 3 },
    });
    expect(detailValues(snapshot?.cards[3] as MetricCard)).toEqual([
      ['metric.inputTokens', '550'],
      ['metric.outputTokens', '200'],
    ]);
    expect(detailValues(snapshot?.cards[4] as MetricCard)).toEqual([
      ['metric.cachedTokens', '110'],
      ['metric.inputTokens', '1.1K'],
    ]);
    expect(snapshot?.cards[4].sublabelKey).toBeUndefined();
  });

  it('keeps missing input/output breakdown unavailable instead of converting it to zero', () => {
    const service = createServiceWithCache([
      createRun('1', new Date(2026, 3, 21, 10), 1000),
    ]);

    const snapshot = service.getCachedSnapshot(customFilters('tokens'));

    expect(detailValues(snapshot?.cards[0] as MetricCard)).toEqual([
      ['metric.inputTokens', 'metric.unavailable'],
      ['metric.outputTokens', 'metric.unavailable'],
    ]);
    expect(snapshot?.cards[4]).toMatchObject({
      id: 'token-cache-hit-rate',
      valueKey: 'metric.unavailable',
    });
  });
});
