import { useEffect, useState } from 'react';
import { Languages, MonitorCog, Moon, Sun, Type } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { getAppPreferences, saveAppPreferences } from '../../api';
import { QueryStatus } from '../../components/QueryStatus';
import { applyThemePreference, readThemePreference, type ThemePreference } from '../../lib/theme';

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const [theme, setTheme] = useState<ThemePreference>(readThemePreference);
  const [language, setLanguage] = useState(() => localStorage.getItem('codex-usage.language') ?? 'zh-CN');
  const [fontScale, setFontScale] = useState(() => Number(localStorage.getItem('codex-usage.font-scale') ?? 1));
  const [idleGapMinutes, setIdleGapMinutes] = useState(30);
  const preferences = useQuery({ queryKey: ['app-preferences'], queryFn: getAppPreferences });
  const savePreferences = useMutation({
    mutationFn: () => saveAppPreferences({
      idleGapMinutes,
      visibleWorkspaceIds: preferences.data?.visibleWorkspaceIds ?? [],
    }),
    onSuccess: (saved) => queryClient.setQueryData(['app-preferences'], saved),
  });

  useEffect(() => {
    applyThemePreference(theme);
  }, [theme]);
  useEffect(() => {
    localStorage.setItem('codex-usage.language', language);
    document.documentElement.lang = language;
    void i18n.changeLanguage(language);
  }, [i18n, language]);
  useEffect(() => {
    const safeScale = Math.max(.9, Math.min(1.35, fontScale));
    localStorage.setItem('codex-usage.font-scale', String(safeScale));
    document.documentElement.style.fontSize = `${safeScale * 100}%`;
  }, [fontScale]);
  useEffect(() => {
    if (preferences.data) setIdleGapMinutes(preferences.data.idleGapMinutes);
  }, [preferences.data]);

  return <section className="feature-page settings-page">
    <QueryStatus loading={preferences.isLoading && !preferences.data} error={preferences.error} onRetry={() => void preferences.refetch()} />
    <section className="feature-card setting-row"><div><MonitorCog /><span><strong>{t('主题')}</strong><small>{t('暗色、浅色或跟随 Windows')}</small></span></div><div className="choice-buttons">
      <button type="button" className={theme === 'system' ? 'active' : ''} onClick={() => setTheme('system')}><MonitorCog />{t('系统')}</button>
      <button type="button" className={theme === 'dark' ? 'active' : ''} onClick={() => setTheme('dark')}><Moon />{t('暗色')}</button>
      <button type="button" className={theme === 'light' ? 'active' : ''} onClick={() => setTheme('light')}><Sun />{t('浅色')}</button>
    </div></section>
    <section className="feature-card setting-row"><div><Languages /><span><strong>{t('语言')}</strong><small>{t('界面语言偏好')}</small></span></div><select value={language} onChange={(event) => setLanguage(event.target.value)}><option value="zh-CN">简体中文</option><option value="en">English</option></select></section>
    <section className="feature-card setting-row"><div><Type /><span><strong>{t('字体缩放')}</strong><small>{Math.round(fontScale * 100)}%</small></span></div><input aria-label={t('字体缩放')} type="range" min="0.9" max="1.35" step="0.05" value={fontScale} onChange={(event) => setFontScale(Number(event.target.value))} /></section>
    <section className="feature-card setting-row"><div><MonitorCog /><span><strong>{t('活跃时间空闲间隔')}</strong><small>{t('缺少任务生命周期事件时用于估算；1–240 分钟')}</small></span></div><div className="setting-action"><input aria-label={t('活跃时间空闲间隔')} type="number" min="1" max="240" value={idleGapMinutes} onChange={(event) => setIdleGapMinutes(Number(event.target.value))} /><span>{t('分钟')}</span><button type="button" className="primary-button" disabled={savePreferences.isPending} onClick={() => savePreferences.mutate()}>{t('保存')}</button></div></section>
    <section className="feature-card about-card"><h2>Codex Usage 2.0</h2><p>{t('高性能、离线优先的 OpenAI Codex 本地用量分析。应用不会读取 auth.json、在线配额或对话正文；价格更新是唯一可选网络能力，且只在用户明确触发后执行。')}</p></section>
  </section>;
}
