import { describe, expect, it } from 'vitest';
import type { RunRecord } from '../../src/shared/contracts';
import { UsageService } from '../../src/main/services/usageService';

describe('export privacy helpers', () => {
  it('anonymizes workspace paths and removes rollout paths', () => {
    const service = new UsageService({
      baseDir: 'C:\\Temp\\CodexUsage',
      settingsPath: 'C:\\Temp\\CodexUsage\\settings.json',
      cacheDir: 'C:\\Temp\\CodexUsage\\cache',
      cachePath: 'C:\\Temp\\CodexUsage\\cache\\summary-cache.json',
      logsDir: 'C:\\Temp\\CodexUsage\\logs',
      logFilePath: 'C:\\Temp\\CodexUsage\\logs\\main.log',
      exportsDir: 'C:\\Temp\\CodexUsage\\exports',
    }, () => {
      throw new Error('settings not needed');
    });

    const run: RunRecord = {
      id: '1',
      title: 'Allowed title',
      workspaceId: 'abc',
      workspaceName: 'secret-project',
      workspacePath: 'C:\\Users\\me\\secret-project',
      startTime: new Date(2026, 3, 21).toISOString(),
      endTime: new Date(2026, 3, 21, 1).toISOString(),
      durationMs: 3600000,
      durationMethod: 'thread-span',
      model: 'gpt',
      modelProvider: 'openai',
      totalTokens: 1000,
      rolloutPath: 'C:\\Users\\me\\.codex\\sessions\\secret.jsonl',
      archived: false,
    };

    const [anonymized] = service.anonymizeRuns([run]);
    expect(anonymized.title).toBe('Allowed title');
    expect(anonymized.workspacePath).toBe('Workspace 1');
    expect(anonymized.rolloutPath).toBeUndefined();
  });
});
