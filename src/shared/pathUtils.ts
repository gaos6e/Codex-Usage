import type { PathAliasRule } from './contracts';

export const ALL_WORKSPACES_ID = 'all';

export function normalizeWorkspacePath(input: string, aliases: PathAliasRule[] = []): string {
  let value = String(input || '').trim();
  if (!value) {
    return '(unknown)';
  }

  value = value.replace(/^\\\\\?\\/, '');
  value = value.replace(/\//g, '\\');
  value = value.replace(/\\+$/g, '');

  if (/^[a-zA-Z]:/.test(value)) {
    value = value.charAt(0).toUpperCase() + value.slice(1);
  }

  for (const alias of aliases) {
    if (!alias.enabled || !alias.from.trim()) {
      continue;
    }
    const from = normalizeWorkspacePath(alias.from, []);
    const to = normalizeWorkspacePath(alias.to || alias.from, []);
    if (value.toLowerCase() === from.toLowerCase()) {
      value = to;
    }
  }

  return value || '(unknown)';
}

export function workspaceIdFromPath(normalizedPath: string): string {
  return Buffer.from(normalizedPath.toLowerCase(), 'utf8')
    .toString('base64')
    .replace(/=/g, '')
    .replace(/\+/g, '-')
    .replace(/\//g, '_');
}

export function friendlyWorkspaceName(normalizedPath: string): string {
  if (!normalizedPath || normalizedPath === '(unknown)') {
    return 'Unknown workspace';
  }
  const parts = normalizedPath.split(/[\\/]/).filter(Boolean);
  const name = parts[parts.length - 1];
  return name || normalizedPath;
}

export function isWorkspaceIgnored(normalizedPath: string, ignored: string[] = []): boolean {
  const lower = normalizedPath.toLowerCase();
  return ignored
    .map((entry) => normalizeWorkspacePath(entry, []).toLowerCase())
    .some((entry) => lower === entry || lower.startsWith(`${entry}\\`));
}

export function anonymizePath(normalizedPath: string, index: number): string {
  if (normalizedPath === '(unknown)') {
    return normalizedPath;
  }
  return `Workspace ${index + 1}`;
}
