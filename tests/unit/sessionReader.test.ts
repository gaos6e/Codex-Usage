import fs from 'fs';
import os from 'os';
import path from 'path';
import { afterEach, describe, expect, it } from 'vitest';
import { extractTokenBreakdownFromInfo, readSessions } from '../../src/main/services/sessionReader';

const tempDirs: string[] = [];

function createTempDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'codex-usage-session-reader-'));
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

describe('sessionReader token breakdown', () => {
  it('extracts cumulative token usage from token_count info', () => {
    const breakdown = extractTokenBreakdownFromInfo({
      total_token_usage: {
        input_tokens: 1200,
        cached_input_tokens: 900,
        output_tokens: 80,
        reasoning_output_tokens: 20,
        total_tokens: 1280,
      },
      last_token_usage: {
        input_tokens: 200,
        cached_input_tokens: 100,
        output_tokens: 10,
        reasoning_output_tokens: 5,
        total_tokens: 210,
      },
    });

    expect(breakdown).toMatchObject({
      total: 1280,
      input: 1200,
      cached: 900,
      output: 80,
      reasoning: 20,
      cacheHitRate: 0.75,
    });
  });

  it('reads token_count from session jsonl files', async () => {
    const root = createTempDir();
    const sessionsDir = path.join(root, 'sessions');
    const archivedDir = path.join(root, 'archived_sessions');
    fs.mkdirSync(sessionsDir, { recursive: true });
    fs.mkdirSync(archivedDir, { recursive: true });

    const filePath = path.join(sessionsDir, 'rollout-1.jsonl');
    fs.writeFileSync(
      filePath,
      [
        JSON.stringify({
          timestamp: '2026-04-23T01:00:00.000Z',
          type: 'session_meta',
          payload: { id: 'thread-1', cwd: 'C:\\base', model: 'gpt-5.4' },
        }),
        JSON.stringify({
          timestamp: '2026-04-23T01:00:10.000Z',
          type: 'event_msg',
          payload: {
            type: 'token_count',
            info: {
              total_token_usage: {
                input_tokens: 1000,
                cached_input_tokens: 600,
                output_tokens: 50,
                reasoning_output_tokens: 15,
                total_tokens: 1050,
              },
            },
          },
        }),
        JSON.stringify({
          timestamp: '2026-04-23T01:00:20.000Z',
          type: 'event_msg',
          payload: {
            type: 'token_count',
            info: {
              total_token_usage: {
                input_tokens: 1500,
                cached_input_tokens: 900,
                output_tokens: 80,
                reasoning_output_tokens: 20,
                total_tokens: 1580,
              },
            },
          },
        }),
      ].join('\n'),
      'utf8',
    );

    const result = await readSessions(sessionsDir, archivedDir, true, 30);
    const session = result.sessionsById.get('thread-1');

    expect(session?.tokenBreakdown).toMatchObject({
      total: 1580,
      input: 1500,
      cached: 900,
      output: 80,
      reasoning: 20,
      cacheHitRate: 0.6,
    });
  });

  it('reuses cached session timing when file size and mtime are unchanged', async () => {
    const root = createTempDir();
    const sessionsDir = path.join(root, 'sessions');
    const archivedDir = path.join(root, 'archived_sessions');
    const cachePath = path.join(root, 'session-cache.json');
    fs.mkdirSync(sessionsDir, { recursive: true });
    fs.mkdirSync(archivedDir, { recursive: true });

    const filePath = path.join(sessionsDir, 'rollout-cache.jsonl');
    fs.writeFileSync(
      filePath,
      [
        JSON.stringify({
          timestamp: '2026-04-23T01:00:00.000Z',
          type: 'session_meta',
          payload: { id: 'thread-cache', cwd: 'C:\\base', model: 'gpt-5.4' },
        }),
        JSON.stringify({
          timestamp: '2026-04-23T01:00:10.000Z',
          type: 'event_msg',
          payload: { started_at: '2026-04-23T01:00:10.000Z' },
        }),
      ].join('\n'),
      'utf8',
    );
    const fixedTime = new Date('2026-04-23T01:01:00.000Z');
    fs.utimesSync(filePath, fixedTime, fixedTime);

    const first = await readSessions(sessionsDir, archivedDir, true, 30, { cachePath });
    expect(first.sessionsById.get('thread-cache')?.activeMs).toBe(10000);

    const originalSize = fs.statSync(filePath).size;
    fs.writeFileSync(filePath, 'x'.repeat(originalSize), 'utf8');
    fs.utimesSync(filePath, fixedTime, fixedTime);

    const second = await readSessions(sessionsDir, archivedDir, true, 30, { cachePath });
    expect(second.sessionsById.get('thread-cache')?.activeMs).toBe(10000);
    expect(second.warnings).toEqual([]);
  });
});
