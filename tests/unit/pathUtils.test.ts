import { describe, expect, it } from 'vitest';
import { anonymizePath, friendlyWorkspaceName, normalizeWorkspacePath, workspaceIdFromPath } from '../../src/shared/pathUtils';

describe('path utilities', () => {
  it('normalizes Windows extended paths, separators, drive casing, and trailing slashes', () => {
    expect(normalizeWorkspacePath('\\\\?\\c:/base/project/')).toBe('C:\\base\\project');
  });

  it('applies enabled aliases after normalization', () => {
    expect(normalizeWorkspacePath('\\\\?\\C:\\base\\', [
      { id: '1', from: 'C:\\base', to: 'D:\\work\\base', enabled: true },
    ])).toBe('D:\\work\\base');
  });

  it('derives stable workspace ids and display names without exposing content', () => {
    const normalized = 'D:\\repo\\codex-usage';
    expect(workspaceIdFromPath(normalized)).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(friendlyWorkspaceName(normalized)).toBe('codex-usage');
    expect(anonymizePath(normalized, 2)).toBe('Workspace 3');
  });
});
