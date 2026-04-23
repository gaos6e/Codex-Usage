import fs from 'fs';
import Database from 'better-sqlite3';
import type { DiagnosticWarning, TokenBreakdown } from '../../shared/contracts';
import { cacheHitRateForBreakdown } from '../../shared/usageMath';

export interface LogsReadResult {
  byThreadId: Map<string, TokenBreakdown>;
  warnings: DiagnosticWarning[];
  rows: number;
  columns: string[];
}

function addBreakdown(target: TokenBreakdown, source: TokenBreakdown): void {
  target.input = (target.input || 0) + (source.input || 0);
  target.output = (target.output || 0) + (source.output || 0);
  target.cached = (target.cached || 0) + (source.cached || 0);
  target.reasoning = (target.reasoning || 0) + (source.reasoning || 0);
}

function extractTokenFields(value: unknown, result: TokenBreakdown = {}): TokenBreakdown {
  if (!value || typeof value !== 'object') {
    return result;
  }
  for (const [key, nested] of Object.entries(value as Record<string, unknown>)) {
    const normalized = key.toLowerCase();
    if (typeof nested === 'number') {
      if (normalized.includes('input') && normalized.includes('token')) result.input = (result.input || 0) + nested;
      if (normalized.includes('output') && normalized.includes('token')) result.output = (result.output || 0) + nested;
      if (normalized.includes('cached') && normalized.includes('token')) result.cached = (result.cached || 0) + nested;
      if (normalized.includes('reasoning') && normalized.includes('token')) result.reasoning = (result.reasoning || 0) + nested;
    } else if (nested && typeof nested === 'object') {
      extractTokenFields(nested, result);
    } else if (typeof nested === 'string' && nested.length > 1 && (nested.startsWith('{') || nested.startsWith('['))) {
      try {
        extractTokenFields(JSON.parse(nested), result);
      } catch {
        // Best-effort only.
      }
    }
  }
  return result;
}

type NumericBreakdownKey = 'input' | 'output' | 'cached' | 'reasoning';

function addTokenValue(result: TokenBreakdown, key: NumericBreakdownKey, value: number): void {
  result[key] = (result[key] || 0) + value;
}

function getNumberField(value: unknown, key: string): number {
  if (!value || typeof value !== 'object') {
    return 0;
  }
  const record = value as Record<string, unknown>;
  return typeof record[key] === 'number' ? record[key] : 0;
}

function extractBreakdownFromUsage(usage: unknown): TokenBreakdown {
  if (!usage || typeof usage !== 'object') {
    return {};
  }
  const record = usage as Record<string, unknown>;
  const inputDetails = record.input_tokens_details;
  const outputDetails = record.output_tokens_details;
  const breakdown: TokenBreakdown = {
    input: getNumberField(record, 'input_tokens'),
    output: getNumberField(record, 'output_tokens'),
    cached: getNumberField(inputDetails, 'cached_tokens'),
    reasoning: getNumberField(outputDetails, 'reasoning_tokens'),
  };
  breakdown.cacheHitRate = cacheHitRateForBreakdown(breakdown);
  return breakdown;
}

function parseEmbeddedJson(body: string): unknown {
  const jsonStart = body.indexOf('{');
  if (jsonStart < 0) {
    return undefined;
  }
  try {
    return JSON.parse(body.slice(jsonStart));
  } catch {
    return undefined;
  }
}

function extractBreakdownFromStructuredBody(body: string): TokenBreakdown {
  const trimmed = body.trim();
  let parsed: unknown;
  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    try {
      parsed = JSON.parse(trimmed);
    } catch {
      parsed = undefined;
    }
  } else {
    parsed = parseEmbeddedJson(body);
  }

  if (!parsed || typeof parsed !== 'object') {
    return {};
  }

  const event = parsed as Record<string, unknown>;
  const response = event.response;
  if (response && typeof response === 'object') {
    const usage = (response as Record<string, unknown>).usage;
    const breakdown = extractBreakdownFromUsage(usage);
    if (breakdown.input || breakdown.output || breakdown.cached || breakdown.reasoning) {
      return breakdown;
    }
  }

  const directBreakdown = extractBreakdownFromUsage(event.usage);
  if (directBreakdown.input || directBreakdown.output || directBreakdown.cached || directBreakdown.reasoning) {
    return directBreakdown;
  }

  return extractTokenFields(parsed);
}

