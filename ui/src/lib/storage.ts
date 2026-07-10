export const storageKeys = {
  theme: 'chronolume.theme',
  language: 'chronolume.language',
  fontScale: 'chronolume.font-scale',
  trendSeries: 'chronolume.trend-series.v1',
} as const;

const legacyStorageKeys: Record<string, string> = {
  [storageKeys.theme]: 'codex-usage.theme',
  [storageKeys.language]: 'codex-usage.language',
  [storageKeys.fontScale]: 'codex-usage.font-scale',
  [storageKeys.trendSeries]: 'codex-usage.trend-series.v1',
};

export function readLocalPreference(key: string): string | null {
  const current = localStorage.getItem(key);
  if (current !== null) return current;

  const legacyKey = legacyStorageKeys[key];
  if (!legacyKey) return null;
  const legacy = localStorage.getItem(legacyKey);
  if (legacy === null) return null;

  localStorage.setItem(key, legacy);
  localStorage.removeItem(legacyKey);
  return legacy;
}

export function writeLocalPreference(key: string, value: string): void {
  localStorage.setItem(key, value);
}
