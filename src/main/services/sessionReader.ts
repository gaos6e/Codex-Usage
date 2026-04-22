import fs from 'fs';
import path from 'path';
import readline from 'readline';
import type { DiagnosticWarning } from '../../shared/contracts';
import { listJsonlFiles } from './sourceDetector';

export interface SessionTiming {
  id: string;
  filePath: string;
  archived: boolean;
  cwd?: string;
  model?: string;
  start?: Date;
  end?: Date;
  activeMs?: number;
  parseWarnings: number;
}

export interface SessionReadResult {
  sessionsById: Map<string, SessionTiming>;
  sessionsByFileStem: Map<string, SessionTiming>;
  warnings: DiagnosticWarning[];
  filesRead: number;
}

function parseTimestamp(value: unknown): Date | null {
  if (typeof value !== 'string') {
    return null;
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

function addActiveGap(timestamps: Date[], idleCapMs: number): number {
  if (timestamps.length <= 1) {
    return 0;
  }
  const sorted = timestamps.map((date) => date.getTime()).sort((a, b) => a - b);
  let total = 0;
  for (let index = 1; index < sorted.length; index += 1) {
    total += Math.min(Math.max(0, sorted[index] - sorted[index - 1]), idleCapMs);
  }
  return total;
}

async function readSessionFile(filePath: string, archived: boolean, idleCapMs: number): Promise<SessionTiming> {
  const timestamps: Date[] = [];
  let id = path.basename(filePath, '.jsonl');
  let cwd: string | undefined;
  let model: string | undefined;
  let parseWarnings = 0;

  const stream = fs.createReadStream(filePath, { encoding: 'utf8' });
  const reader = readline.createInterface({ input: stream, crlfDelay: Infinity });

  for await (const line of reader) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    try {
      const parsed = JSON.parse(trimmed);
      const timestamp = parseTimestamp(parsed.timestamp);
      if (timestamp) {
        timestamps.push(timestamp);
      }

      if (parsed.type === 'session_meta' && parsed.payload) {
        id = String(parsed.payload.id || id);
        cwd = parsed.payload.cwd ? String(parsed.payload.cwd) : cwd;
        model = parsed.payload.model ? String(parsed.payload.model) : model;
      }

      if (parsed.type === 'event_msg' && parsed.payload?.started_at) {
        const startedAt = parseTimestamp(parsed.payload.started_at);
        if (startedAt) {
          timestamps.push(startedAt);
        }
      }
    } catch {
      parseWarnings += 1;
    }
  }

  const sorted = timestamps.sort((a, b) => a.getTime() - b.getTime());
  return {
    id,
    filePath,
    archived,
    cwd,
    model,
    start: sorted[0],
    end: sorted[sorted.length - 1],
    activeMs: addActiveGap(sorted, idleCapMs),
    parseWarnings,
  };
}

export async function readSessions(
  sessionsDir: string,
  archivedSessionsDir: string,
  includeArchived: boolean,
  idleCapMinutes: number,
): Promise<SessionReadResult> {
  const idleCapMs = idleCapMinutes * 60 * 1000;
  const files = [
    ...listJsonlFiles(sessionsDir).map((filePath) => ({ filePath, archived: false })),
    ...(includeArchived ? listJsonlFiles(archivedSessionsDir).map((filePath) => ({ filePath, archived: true })) : []),
  ];

  const sessionsById = new Map<string, SessionTiming>();
  const sessionsByFileStem = new Map<string, SessionTiming>();
  const warnings: DiagnosticWarning[] = [];

  for (const file of files) {
    try {
      const session = await readSessionFile(file.filePath, file.archived, idleCapMs);
      const existing = sessionsById.get(session.id);
      if (!existing || (!existing.archived && session.archived)) {
        sessionsById.set(session.id, session);
      }
      sessionsByFileStem.set(path.basename(file.filePath, '.jsonl'), session);
      if (session.parseWarnings > 0) {
        warnings.push({
          code: 'parse_warning',
          severity: 'warning',
          messageKey: 'warning.jsonlLinesCouldNotBeParsed',
          messageArgs: { count: session.parseWarnings },
          source: file.filePath,
        });
      }
    } catch (error) {
      warnings.push({
        code: 'parse_warning',
        severity: 'warning',
        messageKey: 'warning.errorDetail',
        messageArgs: { detail: error instanceof Error ? error.message : String(error) },
        source: file.filePath,
      });
    }
  }

  return { sessionsById, sessionsByFileStem, warnings, filesRead: files.length };
}
