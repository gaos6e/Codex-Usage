import path from 'path';
import { describe, expect, it } from 'vitest';
import type { AppSettings, RunRecord, TokenBreakdown } from '../../src/shared/contracts';
import type { SessionReadResult } from '../../src/main/services/sessionReader';
import type { ThreadRecord } from '../../src/main/services/stateDbReader';
import { UsageService } from '../../src/main/services/usageService';

interface CreateRunsHost {
  createRuns(
    threads: ThreadRecord[],
    sessions: SessionReadResult,
    logBreakdowns: Map<string, TokenBreakdown>,
    settings: AppSettings,
  ): RunRecord[];
}

const settings: AppSettings = {
  codexDir: 'C:\\Users\\me\\.codex',
  includeArchivedSessions: true,
  includeDetailedLogs: false,
  autoRefreshSeconds: 60,
  idleGapMinutes: 30,
  theme: 'system',
  language: 'zh-CN',
  fontScale: { ui: 1, data: 1 },
  aliases: [],
  ignoredWorkspaces: [],
};

function createService(): UsageService {
  return new UsageService({
    baseDir: 'C:\\Temp\\CodexUsage',
    settingsPath: 'C:\\Temp\\CodexUsage\\settings.json',
    cacheDir: 'C:\\Temp\\CodexUsage\\cache',
    cachePath: 'C:\\Temp\\CodexUsage\\cache\\summary-cache.json',
    logsDir: 'C:\\Temp\\CodexUsage\\logs',
    logFilePath: 'C:\\Temp\\CodexUsage\\logs\\main.log',
    exportsDir: 'C:\\Temp\\CodexUsage\\exports',
  }, () => settings);
}

function createThread(id: string, tokensUsed: number, rolloutPath?: string): ThreadRecord {
  const createdAt = new Date('2026-04-23T01:00:00.000Z');
  const updatedAt = new Date('2026-04-23T01:01:00.000Z');
  return {
    id,
    rolloutPath,
    createdAt,
    updatedAt,
    cwd: 'D:\\Project',
    normalizedPath: 'D:\\Project',
    workspaceId: 'workspace-1',
    workspaceName: 'Project',
    title: id,
    modelProvider: 'openai',
    model: 'gpt',
    tokensUsed,
    archived: false,
    hidden: false,
  };
}

describe('usage service token totals', () => {
  it('prefers parsed token totals and falls back to state tokens when breakdown is unavailable', async () => {
    const service = createService() as unknown as CreateRunsHost;
    const rolloutPath = path.join('C:\\Users\\me\\.codex\\sessions', 'rollout-1.jsonl');
    const sessions: SessionReadResult = {
      sessionsById: new Map([[
        'thread-jsonl',
        {
          id: 'thread-jsonl',
          filePath: rolloutPath,
          archived: false,
          start: new Date('2026-04-23T01:00:00.000Z'),
          end: new Date('2026-04-23T01:00:10.000Z'),
          activeMs: 10000,
          tokenBreakdown: { total: 1280, input: 1200, cached: 900, output: 80, reasoning: 20 },
          parseWarnings: 0,
        },
      ]]),
      sessionsByFileStem: new Map(),
      warnings: [],
      filesRead: 1,
    };

    const runs = service.createRuns(
      [
        createThread('thread-jsonl', 100, rolloutPath),
        createThread('thread-fallback', 321),
      ],
      sessions,
      new Map(),
      settings,
    );
    const jsonlRun = runs.find((run) => run.id === 'thread-jsonl');
    const fallbackRun = runs.find((run) => run.id === 'thread-fallback');

    expect(jsonlRun?.totalTokens).toBe(1280);
    expect(jsonlRun?.tokenBreakdown).toMatchObject({ total: 1280, input: 1200, output: 80 });
    expect(fallbackRun?.totalTokens).toBe(321);
  });
});
