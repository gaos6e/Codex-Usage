import fs from 'fs';
import Database from 'better-sqlite3';
import type { AppSettings, DiagnosticWarning } from '../../shared/contracts';
import {
  friendlyWorkspaceName,
  isWorkspaceIgnored,
  normalizeWorkspacePath,
  workspaceIdFromPath,
} from '../../shared/pathUtils';

export interface ThreadRecord {
  id: string;
  rolloutPath?: string;
  createdAt: Date;
  updatedAt: Date;
  cwd: string;
  normalizedPath: string;
  workspaceId: string;
  workspaceName: string;
  title: string;
  modelProvider: string;
  model: string;
  tokensUsed: number;
  archived: boolean;
  hidden: boolean;
}

export interface StateReadResult {
  threads: ThreadRecord[];
  warnings: DiagnosticWarning[];
  columns: string[];
  rows: number;
}

function dateFromFields(msValue?: number | null, secondsValue?: number | null): Date {
  if (typeof msValue === 'number' && msValue > 0) {
    return new Date(msValue);
  }
  if (typeof secondsValue === 'number' && secondsValue > 0) {
    return new Date(secondsValue * 1000);
  }
  return new Date(0);
}

function optionalColumn(columns: string[], name: string, fallback = "''"): string {
  return columns.includes(name) ? name : `${fallback} as ${name}`;
}

export function readStateDb(dbPath: string, settings: AppSettings): StateReadResult {
  const warnings: DiagnosticWarning[] = [];
  if (!fs.existsSync(dbPath)) {
    return {
      threads: [],
      warnings: [{
        code: 'missing_source',
        severity: 'error',
        messageKey: 'warning.stateDbMissing',
        source: dbPath,
      }],
      columns: [],
      rows: 0,
    };
  }

  try {
    const db = new Database(dbPath, { readonly: true, fileMustExist: true });
    db.pragma('query_only = ON');
    const table = db.prepare("select name from sqlite_master where type='table' and name='threads'").get();
    if (!table) {
      db.close();
      return {
        threads: [],
        warnings: [{
          code: 'schema_mismatch',
          severity: 'error',
          messageKey: 'warning.threadsTableMissing',
          source: dbPath,
        }],
        columns: [],
        rows: 0,
      };
    }

    const columns = db.prepare('pragma table_info(threads)').all().map((row: any) => String(row.name));
    const required = ['id', 'cwd', 'created_at', 'updated_at', 'tokens_used'];
    const missing = required.filter((column) => !columns.includes(column));
    if (missing.length) {
      warnings.push({
        code: 'schema_mismatch',
        severity: 'error',
        messageKey: 'warning.requiredColumnsMissing',
        messageArgs: { columns: missing.join(', ') },
        source: dbPath,
      });
    }

    const select = [
      optionalColumn(columns, 'id'),
      optionalColumn(columns, 'rollout_path'),
      optionalColumn(columns, 'created_at', '0'),
      optionalColumn(columns, 'updated_at', '0'),
      optionalColumn(columns, 'created_at_ms', '0'),
      optionalColumn(columns, 'updated_at_ms', '0'),
      optionalColumn(columns, 'cwd'),
      optionalColumn(columns, 'title'),
      optionalColumn(columns, 'model_provider'),
      optionalColumn(columns, 'model'),
      optionalColumn(columns, 'tokens_used', '0'),
      optionalColumn(columns, 'archived', '0'),
    ].join(', ');

    const rows = db.prepare(`select ${select} from threads`).all() as any[];
    const threads = rows
      .map((row, index): ThreadRecord => {
        const normalizedPath = normalizeWorkspacePath(row.cwd || '(unknown)', settings.aliases);
        const workspaceId = workspaceIdFromPath(normalizedPath);
        return {
          id: String(row.id || `thread-${index}`),
          rolloutPath: row.rollout_path ? String(row.rollout_path) : undefined,
          createdAt: dateFromFields(Number(row.created_at_ms), Number(row.created_at)),
          updatedAt: dateFromFields(Number(row.updated_at_ms), Number(row.updated_at)),
          cwd: String(row.cwd || '(unknown)'),
          normalizedPath,
          workspaceId,
          workspaceName: friendlyWorkspaceName(normalizedPath),
          title: String(row.title || 'Untitled run'),
          modelProvider: String(row.model_provider || 'unknown'),
          model: String(row.model || 'unknown'),
          tokensUsed: Number(row.tokens_used || 0),
          archived: Boolean(row.archived),
          hidden: isWorkspaceIgnored(normalizedPath, settings.ignoredWorkspaces),
        };
      });

    db.close();
    return { threads, warnings, columns, rows: rows.length };
  } catch (error) {
    return {
      threads: [],
      warnings: [{
        code: 'schema_mismatch',
        severity: 'error',
        messageKey: 'warning.errorDetail',
        messageArgs: { detail: error instanceof Error ? error.message : String(error) },
        source: dbPath,
      }],
      columns: [],
      rows: 0,
    };
  }
}