function addRegexMatches(result: TokenBreakdown, key: NumericBreakdownKey, regex: RegExp, body: string): void {
  for (const match of body.matchAll(regex)) {
    addTokenValue(result, key, Number(match[1]));
  }
}

export function extractBreakdownFromText(body: string): TokenBreakdown {
  let result: TokenBreakdown = {};
  const structured = extractBreakdownFromStructuredBody(body);
  if (structured.input || structured.output || structured.cached || structured.reasoning) {
    result = structured;
  }

  const regexMap: Array<[NumericBreakdownKey, RegExp]> = [
    ['input', /input[_-]?tokens?["'\s:=]+(\d+)/gi],
    ['output', /output[_-]?tokens?["'\s:=]+(\d+)/gi],
    ['cached', /cached[_-]?tokens?["'\s:=]+(\d+)/gi],
    ['reasoning', /reasoning[_-]?tokens?["'\s:=]+(\d+)/gi],
  ];
  if (!result.input && !result.output && !result.cached && !result.reasoning) {
    for (const [key, regex] of regexMap) {
      addRegexMatches(result, key, regex, body);
    }
  }
  result.cacheHitRate = cacheHitRateForBreakdown(result);
  return result;
}

export function readLogsDb(dbPath: string, enabled: boolean): LogsReadResult {
  const warnings: DiagnosticWarning[] = [];
  if (!enabled) {
    return {
      byThreadId: new Map(),
      warnings: [{
        code: 'partial_data',
        severity: 'info',
        messageKey: 'warning.detailedLogsDisabled',
        source: dbPath,
      }],
      rows: 0,
      columns: [],
    };
  }
  if (!fs.existsSync(dbPath)) {
    return {
      byThreadId: new Map(),
      warnings: [{
        code: 'missing_source',
        severity: 'warning',
        messageKey: 'warning.logsDbMissing',
        source: dbPath,
      }],
      rows: 0,
      columns: [],
    };
  }

  try {
    const db = new Database(dbPath, { readonly: true, fileMustExist: true });
    db.pragma('query_only = ON');
    const table = db.prepare("select name from sqlite_master where type='table' and name='logs'").get();
    if (!table) {
      db.close();
      return {
        byThreadId: new Map(),
        warnings: [{
          code: 'schema_mismatch',
          severity: 'warning',
          messageKey: 'warning.logsTableMissing',
          source: dbPath,
        }],
        rows: 0,
        columns: [],
      };
    }

    const columns = db.prepare('pragma table_info(logs)').all().map((row: any) => String(row.name));
    const rows = db.prepare('select count(*) as count from logs').get() as { count: number };
    const byThreadId = new Map<string, TokenBreakdown>();

    if (!columns.includes('thread_id') || !columns.includes('feedback_log_body')) {
      warnings.push({
        code: 'schema_mismatch',
        severity: 'warning',
        messageKey: 'warning.logsColumnsMissing',
        source: dbPath,
      });
      db.close();
      return { byThreadId, warnings, rows: rows.count, columns };
    }

    const records = db.prepare(
      'select thread_id, feedback_log_body from logs where thread_id is not null and feedback_log_body is not null',
    ).all() as Array<{ thread_id: string; feedback_log_body: string }>;

    let parsed = 0;
    for (const record of records) {
      const breakdown = extractBreakdownFromText(String(record.feedback_log_body || ''));
      if (breakdown.input || breakdown.output || breakdown.cached || breakdown.reasoning) {
        parsed += 1;
        const current = byThreadId.get(String(record.thread_id)) || {};
        addBreakdown(current, breakdown);
        byThreadId.set(String(record.thread_id), current);
      }
    }

    for (const breakdown of byThreadId.values()) {
      breakdown.cacheHitRate = cacheHitRateForBreakdown(breakdown);
    }

    if (parsed === 0) {
      warnings.push({
        code: 'partial_data',
        severity: 'warning',
        messageKey: 'warning.noStableTokenBreakdown',
        source: dbPath,
      });
    }

    db.close();
    return { byThreadId, warnings, rows: rows.count, columns };
  } catch (error) {
    return {
      byThreadId: new Map(),
      warnings: [{
        code: 'schema_mismatch',
        severity: 'warning',
        messageKey: 'warning.errorDetail',
        messageArgs: { detail: error instanceof Error ? error.message : String(error) },
        source: dbPath,
      }],
      rows: 0,
      columns: [],
    };
  }
}
