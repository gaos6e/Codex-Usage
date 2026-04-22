import fs from 'fs';
import path from 'path';
import Database from 'better-sqlite3';
import type { AppSettings, DiagnosticWarning, SourceStatus } from '../../shared/contracts';

export interface SourceDetection {
  stateDbPath: string;
  sessionsDir: string;
  archivedSessionsDir: string;
  logsDbPath: string;
  statuses: SourceStatus[];
  warnings: DiagnosticWarning[];
}

function safeStat(target: string): fs.Stats | null {
  try {
    return fs.statSync(target);
  } catch {
    return null;
  }
}

export function listJsonlFiles(root: string): string[] {
  const files: string[] = [];
  if (!safeStat(root)?.isDirectory()) {
    return files;
  }

  const visit = (dir: string) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        visit(fullPath);
      } else if (entry.isFile() && entry.name.toLowerCase().endsWith('.jsonl')) {
        files.push(fullPath);
      }
    }
  };

  visit(root);
  return files;
}

function inspectSqlite(dbPath: string, tableName: string): { rows?: number; columns?: string[]; earliest?: string; latest?: string; warning?: string } {
  if (!safeStat(dbPath)?.isFile()) {
    return {};
  }
  try {
    const db = new Database(dbPath, { readonly: true, fileMustExist: true });
    db.pragma('query_only = ON');
    const tables = db.prepare("select name from sqlite_master where type='table' and name = ?").all(tableName);
    if (!tables.length) {
      db.close();
      return { warning: `Table ${tableName} is missing` };
    }
    const columns = db.prepare(`pragma table_info(${tableName})`).all().map((row: any) => String(row.name));
    const rows = db.prepare(`select count(*) as count from ${tableName}`).get() as { count: number };
    let earliest: string | undefined;
    let latest: string | undefined;
    if (tableName === 'threads' && columns.includes('created_at')) {
      const range = db.prepare('select min(created_at) as minValue, max(updated_at) as maxValue from threads').get() as any;
      earliest = range?.minValue ? new Date(Number(range.minValue) * 1000).toISOString() : undefined;
      latest = range?.maxValue ? new Date(Number(range.maxValue) * 1000).toISOString() : undefined;
    }
    if (tableName === 'logs' && columns.includes('ts')) {
      const range = db.prepare('select min(ts) as minValue, max(ts) as maxValue from logs').get() as any;
      earliest = range?.minValue ? String(range.minValue) : undefined;
      latest = range?.maxValue ? String(range.maxValue) : undefined;
    }
    db.close();
    return { rows: rows.count, columns, earliest, latest };
  } catch (error) {
    return { warning: error instanceof Error ? error.message : String(error) };
  }
}

export function detectSources(settings: AppSettings): SourceDetection {
  const stateDbPath = path.join(settings.codexDir, 'state_5.sqlite');
  const sessionsDir = path.join(settings.codexDir, 'sessions');
  const archivedSessionsDir = path.join(settings.codexDir, 'archived_sessions');
  const logsDbPath = path.join(settings.codexDir, 'logs_2.sqlite');
  const warnings: DiagnosticWarning[] = [];

  const stateInspection = inspectSqlite(stateDbPath, 'threads');
  const logsInspection = inspectSqlite(logsDbPath, 'logs');
  const sessionFiles = listJsonlFiles(sessionsDir);
  const archivedFiles = settings.includeArchivedSessions ? listJsonlFiles(archivedSessionsDir) : [];

  const statuses: SourceStatus[] = [
    {
      key: 'state',
      label: 'state_5.sqlite',
      path: stateDbPath,
      exists: Boolean(safeStat(stateDbPath)?.isFile()),
      ...stateInspection,
    },
    {
      key: 'sessions',
      label: 'sessions JSONL',
      path: sessionsDir,
      exists: Boolean(safeStat(sessionsDir)?.isDirectory()),
      files: sessionFiles.length,
    },
    {
      key: 'archivedSessions',
      label: 'archived sessions JSONL',
      path: archivedSessionsDir,
      exists: Boolean(safeStat(archivedSessionsDir)?.isDirectory()),
      files: archivedFiles.length,
      warning: settings.includeArchivedSessions ? undefined : 'Disabled in settings',
    },
    {
      key: 'logs',
      label: 'logs_2.sqlite',
      path: logsDbPath,
      exists: Boolean(safeStat(logsDbPath)?.isFile()),
      ...logsInspection,
      warning: settings.includeDetailedLogs ? logsInspection.warning : 'Disabled in settings',
    },
  ];

  for (const status of statuses) {
    if (!status.exists && (status.key === 'state' || status.key === 'sessions')) {
      warnings.push({
        code: 'missing_source',
        severity: status.key === 'state' ? 'error' : 'warning',
        messageKey: 'warning.sourceNotFound',
        messageArgs: { label: status.label },
        source: status.path,
      });
    }
    if (status.warning) {
      warnings.push({
        code: status.warning.includes('missing') ? 'schema_mismatch' : 'partial_data',
        severity: status.key === 'state' ? 'error' : 'warning',
        messageKey: 'warning.sourceDetail',
        messageArgs: { label: status.label, detail: status.warning },
        source: status.path,
      });
    }
  }

  return { stateDbPath, sessionsDir, archivedSessionsDir, logsDbPath, statuses, warnings };
}
