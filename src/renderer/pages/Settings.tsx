import React, { useEffect, useState } from 'react';
import { FolderOpen, Plus, Save, Trash2 } from 'lucide-react';
import type { AppSettings } from '../../shared/contracts';
import { useI18n } from '../i18n/I18nContext';

interface Props {
  settings: AppSettings | null;
  onSave: (settings: AppSettings) => Promise<void>;
}

export function Settings({ settings, onSave }: Props): React.ReactElement {
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  const [status, setStatus] = useState('');
  const { t } = useI18n();

  useEffect(() => setDraft(settings), [settings]);

  if (!draft) {
    return <main className="content-pane"><div className="loading-row">{t('settings.title')}…</div></main>;
  }

  const update = (patch: Partial<AppSettings>) => setDraft({ ...draft, ...patch });

  const save = async () => {
    await onSave(draft);
    setStatus(t('settings.saved'));
  };

  const chooseDirectory = async () => {
    const directory = await window.codexUsage.chooseCodexDirectory();
    if (directory) {
      update({ codexDir: directory });
    }
  };

  return (
    <main className="content-pane settings-pane">
      <div className="content-header">
        <div>
          <h1>{t('settings.title')}</h1>
          <p>{t('settings.subtitle')}</p>
        </div>
        <button className="primary-button" onClick={save}><Save size={15} /> {t('settings.save')}</button>
      </div>

      {status ? <div className="success-banner" role="status">{status}</div> : null}

      <section className="settings-group">
        <h2>{t('settings.dataSources')}</h2>
        <label>
          {t('settings.codexDir')}
          <div className="path-picker">
            <input value={draft.codexDir} onChange={(event) => update({ codexDir: event.target.value })} />
            <button className="secondary-button" onClick={chooseDirectory} title={t('settings.chooseCodexDir')}>
              <FolderOpen size={15} /> {t('settings.choose')}
            </button>
          </div>
        </label>
        <label className="checkbox-row">
          <input type="checkbox" checked={draft.includeArchivedSessions} onChange={(event) => update({ includeArchivedSessions: event.target.checked })} />
          {t('settings.includeArchived')}
        </label>
        <label className="checkbox-row">
          <input type="checkbox" checked={draft.includeDetailedLogs} onChange={(event) => update({ includeDetailedLogs: event.target.checked })} />
          {t('settings.includeDetailedLogs')}
        </label>
      </section>

      <section className="settings-group">
        <h2>{t('settings.refreshAndTime')}</h2>
        <div className="settings-grid">
          <label>
            {t('settings.autoRefreshSeconds')}
            <input type="number" min={10} max={3600} value={draft.autoRefreshSeconds} onChange={(event) => update({ autoRefreshSeconds: Number(event.target.value) })} />
          </label>
          <label>
            {t('settings.idleGapMinutes')}
            <input type="number" min={1} max={180} value={draft.idleGapMinutes} onChange={(event) => update({ idleGapMinutes: Number(event.target.value) })} />
          </label>
          <label>
            {t('settings.theme')}
            <select value={draft.theme} onChange={(event) => update({ theme: event.target.value as AppSettings['theme'] })}>
              <option value="system">{t('settings.themeSystem')}</option>
              <option value="light">{t('settings.themeLight')}</option>
              <option value="dark">{t('settings.themeDark')}</option>
            </select>
          </label>
          <label>
            {t('settings.language')}
            <select value={draft.language} onChange={(event) => update({ language: event.target.value as AppSettings['language'] })}>
              <option value="zh-CN">{t('settings.languageZhCN')}</option>
              <option value="en">{t('settings.languageEn')}</option>
            </select>
          </label>
        </div>
      </section>

      <section className="settings-group">
        <div className="section-heading">
          <h2>{t('settings.pathAliases')}</h2>
          <button className="secondary-button" onClick={() => update({ aliases: [...draft.aliases, { id: `alias-${Date.now()}`, from: '', to: '', enabled: true }] })}>
            <Plus size={15} /> {t('settings.addAlias')}
          </button>
        </div>
        <p className="help-text">{t('settings.aliasHelp')}</p>
        <div className="alias-list">
          {draft.aliases.map((alias, index) => (
            <div className="alias-row" key={alias.id}>
              <input aria-label={t('settings.aliasFrom')} placeholder={t('settings.aliasFrom')} value={alias.from} onChange={(event) => {
                const aliases = [...draft.aliases];
                aliases[index] = { ...alias, from: event.target.value };
                update({ aliases });
              }} />
              <input aria-label={t('settings.aliasTo')} placeholder={t('settings.aliasTo')} value={alias.to} onChange={(event) => {
                const aliases = [...draft.aliases];
                aliases[index] = { ...alias, to: event.target.value };
                update({ aliases });
              }} />
              <label className="checkbox-row">
                <input type="checkbox" checked={alias.enabled} onChange={(event) => {
                  const aliases = [...draft.aliases];
                  aliases[index] = { ...alias, enabled: event.target.checked };
                  update({ aliases });
                }} />
                {t('settings.enabled')}
              </label>
              <button className="icon-button" title={t('settings.removeAlias')} aria-label={t('settings.removeAlias')} onClick={() => update({ aliases: draft.aliases.filter((item) => item.id !== alias.id) })}>
                <Trash2 size={16} />
              </button>
            </div>
          ))}
        </div>
      </section>

      <section className="settings-group">
        <h2>{t('settings.privacy')}</h2>
        <p className="help-text">{t('settings.privacyHelp')}</p>
      </section>
    </main>
  );
}
