import { afterEach, describe, expect, it } from 'vitest';
import i18n from './i18n';

describe('i18n', () => {
  afterEach(async () => { await i18n.changeLanguage('zh-CN'); });

  it('switches core navigation and privacy copy between Chinese and English', async () => {
    await i18n.changeLanguage('en');
    expect(i18n.t('本地用量总览')).toBe('Local usage overview');
    expect(i18n.t('匿名路径')).toBe('Anonymous paths');
    await i18n.changeLanguage('zh-CN');
    expect(i18n.t('本地用量总览')).toBe('本地用量总览');
  });
});
