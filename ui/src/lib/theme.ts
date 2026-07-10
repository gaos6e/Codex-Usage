import { getCurrentWindow } from '@tauri-apps/api/window';
import { readLocalPreference, storageKeys, writeLocalPreference } from './storage';

export type ThemePreference = 'system' | 'dark' | 'light';

export function readThemePreference(): ThemePreference {
  const stored = readLocalPreference(storageKeys.theme);
  return stored === 'dark' || stored === 'light' || stored === 'system' ? stored : 'system';
}

export function resolveTheme(preference: ThemePreference): 'dark' | 'light' {
  if (preference !== 'system') return preference;
  return typeof window.matchMedia === 'function'
    && window.matchMedia('(prefers-color-scheme: dark)').matches
    ? 'dark'
    : 'light';
}

export function applyThemePreference(preference: ThemePreference): void {
  writeLocalPreference(storageKeys.theme, preference);
  document.documentElement.dataset.theme = resolveTheme(preference);
  if ('__TAURI_INTERNALS__' in window) {
    void getCurrentWindow().setTheme(preference === 'system' ? null : preference);
  }
}

export function initializeTheme(): void {
  applyThemePreference(readThemePreference());
  if (typeof window.matchMedia !== 'function') return;
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (readThemePreference() === 'system') applyThemePreference('system');
  });
}
