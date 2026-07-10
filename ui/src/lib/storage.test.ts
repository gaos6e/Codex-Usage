import { beforeEach, describe, expect, it } from 'vitest';
import { readLocalPreference, storageKeys, writeLocalPreference } from './storage';

describe('Chronolume local preference migration', () => {
  beforeEach(() => localStorage.clear());

  it('moves a legacy Codex Usage key into the Chronolume namespace', () => {
    localStorage.setItem('codex-usage.language', 'en');

    expect(readLocalPreference(storageKeys.language)).toBe('en');
    expect(localStorage.getItem(storageKeys.language)).toBe('en');
    expect(localStorage.getItem('codex-usage.language')).toBeNull();
  });

  it('keeps an existing Chronolume value authoritative', () => {
    localStorage.setItem('codex-usage.theme', 'light');
    writeLocalPreference(storageKeys.theme, 'dark');

    expect(readLocalPreference(storageKeys.theme)).toBe('dark');
    expect(localStorage.getItem('codex-usage.theme')).toBe('light');
  });
});
