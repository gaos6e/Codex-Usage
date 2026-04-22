import fs from 'fs';
import os from 'os';
import path from 'path';
import type { AppSettings } from '../../shared/contracts';
import { normalizeWorkspacePath } from '../../shared/pathUtils';
import { clampNumber } from '../../shared/formatting';
import type { AppPaths } from './appPaths';

export function createDefaultSettings(): AppSettings {
  return {
    codexDir: path.join(os.homedir(), '.codex'),
    includeArchivedSessions: true,
    includeDetailedLogs: true,
    autoRefreshSeconds: 60,
    idleGapMinutes: 30,
    theme: 'system',
    language: 'zh-CN',
    aliases: [],
    ignoredWorkspaces: [normalizeWorkspacePath('C:\\Windows\\System32')],
  };
}

export class SettingsStore {
  constructor(private readonly paths: AppPaths) {}

  load(): AppSettings {
    const defaults = createDefaultSettings();
    if (!fs.existsSync(this.paths.settingsPath)) {
      this.save(defaults);
      return defaults;
    }

    try {
      const parsed = JSON.parse(fs.readFileSync(this.paths.settingsPath, 'utf8')) as Partial<AppSettings>;
      return this.validate({ ...defaults, ...parsed });
    } catch {
      return defaults;
    }
  }

  save(settings: AppSettings): AppSettings {
    const validated = this.validate(settings);
    fs.writeFileSync(this.paths.settingsPath, JSON.stringify(validated, null, 2), 'utf8');
    return validated;
  }

  validate(settings: AppSettings): AppSettings {
    const defaults = createDefaultSettings();
    const theme = settings.theme === 'dark' || settings.theme === 'light' || settings.theme === 'system'
      ? settings.theme
      : defaults.theme;

    return {
      codexDir: settings.codexDir || defaults.codexDir,
      includeArchivedSessions: Boolean(settings.includeArchivedSessions),
      includeDetailedLogs: Boolean(settings.includeDetailedLogs),
      autoRefreshSeconds: clampNumber(Number(settings.autoRefreshSeconds) || defaults.autoRefreshSeconds, 10, 3600),
      idleGapMinutes: clampNumber(Number(settings.idleGapMinutes) || defaults.idleGapMinutes, 1, 180),
      theme,
      language: settings.language === 'en' ? 'en' : 'zh-CN',
      aliases: Array.isArray(settings.aliases)
        ? settings.aliases.map((alias, index) => ({
            id: alias.id || `alias-${index}`,
            from: alias.from || '',
            to: alias.to || '',
            enabled: alias.enabled !== false,
          }))
        : [],
      ignoredWorkspaces: Array.isArray(settings.ignoredWorkspaces)
        ? settings.ignoredWorkspaces.map((item) => normalizeWorkspacePath(item))
        : defaults.ignoredWorkspaces,
    };
  }
}
