import fs from 'fs';
import os from 'os';
import path from 'path';
import { describe, expect, it } from 'vitest';
import type { AppSettings, RunRecord, TokenBreakdown } from '../../src/shared/contracts';
import type { SessionReadResult } from '../../src/main/services/sessionReader';
import type { SourceDetection } from '../../src/main/services/sourceDetector';
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

interface FingerprintHost {
  createFingerprint(settings: AppSettings, detection: SourceDetection): string;
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
  it('changes the source fingerprint when nested jsonl sessions change or are deleted', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'codex-usage-fingerprint-'));
    try {
      const sessionsDir = path.join(root, 'sessions');
      const archivedSessionsDir = path.join(root, 'archived_sessions');
      const nestedDir = path.join(sessionsDir, '2026', '06', '23');
      fs.mkdirSync(nestedDir, { recursive: true });
      fs.mkdirSync(archivedSessionsDir, { recursive: true });
      const sessionPath = path.join(nestedDir, 'session-1.jsonl');
      fs.writeFileSync(sessionPath, 'first\n', 'utf8');

      const service = createService() as unknown as FingerprintHost;
      const detection: SourceDetection = {
        stateDbPath: path.join(root, 'state_5.sqlite'),
        logsDbPath: path.join(root, 'logs_2.sqlite'),
        sessionsDir,
        archivedSessionsDir,
        sessionFiles: [sessionPath],
        archivedSessionFiles: [],
        statuses: [],
        warnings: [],
      };

      const first = service.createFingerprint(settings, detection);
      fs.writeFileSync(sessionPath, 'first\nsecond\n', 'utf8');
      const changed = service.createFingerprint(settings, detection);
      fs.unlinkSync(sessionPath);
      detection.sessionFiles = [];
      const deleted = service.createFingerprint(settings, detection);

      expect(changed).not.toBe(first);
      expect(deleted).not.toBe(changed);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it('uses state token totals while retaining parsed token breakdown details', async () => {
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

    expect(jsonlRun?.totalTokens).toBe(100);
    expect(jsonlRun?.tokenBreakdown).toMatchObject({ total: 100, input: 1200, output: 80 });
    expect(fallbackRun?.totalTokens).toBe(321);
  });

  it('falls back to parsed totals only when state tokens are unavailable', async () => {
    const service = createService() as unknown as CreateRunsHost;
    const rolloutPath = path.join('C:\\Users\\me\\.codex\\sessions', 'rollout-2.jsonl');
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
      [createThread('thread-jsonl', 0, rolloutPath)],
      sessions,
      new Map(),
      settings,
    );

    expect(runs[0]?.totalTokens).toBe(1280);
    expect(runs[0]?.tokenBreakdown).toMatchObject({ total: 1280, input: 1200, output: 80 });
  });

  it('includes jsonl sessions that are not present in the state database yet', async () => {
    const service = createService() as unknown as CreateRunsHost;
    const rolloutPath = path.join('C:\\Users\\me\\.codex\\sessions', 'rollout-3.jsonl');
    const sessions: SessionReadResult = {
      sessionsById: new Map([[
        'session-only',
        {
          id: 'session-only',
          filePath: rolloutPath,
          archived: false,
          cwd: 'D:\\Project',
          model: 'gpt-5',
          start: new Date('2026-04-23T01:00:00.000Z'),
          end: new Date('2026-04-23T01:05:00.000Z'),
          activeMs: 300000,
          tokenBreakdown: { total: 5000, input: 4500, cached: 3000, output: 500, reasoning: 100 },
          parseWarnings: 0,
        },
      ]]),
      sessionsByFileStem: new Map(),
      warnings: [],
      filesRead: 1,
    };

    const runs = service.createRuns([], sessions, new Map(), settings);

    expect(runs).toHaveLength(1);
    expect(runs[0]).toMatchObject({
      id: 'session-only',
      title: 'JSONL session session-',
      workspacePath: 'D:\\Project',
      totalTokens: 5000,
      tokenBreakdown: { total: 5000, input: 4500, output: 500 },
      rolloutPath,
    });
  });
});
