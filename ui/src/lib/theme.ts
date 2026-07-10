import { getCurrentWindow } from '@tauri-apps/api/window';

export type ThemePreference = 'system' | 'dark' | 'light';

export function readThemePreference(): ThemePreference {
  const stored = localStorage.getItem('codex-usage.theme');
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
  localStorage.setItem('codex-usage.theme', preference);
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
